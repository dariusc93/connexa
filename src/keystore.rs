pub mod cipher;
pub mod store;

use cipher::xchacha20poly1305::XChaCha20Poly1305Cipher;
use libp2p::identity::Keypair;
use rand::RngCore;
use std::future::Future;
use std::sync::Arc;
use thiserror::Error;
use web_time::Duration;
use web_time::SystemTime;
use zeroize::Zeroizing;

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("no key found for label '{0}'")]
    NotFound(String),
    #[error("key '{0}' has expired")]
    Expired(String),
    #[error("failed to encrypt key material")]
    EncryptFailed,
    #[error("failed to decrypt key material (wrong master key or tampered entry)")]
    DecryptFailed,
    #[error(transparent)]
    Key(#[from] libp2p::identity::DecodingError),
    #[error("keystore backend error: {0}")]
    Backend(String),
    #[error("cannot generate a key of type {0:?}")]
    UnsupportedKeyType(KeyType),
}

/// The cryptographic family of a stored key, mirroring [`libp2p::identity::KeyType`].
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyType {
    Ed25519,
    Rsa,
    Secp256k1,
    Ecdsa,
}

impl From<libp2p::identity::KeyType> for KeyType {
    fn from(value: libp2p::identity::KeyType) -> Self {
        use libp2p::identity::KeyType as P2pKeyType;
        match value {
            P2pKeyType::Ed25519 => KeyType::Ed25519,
            P2pKeyType::RSA => KeyType::Rsa,
            P2pKeyType::Secp256k1 => KeyType::Secp256k1,
            P2pKeyType::Ecdsa => KeyType::Ecdsa,
        }
    }
}

/// What [`Keychain::rotate`] installs as the new key: a supplied keypair, or a freshly generated
/// one of the given [`KeyType`].
#[allow(clippy::large_enum_variant)]
pub enum RotateKey {
    Keypair(Keypair),
    Generate(KeyType),
}

impl From<Keypair> for RotateKey {
    fn from(keypair: Keypair) -> Self {
        RotateKey::Keypair(keypair)
    }
}

impl From<&Keypair> for RotateKey {
    fn from(keypair: &Keypair) -> Self {
        RotateKey::Keypair(keypair.clone())
    }
}

impl From<KeyType> for RotateKey {
    fn from(key_type: KeyType) -> Self {
        RotateKey::Generate(key_type)
    }
}

/// When a stored key should expire.
#[derive(Debug, Clone, Copy)]
pub enum Expiry {
    /// The key never expires.
    Never,
    /// The key expires this long after it is stored.
    After(Duration),
    /// The key expires at a specific point in time.
    At(SystemTime),
}

impl Expiry {
    fn resolve(self, created_at: SystemTime) -> Option<SystemTime> {
        match self {
            Expiry::Never => None,
            Expiry::After(ttl) => created_at.checked_add(ttl),
            Expiry::At(at) => Some(at),
        }
    }
}

/// Metadata describing a stored key.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone)]
pub struct KeyMetadata {
    pub label: String,
    pub key_type: KeyType,
    /// How many times this label has been written; starts at 1, bumped by [`Keychain::rotate`].
    pub version: u32,
    pub created_at: SystemTime,
    pub expires_at: Option<SystemTime>,
}

impl KeyMetadata {
    /// Whether the key is past its expiration time.
    pub fn is_expired(&self) -> bool {
        self.expires_at.is_some_and(|exp| SystemTime::now() >= exp)
    }
}

/// An encrypted key entry as seen by a [`Keystore`] backend. Backends only ever handle ciphertext.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone)]
pub struct EncryptedEntry {
    pub metadata: KeyMetadata,
    /// Opaque ciphertext blob produced by the [`Cipher`] backend (it owns any nonce/framing).
    ciphertext: Vec<u8>,
}

impl EncryptedEntry {
    pub fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }
}

/// Generate a random 32-byte master key for a [`Keychain`].
pub fn generate_key() -> [u8; 32] {
    let mut key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);
    key
}

fn generate_keypair(key_type: KeyType) -> Result<Keypair> {
    match key_type {
        #[cfg(feature = "ed25519")]
        KeyType::Ed25519 => Ok(Keypair::generate_ed25519()),
        #[cfg(feature = "secp256k1")]
        KeyType::Secp256k1 => Ok(Keypair::generate_secp256k1()),
        #[cfg(feature = "ecdsa")]
        KeyType::Ecdsa => Ok(Keypair::generate_ecdsa()),
        other => Err(Error::UnsupportedKeyType(other)),
    }
}

/// Backend responsible for persisting encrypted key entries.
pub trait Keystore: Send + Sync + 'static {
    /// Store (or replace) an entry, keyed by its `label`.
    fn put(&self, entry: EncryptedEntry) -> impl Future<Output = Result<()>> + Send;
    /// Fetch the entry for `label`, if present.
    fn get(&self, label: &str) -> impl Future<Output = Result<Option<EncryptedEntry>>> + Send;
    /// List metadata for all stored entries.
    fn list(&self) -> impl Future<Output = Result<Vec<KeyMetadata>>> + Send;
    /// Remove the entry for `label`, returning whether one existed.
    fn remove(&self, label: &str) -> impl Future<Output = Result<bool>> + Send;
}

/// Pluggable authenticated-encryption backend for a [`Keychain`].
pub trait Cipher: Send + Sync + 'static {
    fn encrypt(&self, aad: Option<&[u8]>, plaintext: &[u8]) -> Result<Vec<u8>>;
    fn decrypt(&self, aad: Option<&[u8]>, ciphertext: &[u8]) -> Result<Vec<u8>>;
}

/// An encrypted keychain
pub struct Keychain<S> {
    cipher: Arc<dyn Cipher>,
    backend: Arc<S>,
}

impl<S> Clone for Keychain<S> {
    fn clone(&self) -> Self {
        Self {
            cipher: self.cipher.clone(),
            backend: self.backend.clone(),
        }
    }
}

impl Keychain<store::memory::MemoryKeystore> {
    /// Create a keychain backed by an in-memory store using the given 32-byte master key.
    pub fn in_memory(key: [u8; 32]) -> Self {
        Self::new(key, store::memory::MemoryKeystore::default())
    }

    pub fn in_memory_with_custom_cipher(cipher: impl Cipher) -> Self {
        Self::with_cipher(cipher, store::memory::MemoryKeystore::default())
    }
}

impl<S: Keystore> Keychain<S> {
    /// Create a keychain over `backend`.
    pub fn new(key: [u8; 32], backend: S) -> Self {
        Self::with_cipher(XChaCha20Poly1305Cipher::new(key), backend)
    }

    /// Create a keychain over `backend` using a custom [`Cipher`] backend (e.g. AES-GCM).
    pub fn with_cipher(cipher: impl Cipher, backend: S) -> Self {
        Self {
            cipher: Arc::new(cipher),
            backend: Arc::new(backend),
        }
    }

    /// Encrypt `keypair` and store it under `label` with no expiry, replacing any existing entry.
    pub async fn insert(&self, label: &str, keypair: &Keypair) -> Result<()> {
        self.store(label, keypair, Expiry::Never, 1).await
    }

    /// Like [`Keychain::insert`], but applying an expiration policy.
    pub async fn insert_with_expiry(
        &self,
        label: &str,
        keypair: &Keypair,
        expiry: Expiry,
    ) -> Result<()> {
        self.store(label, keypair, expiry, 1).await
    }

    /// Generate a fresh Ed25519 keypair, store it under `label`
    #[cfg(feature = "ed25519")]
    pub async fn generate_ed25519(&self, label: &str) -> Result<Keypair> {
        self.generate(label, KeyType::Ed25519).await
    }

    /// Like [`Keychain::generate_ed25519`], for Secp256k1.
    #[cfg(feature = "secp256k1")]
    pub async fn generate_secp256k1(&self, label: &str) -> Result<Keypair> {
        self.generate(label, KeyType::Secp256k1).await
    }

    /// Like [`Keychain::generate_ed25519`], for ECDSA.
    #[cfg(feature = "ecdsa")]
    pub async fn generate_ecdsa(&self, label: &str) -> Result<Keypair> {
        self.generate(label, KeyType::Ecdsa).await
    }

    /// Generate a fresh keypair of `key_type`, store it under `label`.
    pub async fn generate(&self, label: &str, key_type: KeyType) -> Result<Keypair> {
        let keypair = generate_keypair(key_type)?;
        self.insert(label, &keypair).await?;
        Ok(keypair)
    }

    /// Replace the key stored under `label`, bumping its [`KeyMetadata::version`]. The replacement
    /// is either a supplied keypair or one generated from a [`KeyType`].
    pub async fn rotate(
        &self,
        label: &str,
        key: impl Into<RotateKey>,
        expiry: Expiry,
    ) -> Result<()> {
        let current = self
            .backend
            .get(label)
            .await?
            .ok_or_else(|| Error::NotFound(label.to_owned()))?;
        let version = current.metadata.version.saturating_add(1);
        let keypair = match key.into() {
            RotateKey::Keypair(keypair) => keypair,
            RotateKey::Generate(key_type) => generate_keypair(key_type)?,
        };
        self.store(label, &keypair, expiry, version).await
    }

    async fn store(
        &self,
        label: &str,
        keypair: &Keypair,
        expiry: Expiry,
        version: u32,
    ) -> Result<()> {
        let plaintext = Zeroizing::new(keypair.to_protobuf_encoding()?);
        let ciphertext = self
            .cipher
            .encrypt(Some(label.as_bytes()), plaintext.as_slice())?;
        let created_at = SystemTime::now();
        let entry = EncryptedEntry {
            metadata: KeyMetadata {
                label: label.to_owned(),
                key_type: keypair.key_type().into(),
                version,
                created_at,
                expires_at: expiry.resolve(created_at),
            },
            ciphertext,
        };
        self.backend.put(entry).await
    }

    /// Fetch and decrypt the keypair stored under `label`. Returns with [`Error::Expired`] if the
    /// key is past its expiration (the entry is left in place. See [`Keychain::purge_expired`]).
    pub async fn get(&self, label: &str) -> Result<Keypair> {
        let entry = self
            .backend
            .get(label)
            .await?
            .ok_or_else(|| Error::NotFound(label.to_owned()))?;
        if entry.metadata.is_expired() {
            return Err(Error::Expired(label.to_owned()));
        }
        let plaintext = Zeroizing::new(
            self.cipher
                .decrypt(Some(label.as_bytes()), &entry.ciphertext)?,
        );
        Keypair::from_protobuf_encoding(plaintext.as_slice()).map_err(Error::from)
    }

    /// Load the keypair stored under `label`, or generate and store a new identity if none
    /// exists yet. An existing-but-expired key surfaces as [`Error::Expired`] rather than being
    /// regenerated.
    pub async fn get_or_create(&self, label: &str) -> Result<Keypair> {
        match self.get(label).await {
            Ok(keypair) => Ok(keypair),
            Err(Error::NotFound(_)) => {
                let keypair = Keypair::generate_ed25519();
                self.insert(label, &keypair).await?;
                Ok(keypair)
            }
            Err(err) => Err(err),
        }
    }

    /// List metadata for all stored keys.
    pub async fn list(&self) -> Result<Vec<KeyMetadata>> {
        self.backend.list().await
    }

    /// Fetch the metadata for `label` without decrypting the key.
    pub async fn metadata(&self, label: &str) -> Result<Option<KeyMetadata>> {
        Ok(self.backend.get(label).await?.map(|entry| entry.metadata))
    }

    /// Remove the key stored under `label`, returning whether one existed.
    pub async fn remove(&self, label: &str) -> Result<bool> {
        self.backend.remove(label).await
    }

    /// Remove every expired key, returning how many were removed.
    pub async fn purge_expired(&self) -> Result<usize> {
        let expired = self
            .backend
            .list()
            .await?
            .into_iter()
            .filter(KeyMetadata::is_expired)
            .map(|metadata| metadata.label);
        let mut removed = 0;
        for label in expired {
            if self.backend.remove(&label).await? {
                removed += 1;
            }
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "ed25519")]
    #[tokio::test]
    async fn round_trip_list_remove() {
        let keychain = Keychain::in_memory(generate_key());
        let keypair = Keypair::generate_ed25519();

        keychain.insert("identity", &keypair).await.unwrap();

        let recovered = keychain.get("identity").await.unwrap();
        assert_eq!(
            recovered.public().to_peer_id(),
            keypair.public().to_peer_id()
        );

        let meta = keychain.metadata("identity").await.unwrap().unwrap();
        assert_eq!(meta.label, "identity");
        assert_eq!(meta.key_type, KeyType::Ed25519);
        assert_eq!(meta.version, 1);
        assert!(meta.expires_at.is_none());

        assert!(keychain.remove("identity").await.unwrap());
        assert!(matches!(
            keychain.get("identity").await,
            Err(Error::NotFound(_))
        ));
    }

    #[cfg(feature = "ed25519")]
    #[tokio::test]
    async fn expired_key_is_rejected_and_purged() {
        let keychain = Keychain::in_memory(generate_key());
        let keypair = Keypair::generate_ed25519();

        keychain
            .insert_with_expiry("old", &keypair, Expiry::At(web_time::UNIX_EPOCH))
            .await
            .unwrap();
        keychain
            .insert_with_expiry("live", &keypair, Expiry::After(Duration::from_secs(3600)))
            .await
            .unwrap();

        assert!(matches!(keychain.get("old").await, Err(Error::Expired(_))));
        assert!(keychain.get("live").await.is_ok());

        assert_eq!(keychain.purge_expired().await.unwrap(), 1);
        assert!(matches!(keychain.get("old").await, Err(Error::NotFound(_))));
        assert!(keychain.get("live").await.is_ok());
    }

    #[cfg(feature = "ed25519")]
    #[tokio::test]
    async fn rotate_bumps_version_and_replaces_key() {
        let keychain = Keychain::in_memory(generate_key());
        let first = Keypair::generate_ed25519();
        let second = Keypair::generate_ed25519();

        keychain.insert("id", &first).await.unwrap();
        assert_eq!(keychain.metadata("id").await.unwrap().unwrap().version, 1);

        keychain.rotate("id", &second, Expiry::Never).await.unwrap();
        let meta = keychain.metadata("id").await.unwrap().unwrap();
        assert_eq!(meta.version, 2);
        assert_eq!(
            keychain.get("id").await.unwrap().public().to_peer_id(),
            second.public().to_peer_id()
        );

        assert!(matches!(
            keychain.rotate("missing", &first, Expiry::Never).await,
            Err(Error::NotFound(_))
        ));
    }

    #[cfg(feature = "ed25519")]
    #[tokio::test]
    async fn wrong_master_key_fails_to_decrypt() {
        let backend = store::memory::MemoryKeystore::default();
        let keypair = Keypair::generate_ed25519();
        Keychain::new(generate_key(), backend.clone())
            .insert("k", &keypair)
            .await
            .unwrap();

        let other = Keychain::new(generate_key(), backend);
        assert!(matches!(other.get("k").await, Err(Error::DecryptFailed)));
    }

    #[cfg(feature = "ed25519")]
    #[tokio::test]
    async fn get_or_create_persists_identity() {
        let keychain = Keychain::in_memory(generate_key());
        let first = keychain.get_or_create("id").await.unwrap();
        let second = keychain.get_or_create("id").await.unwrap();
        assert_eq!(
            first.public().to_peer_id(),
            second.public().to_peer_id(),
            "get_or_create must not regenerate on the second call"
        );
        assert_eq!(keychain.metadata("id").await.unwrap().unwrap().version, 1);
    }

    #[cfg(feature = "ed25519")]
    #[tokio::test]
    async fn custom_cipher_round_trip() {
        // Note: XOR is not real encryption; it only exercises the custom-Crypto plug point.
        struct XorCipher(u8);
        impl Cipher for XorCipher {
            fn encrypt(&self, _aad: Option<&[u8]>, plaintext: &[u8]) -> Result<Vec<u8>> {
                Ok(plaintext.iter().map(|b| b ^ self.0).collect())
            }
            fn decrypt(&self, _aad: Option<&[u8]>, ciphertext: &[u8]) -> Result<Vec<u8>> {
                Ok(ciphertext.iter().map(|b| b ^ self.0).collect())
            }
        }

        let keychain = Keychain::in_memory_with_custom_cipher(XorCipher(0x5a));
        let keypair = Keypair::generate_ed25519();
        keychain.insert("id", &keypair).await.unwrap();
        let recovered = keychain.get("id").await.unwrap();
        assert_eq!(
            recovered.public().to_peer_id(),
            keypair.public().to_peer_id()
        );
    }

    #[cfg(feature = "ed25519")]
    #[tokio::test]
    async fn generate_stores_and_returns() {
        let keychain = Keychain::in_memory(generate_key());
        let kp = keychain.generate_ed25519("id").await.unwrap();
        assert_eq!(keychain.metadata("id").await.unwrap().unwrap().version, 1);
        assert_eq!(
            keychain.get("id").await.unwrap().public().to_peer_id(),
            kp.public().to_peer_id()
        );
    }

    #[cfg(feature = "ed25519")]
    #[tokio::test]
    async fn rotate_generate_makes_fresh_key() {
        let keychain = Keychain::in_memory(generate_key());
        let original = keychain.generate_ed25519("id").await.unwrap();

        keychain
            .rotate("id", KeyType::Ed25519, Expiry::Never)
            .await
            .unwrap();

        assert_eq!(keychain.metadata("id").await.unwrap().unwrap().version, 2);
        assert_ne!(
            keychain.get("id").await.unwrap().public().to_peer_id(),
            original.public().to_peer_id()
        );
    }

    #[tokio::test]
    async fn generate_rsa_is_unsupported() {
        let keychain = Keychain::in_memory(generate_key());
        assert!(matches!(
            keychain.generate("x", KeyType::Rsa).await,
            Err(Error::UnsupportedKeyType(KeyType::Rsa))
        ));
    }

    #[cfg(all(feature = "aes-gcm", feature = "ed25519"))]
    #[tokio::test]
    async fn aesgcm_round_trip() {
        let keychain = Keychain::in_memory_with_custom_cipher(cipher::aes_gcm::AesGcmCipher::new(
            generate_key(),
        ));
        let keypair = Keypair::generate_ed25519();
        keychain.insert("id", &keypair).await.unwrap();
        let recovered = keychain.get("id").await.unwrap();
        assert_eq!(
            recovered.public().to_peer_id(),
            keypair.public().to_peer_id()
        );
    }
}
