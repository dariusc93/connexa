use crate::keystore::{EncryptedEntry, Error, KeyMetadata, Keystore, Result};
use redb::{Database, ReadableTable, TableDefinition, TableError};
use std::path::Path;
use std::sync::Arc;

const TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("keys");

/// redb [`Keystore`] backend.
#[derive(Clone)]
pub struct RedbKeystore {
    db: Arc<Database>,
}

impl RedbKeystore {
    pub async fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let db = tokio::task::spawn_blocking(move || Database::create(path))
            .await
            .map_err(backend)?
            .map_err(backend)?;
        Ok(Self { db: Arc::new(db) })
    }
}

impl Keystore for RedbKeystore {
    async fn put(&self, entry: EncryptedEntry) -> Result<()> {
        let db = self.db.clone();
        let bytes = postcard::to_allocvec(&entry).map_err(backend)?;
        let label = entry.metadata.label;
        tokio::task::spawn_blocking(move || -> Result<()> {
            let tx = db.begin_write().map_err(backend)?;
            {
                let mut table = tx.open_table(TABLE).map_err(backend)?;
                table
                    .insert(label.as_str(), bytes.as_slice())
                    .map_err(backend)?;
            }
            tx.commit().map_err(backend)?;
            Ok(())
        })
        .await
        .map_err(backend)?
    }

    async fn get(&self, label: &str) -> Result<Option<EncryptedEntry>> {
        let db = self.db.clone();
        let label = label.to_owned();
        tokio::task::spawn_blocking(move || -> Result<Option<EncryptedEntry>> {
            let tx = db.begin_read().map_err(backend)?;
            let table = match tx.open_table(TABLE) {
                Ok(table) => table,
                Err(TableError::TableDoesNotExist(_)) => return Ok(None),
                Err(e) => return Err(backend(e)),
            };
            match table.get(label.as_str()).map_err(backend)? {
                Some(value) => Ok(Some(postcard::from_bytes(value.value()).map_err(backend)?)),
                None => Ok(None),
            }
        })
        .await
        .map_err(backend)?
    }

    async fn list(&self) -> Result<Vec<KeyMetadata>> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || -> Result<Vec<KeyMetadata>> {
            let tx = db.begin_read().map_err(backend)?;
            let table = match tx.open_table(TABLE) {
                Ok(table) => table,
                Err(TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
                Err(e) => return Err(backend(e)),
            };
            let mut metadata = Vec::new();
            for entry in table.iter().map_err(backend)? {
                let (_label, value) = entry.map_err(backend)?;
                let decoded: EncryptedEntry =
                    postcard::from_bytes(value.value()).map_err(backend)?;
                metadata.push(decoded.metadata);
            }
            Ok(metadata)
        })
        .await
        .map_err(backend)?
    }

    async fn remove(&self, label: &str) -> Result<bool> {
        let db = self.db.clone();
        let label = label.to_owned();
        tokio::task::spawn_blocking(move || -> Result<bool> {
            let tx = db.begin_write().map_err(backend)?;
            let existed = {
                let mut table = tx.open_table(TABLE).map_err(backend)?;
                table.remove(label.as_str()).map_err(backend)?.is_some()
            };
            tx.commit().map_err(backend)?;
            Ok(existed)
        })
        .await
        .map_err(backend)?
    }
}

fn backend<E: std::fmt::Display>(err: E) -> Error {
    Error::Backend(err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "ed25519")]
    #[tokio::test]
    async fn persists_across_instances() {
        use crate::keystore::{Keychain, generate_key};
        use libp2p::identity::Keypair;

        let path = std::env::temp_dir().join(format!("connexa-redb-{}.redb", std::process::id()));
        let _ = tokio::fs::remove_file(&path).await;
        let key = generate_key();
        let keypair = Keypair::generate_ed25519();

        Keychain::new(key, RedbKeystore::new(&path).await.unwrap())
            .insert("identity", &keypair)
            .await
            .unwrap();

        let reopened = Keychain::new(key, RedbKeystore::new(&path).await.unwrap());
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

        let _ = tokio::fs::remove_file(&path).await;
    }
}
