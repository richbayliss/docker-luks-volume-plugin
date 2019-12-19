use openssl::rand::rand_bytes;
use std::fmt;

#[derive(Debug)]
pub enum CryptoError {
    InvalidKey,
    UnableToEncrypt(String),
    UnableToDecrypt(String),
}

impl fmt::Display for CryptoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match &self {
                Self::InvalidKey => "Invalid Key",
                _ => "Unknown",
            }
        )
    }
}

pub type Blob = Vec<u8>;
pub type CryptoResult<T> = Result<T, CryptoError>;

pub trait VirtualHSM {
    fn encrypt(&self, blob: Blob) -> CryptoResult<Blob>;
    fn decrypt(&self, blob: Blob) -> CryptoResult<Blob>;
    fn random_bytes(&self) -> CryptoResult<Blob>;
}

pub struct DummyHSM {}
impl DummyHSM {
    pub fn new() -> Self {
        Self {}
    }
}

impl VirtualHSM for DummyHSM {
    fn encrypt(&self, blob: Blob) -> CryptoResult<Blob> {
        Ok(blob)
    }

    fn decrypt(&self, blob: Blob) -> CryptoResult<Blob> {
        Ok(blob)
    }

    fn random_bytes(&self) -> CryptoResult<Blob> {
        let mut buf = [0; 1024];
        rand_bytes(&mut buf).unwrap();

        Ok(buf.to_vec())
    }
}
