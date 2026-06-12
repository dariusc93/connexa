use crate::keystore::{EncryptedEntry, Error, KeyMetadata, Keystore, Result};
use idb::{Database, DatabaseEvent, Factory, ObjectStoreParams, TransactionMode};
use send_wrapper::SendWrapper;
use std::future::Future;
use tokio::sync::OnceCell;
use wasm_bindgen::JsValue;

const OBJECT_STORE: &str = "keys";

/// IndexedDB [`Keystore`] backend for wasm.
pub struct IndexedDbKeystore {
    identifier: String,
    db: OnceCell<SendWrapper<Database>>,
}

impl IndexedDbKeystore {
    pub fn new(identifier: impl Into<String>) -> Self {
        Self {
            identifier: identifier.into(),
            db: OnceCell::new(),
        }
    }

    async fn db(&self) -> Result<&Database> {
        let db = self
            .db
            .get_or_try_init(|| {
                SendWrapper::new(async {
                    let factory = Factory::new().map_err(backend)?;
                    let name = format!("connexa-keystore-{}", self.identifier);
                    let mut request = factory.open(&name, Some(1)).map_err(backend)?;
                    request.on_upgrade_needed(|event| {
                        if let Ok(db) = event.database() {
                            let _ = db.create_object_store(OBJECT_STORE, ObjectStoreParams::new());
                        }
                    });
                    let database = request.await.map_err(backend)?;
                    Ok::<_, Error>(SendWrapper::new(database))
                })
            })
            .await?;
        Ok(&**db)
    }
}

impl Keystore for IndexedDbKeystore {
    fn put(&self, entry: EncryptedEntry) -> impl Future<Output = Result<()>> + Send {
        SendWrapper::new(async move {
            let db = self.db().await?;
            let value = serde_wasm_bindgen::to_value(&entry).map_err(backend)?;
            let key = JsValue::from_str(&entry.metadata.label);
            let tx = db
                .transaction(&[OBJECT_STORE], TransactionMode::ReadWrite)
                .map_err(backend)?;
            let store = tx.object_store(OBJECT_STORE).map_err(backend)?;
            store
                .put(&value, Some(&key))
                .map_err(backend)?
                .await
                .map_err(backend)?;
            tx.commit().map_err(backend)?.await.map_err(backend)?;
            Ok(())
        })
    }

    fn put_many(&self, entries: Vec<EncryptedEntry>) -> impl Future<Output = Result<()>> + Send {
        SendWrapper::new(async move {
            let db = self.db().await?;
            let tx = db
                .transaction(&[OBJECT_STORE], TransactionMode::ReadWrite)
                .map_err(backend)?;
            let store = tx.object_store(OBJECT_STORE).map_err(backend)?;
            for entry in entries {
                let value = serde_wasm_bindgen::to_value(&entry).map_err(backend)?;
                let key = JsValue::from_str(&entry.metadata.label);
                store
                    .put(&value, Some(&key))
                    .map_err(backend)?
                    .await
                    .map_err(backend)?;
            }
            tx.commit().map_err(backend)?.await.map_err(backend)?;
            Ok(())
        })
    }

    fn get(&self, label: &str) -> impl Future<Output = Result<Option<EncryptedEntry>>> + Send {
        let key = JsValue::from_str(label);
        SendWrapper::new(async move {
            let db = self.db().await?;
            let tx = db
                .transaction(&[OBJECT_STORE], TransactionMode::ReadOnly)
                .map_err(backend)?;
            let store = tx.object_store(OBJECT_STORE).map_err(backend)?;
            match store.get(key).map_err(backend)?.await.map_err(backend)? {
                Some(value) => Ok(Some(
                    serde_wasm_bindgen::from_value(value).map_err(backend)?,
                )),
                None => Ok(None),
            }
        })
    }

    fn list(&self) -> impl Future<Output = Result<Vec<KeyMetadata>>> + Send {
        SendWrapper::new(async move {
            let db = self.db().await?;
            let tx = db
                .transaction(&[OBJECT_STORE], TransactionMode::ReadOnly)
                .map_err(backend)?;
            let store = tx.object_store(OBJECT_STORE).map_err(backend)?;
            let values = store
                .get_all(None, None)
                .map_err(backend)?
                .await
                .map_err(backend)?;
            let mut metadata = Vec::with_capacity(values.len());
            for value in values {
                let entry: EncryptedEntry =
                    serde_wasm_bindgen::from_value(value).map_err(backend)?;
                metadata.push(entry.metadata);
            }
            Ok(metadata)
        })
    }

    fn remove(&self, label: &str) -> impl Future<Output = Result<bool>> + Send {
        let key = JsValue::from_str(label);
        SendWrapper::new(async move {
            let db = self.db().await?;
            let tx = db
                .transaction(&[OBJECT_STORE], TransactionMode::ReadWrite)
                .map_err(backend)?;
            let store = tx.object_store(OBJECT_STORE).map_err(backend)?;
            let existed = store
                .count(Some(key.clone().into()))
                .map_err(backend)?
                .await
                .map_err(backend)?
                > 0;
            store.delete(key).map_err(backend)?.await.map_err(backend)?;
            tx.commit().map_err(backend)?.await.map_err(backend)?;
            Ok(existed)
        })
    }
}

fn backend<E: std::fmt::Display>(err: E) -> Error {
    Error::Backend(err.to_string())
}
