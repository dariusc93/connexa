use crate::keystore::{EncryptedEntry, Error, KeyMetadata, Keystore, Result};
use std::path::{Path, PathBuf};

/// Filesystem [`Keystore`] backend.
#[derive(Debug, Clone)]
pub struct FilesystemKeystore {
    dir: PathBuf,
}

impl FilesystemKeystore {
    /// Point the keystore at `dir`; it is created on first write if absent.
    pub fn new(dir: impl AsRef<Path>) -> Self {
        Self {
            dir: dir.as_ref().to_path_buf(),
        }
    }

    fn entry_path(&self, label: &str) -> Result<PathBuf> {
        crate::keystore::validate_label(label)?;
        Ok(self.dir.join(label))
    }
}

impl Keystore for FilesystemKeystore {
    async fn put(&self, entry: EncryptedEntry) -> Result<()> {
        use tokio::io::AsyncWriteExt;

        let path = self.entry_path(&entry.metadata.label)?;
        let bytes = postcard::to_allocvec(&entry).map_err(|e| Error::Backend(e.to_string()))?;

        let tmp_dir = self.dir.join(".tmp");
        async {
            create_dir(&tmp_dir).await?;
            let tmp = tmp_dir.join(format!("{:016x}", rand::random::<u64>()));

            let mut file = create_file(&tmp).await?;
            file.write_all(&bytes).await?;
            file.sync_all().await?;
            drop(file);

            tokio::fs::rename(&tmp, &path).await
        }
        .await
        .map_err(|e| Error::Backend(e.to_string()))
    }

    async fn get(&self, label: &str) -> Result<Option<EncryptedEntry>> {
        let path = self.entry_path(label)?;
        match tokio::fs::read(&path).await {
            Ok(bytes) => Ok(Some(
                postcard::from_bytes(&bytes).map_err(|e| Error::Backend(e.to_string()))?,
            )),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(Error::Backend(e.to_string())),
        }
    }

    async fn list(&self) -> Result<Vec<KeyMetadata>> {
        let mut read_dir = match tokio::fs::read_dir(&self.dir).await {
            Ok(read_dir) => read_dir,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(Error::Backend(e.to_string())),
        };

        let mut metadata = Vec::new();
        while let Some(entry) = read_dir
            .next_entry()
            .await
            .map_err(|e| Error::Backend(e.to_string()))?
        {
            // Skip non-files and anything that doesn't decode, so the directory can coexist with
            // unrelated files; a genuinely corrupt key still surfaces via `get`.
            if !entry
                .file_type()
                .await
                .map(|t| t.is_file())
                .unwrap_or(false)
            {
                continue;
            }
            if let Ok(bytes) = tokio::fs::read(entry.path()).await {
                if let Ok(decoded) = postcard::from_bytes::<EncryptedEntry>(&bytes) {
                    metadata.push(decoded.metadata);
                }
            }
        }
        Ok(metadata)
    }

    async fn remove(&self, label: &str) -> Result<bool> {
        let path = self.entry_path(label)?;
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(Error::Backend(e.to_string())),
        }
    }
}

#[cfg(unix)]
async fn create_dir(dir: &Path) -> std::io::Result<()> {
    tokio::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .await
}

#[cfg(not(unix))]
async fn create_dir(dir: &Path) -> std::io::Result<()> {
    tokio::fs::create_dir_all(dir).await
}

#[cfg(unix)]
async fn create_file(path: &Path) -> std::io::Result<tokio::fs::File> {
    tokio::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .await
}

#[cfg(not(unix))]
async fn create_file(path: &Path) -> std::io::Result<tokio::fs::File> {
    tokio::fs::File::create(path).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "ed25519")]
    #[tokio::test]
    async fn persists_across_instances() {
        use crate::keystore::{Keychain, generate_key};
        use libp2p::identity::Keypair;

        let dir = std::env::temp_dir().join(format!("connexa-fskeystore-{}", std::process::id()));
        let _ = tokio::fs::remove_dir_all(&dir).await;
        let key = generate_key();
        let keypair = Keypair::generate_ed25519();

        Keychain::new_with_store(key, FilesystemKeystore::new(&dir))
            .insert("identity", &keypair)
            .await
            .unwrap();

        // A fresh keychain over the same dir + master key reads the persisted entry.
        let reopened = Keychain::new_with_store(key, FilesystemKeystore::new(&dir));
        let recovered = reopened.get("identity").await.unwrap();
        assert_eq!(
            recovered.public().to_peer_id(),
            keypair.public().to_peer_id()
        );
        assert_eq!(reopened.list().await.unwrap().len(), 1);
        assert!(reopened.remove("identity").await.unwrap());
        assert!(matches!(
            reopened.get("identity").await,
            Err(Error::NotFound(_))
        ));

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn rejects_unsafe_label() {
        let store = FilesystemKeystore::new(std::env::temp_dir());
        assert!(matches!(
            store.get("../escape").await,
            Err(Error::InvalidLabel(_))
        ));
        assert!(matches!(
            store.remove("a/b").await,
            Err(Error::InvalidLabel(_))
        ));
    }

    #[cfg(all(unix, feature = "ed25519"))]
    #[tokio::test]
    async fn unix_files_are_owner_only() {
        use crate::keystore::{Keychain, generate_key};
        use libp2p::identity::Keypair;
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(format!("connexa-fsperms-{}", std::process::id()));
        let _ = tokio::fs::remove_dir_all(&dir).await;

        Keychain::new_with_store(generate_key(), FilesystemKeystore::new(&dir))
            .insert("identity", &Keypair::generate_ed25519())
            .await
            .unwrap();

        let dir_mode = tokio::fs::metadata(&dir)
            .await
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        let file_mode = tokio::fs::metadata(dir.join("identity"))
            .await
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(dir_mode, 0o700);
        assert_eq!(file_mode, 0o600);

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}
