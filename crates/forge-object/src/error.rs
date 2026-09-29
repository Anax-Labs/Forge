//! Errors returned by the canonical object engine.
//!
//! The engine never panics on malformed input; every failure is an explicit
//! variant so callers can distinguish protocol errors from programming errors.

use crate::hash::HashAlgorithm;

/// Errors produced while constructing, encoding, or decoding canonical Forge
/// objects and attestations.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ObjectError {
    /// An unknown object type string was supplied.
    #[error("invalid object type: {0}")]
    InvalidObjectType(String),

    /// An object id had the wrong byte length for its hash algorithm.
    #[error("invalid oid length {0} for hash algorithm {1}")]
    InvalidOidLength(usize, &'static str),

    /// A hex string could not be decoded.
    #[error("invalid hex string: {0}")]
    InvalidHex(String),

    /// An algorithm-tagged oid string was malformed.
    #[error("invalid algorithm-tagged oid: {0}")]
    InvalidOidString(String),

    /// An oid's algorithm did not match the expected algorithm.
    #[error("hash algorithm mismatch: expected {expected}, found {found}")]
    AlgorithmMismatch {
        /// The algorithm required by the surrounding context.
        expected: &'static str,
        /// The algorithm actually present on the value.
        found: &'static str,
    },

    /// A tree entry name violated the Forge-safe path subset.
    #[error("unsafe or invalid path entry name: {0}")]
    InvalidName(String),

    /// A tree entry mode string was not one of the supported Git modes.
    #[error("invalid tree entry mode: {0}")]
    InvalidMode(String),

    /// Two tree entries had the same name.
    #[error("duplicate tree entry name: {0}")]
    DuplicateEntry(String),

    /// A commit identity field was malformed.
    #[error("invalid identity: {0}")]
    InvalidIdentity(String),

    /// Canonical CBOR decoding failed.
    #[error("cbor decode error: {0}")]
    Cbor(String),

    /// The attestation CBOR was valid but not in canonical form.
    #[error("attestation cbor is not canonical")]
    NonCanonicalCbor,
}

impl ObjectError {
    /// Convenience constructor for [`ObjectError::AlgorithmMismatch`].
    pub(crate) fn algorithm_mismatch(expected: HashAlgorithm, found: HashAlgorithm) -> Self {
        ObjectError::AlgorithmMismatch {
            expected: expected.tag(),
            found: found.tag(),
        }
    }
}
