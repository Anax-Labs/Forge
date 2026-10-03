//! Errors returned by the storage layer.
//!
//! Every failure is explicit: callers must never treat a missing, corrupt, or
//! hint-mismatched object as success (§8.3, §11).

use forge_object::ObjectError;

/// Errors produced while building, uploading, fetching, or verifying stored
/// objects.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// A Git object or CAR framing error originating in `forge-object`.
    #[error("object codec error: {0}")]
    Object(#[from] ObjectError),

    /// Fetched bytes hashed to a different Git object id than requested.
    #[error("oid mismatch: expected {expected}, actual {actual}")]
    OidMismatch {
        /// The identifier the caller asked for.
        expected: String,
        /// The identifier recomputed from the fetched bytes.
        actual: String,
    },

    /// A CIDv1 did not match the SHA-256 of its block bytes.
    #[error("cid mismatch: expected {expected}, actual {actual}")]
    CidMismatch {
        /// Locator the caller asked for.
        expected: String,
        /// CID recomputed from the block.
        actual: String,
    },

    /// The requested object or locator was not present.
    #[error("not found: {0}")]
    NotFound(String),

    /// CAR / varint / DAG-CBOR header was malformed.
    #[error("invalid CAR: {0}")]
    InvalidCar(String),

    /// CID encoding or multibase string was malformed.
    #[error("invalid CID: {0}")]
    InvalidCid(String),

    /// The storage-index file was missing or malformed.
    #[error("invalid storage index: {0}")]
    InvalidIndex(String),

    /// Fewer than two independent pinning backends were configured (§8.3).
    #[error("availability policy requires at least two independent pins")]
    TooFewPins,

    /// An HTTP pinning or bundler request failed.
    #[error("storage HTTP error: {0}")]
    Http(String),

    /// Filesystem I/O failed.
    #[error("storage I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON (index, Kubo, Irys, or Arweave manifest) failed to parse.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// A required environment or configuration value was missing.
    #[error("storage configuration error: {0}")]
    Config(String),
}
