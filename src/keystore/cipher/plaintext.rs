use crate::keystore::{Cipher, Result};

/// A no-op [`Cipher`] that stores key material in cleartext with no encryption or authentication.
/// Intended for tests, or backends that already encrypt at rest.
pub struct Plaintext;

impl Cipher for Plaintext {
    fn encrypt(&self, _: Option<&[u8]>, data: &[u8]) -> Result<Vec<u8>> {
        Ok(data.to_vec())
    }

    fn decrypt(&self, _: Option<&[u8]>, data: &[u8]) -> Result<Vec<u8>> {
        Ok(data.to_vec())
    }
}
