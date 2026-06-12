use crate::keystore::{EncryptedEntry, Error, KeyMetadata, Keystore, Result};
use sqlx::SqlitePool;
use sqlx::sqlite::SqliteConnectOptions;
use std::path::Path;
use std::str::FromStr;
use tokio::sync::OnceCell;

/// SQLite [`Keystore`] backend.
pub struct SqliteKeystore {
    options: SqliteConnectOptions,
    pool: OnceCell<SqlitePool>,
}

impl SqliteKeystore {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true);
        Ok(Self::from_options(options))
    }

    pub fn in_memory() -> Result<Self> {
        let options =
            SqliteConnectOptions::from_str("sqlite:file::memory:?cache=shared").map_err(backend)?;
        Ok(Self::from_options(options))
    }

    fn from_options(options: SqliteConnectOptions) -> Self {
        Self {
            options,
            pool: OnceCell::new(),
        }
    }

    async fn pool(&self) -> Result<&SqlitePool> {
        self.pool
            .get_or_try_init(|| async {
                let pool = SqlitePool::connect_with(self.options.clone())
                    .await
                    .map_err(backend)?;
                sqlx::query(
                    "CREATE TABLE IF NOT EXISTS keys (label TEXT PRIMARY KEY, data BLOB NOT NULL)",
                )
                .execute(&pool)
                .await
                .map_err(backend)?;
                Ok(pool)
            })
            .await
    }
}

impl Keystore for SqliteKeystore {
    async fn put(&self, entry: EncryptedEntry) -> Result<()> {
        let pool = self.pool().await?;
        let bytes = postcard::to_allocvec(&entry).map_err(backend)?;
        sqlx::query("INSERT OR REPLACE INTO keys (label, data) VALUES (?1, ?2)")
            .bind(entry.metadata.label)
            .bind(bytes)
            .execute(pool)
            .await
            .map_err(backend)?;
        Ok(())
    }

    async fn get(&self, label: &str) -> Result<Option<EncryptedEntry>> {
        let pool = self.pool().await?;
        let row: Option<(Vec<u8>,)> = sqlx::query_as("SELECT data FROM keys WHERE label = ?1")
            .bind(label)
            .fetch_optional(pool)
            .await
            .map_err(backend)?;
        match row {
            Some((bytes,)) => Ok(Some(postcard::from_bytes(&bytes).map_err(backend)?)),
            None => Ok(None),
        }
    }

    async fn list(&self) -> Result<Vec<KeyMetadata>> {
        let pool = self.pool().await?;
        let rows: Vec<(Vec<u8>,)> = sqlx::query_as("SELECT data FROM keys")
            .fetch_all(pool)
            .await
            .map_err(backend)?;
        let mut metadata = Vec::with_capacity(rows.len());
        for (bytes,) in rows {
            let entry: EncryptedEntry = postcard::from_bytes(&bytes).map_err(backend)?;
            metadata.push(entry.metadata);
        }
        Ok(metadata)
    }

    async fn remove(&self, label: &str) -> Result<bool> {
        let pool = self.pool().await?;
        let result = sqlx::query("DELETE FROM keys WHERE label = ?1")
            .bind(label)
            .execute(pool)
            .await
            .map_err(backend)?;
        Ok(result.rows_affected() > 0)
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

        let path = std::env::temp_dir().join(format!("connexa-sqlite-{}.db", std::process::id()));
        let _ = tokio::fs::remove_file(&path).await;
        let key = generate_key();
        let keypair = Keypair::generate_ed25519();

        Keychain::new(key, SqliteKeystore::new(&path).unwrap())
            .insert("identity", &keypair)
            .await
            .unwrap();

        let reopened = Keychain::new(key, SqliteKeystore::new(&path).unwrap());
        assert_eq!(
            reopened
                .get("identity")
                .await
                .unwrap()
                .public()
                .to_peer_id(),
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
