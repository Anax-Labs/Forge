//! Backend-agnostic storage trait and locators (§8.3).
//!
//! A locator (CID, Arweave TXID, or local path) is a **hint**. Fetchers must
//! recompute the Git OID of returned bytes and reject mismatches.

use crate::error::StorageError;

/// Onchain `storage_backend` tag (§4.1 / program constants).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum StorageBackendKind {
    /// IPFS hot layer.
    Ipfs = 0,
    /// Arweave cold layer.
    Arweave = 1,
    /// Hybrid (IPFS + Arweave), the recommended default (§8.3).
    Hybrid = 2,
}

impl StorageBackendKind {
    /// Parses the onchain `u8` tag.
    ///
    /// # Errors
    /// Returns [`StorageError::Config`] for unknown tags.
    pub fn from_u8(value: u8) -> Result<Self, StorageError> {
        match value {
            0 => Ok(Self::Ipfs),
            1 => Ok(Self::Arweave),
            2 => Ok(Self::Hybrid),
            other => Err(StorageError::Config(format!(
                "unknown storage_backend tag {other}"
            ))),
        }
    }
}

/// Wire name of a locator's transport (`ipfs` / `arweave` / `fs` / `memory`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LocatorBackend {
    /// Kubo / pinning-service HTTP.
    Ipfs,
    /// Irys (or compatible) Arweave bundler.
    Arweave,
    /// Local content-addressed directory under `.forge/cas`.
    Fs,
    /// In-process map (tests).
    Memory,
}

/// Where a copy of some bytes was stored. Never treated as authentic.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Locator {
    /// Transport kind.
    pub backend: LocatorBackend,
    /// Independent pin / provider id (e.g. `ipfs-0`, `irys`, `cas`).
    pub provider: String,
    /// CID, TXID, or CAS filename.
    pub id: String,
}

/// A content-addressed byte store.
pub trait StorageBackend: Send + Sync {
    /// Stable provider id recorded in the storage index.
    fn provider_id(&self) -> &str;

    /// Transport kind for locators this backend produces.
    fn locator_backend(&self) -> LocatorBackend;

    /// Stores `bytes` and returns a locator. The locator is a hint.
    ///
    /// # Errors
    /// Returns I/O or HTTP errors.
    fn put(&self, bytes: &[u8]) -> Result<Locator, StorageError>;

    /// Fetches raw bytes for `locator_id`. Callers must hash-verify.
    ///
    /// # Errors
    /// Returns [`StorageError::NotFound`] or I/O/HTTP errors.
    fn get(&self, locator_id: &str) -> Result<Vec<u8>, StorageError>;

    /// Whether `locator_id` is present without fetching the body.
    ///
    /// # Errors
    /// Returns I/O or HTTP errors.
    fn exists(&self, locator_id: &str) -> Result<bool, StorageError>;

    /// Drops a stored object (tests / killed-pin simulation).
    ///
    /// # Errors
    /// Returns I/O errors. Missing keys are not an error.
    fn delete(&self, locator_id: &str) -> Result<(), StorageError>;
}

/// Fetches `locator` from `backend` and verifies it as Git-framed `expected`.
///
/// If the bytes are a CARv1, the object is extracted from the archive.
///
/// # Errors
/// Returns not-found, OID mismatch, or codec errors.
pub fn fetch_verified(
    backend: &dyn StorageBackend,
    locator: &Locator,
    expected: &forge_object::hash::Oid,
) -> Result<crate::object::GitObject, StorageError> {
    let bytes = backend.get(&locator.id)?;
    match crate::car::Car::decode(&bytes) {
        Ok(car) => car.get(expected),
        Err(StorageError::InvalidCar(_)) => {
            crate::object::GitObject::verify_framed(&bytes, expected)
        }
        Err(err) => Err(err),
    }
}
