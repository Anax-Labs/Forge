//! Hash algorithms and algorithm-tagged object identifiers.
//!
//! Forge-native repositories use SHA-256 (§5.1). SHA-1 is supported for reading
//! and interoperating with existing Git repositories (§5.8). The algorithm is
//! always carried alongside the digest so the two can never be confused.
//!
//! ## Canonical 32-byte form
//!
//! Onchain account fields are fixed `[u8; 32]` (§4). SHA-256 digests map
//! directly. SHA-1 digests are padded to 32 bytes with a leading version tag
//! (`0x01`), per the §4 notation "pad SHA-1 to 32 with a version tag":
//!
//! ```text
//! SHA-256 -> [ digest[0..32] ]
//! SHA-1   -> [ 0x01, digest[0..20], 0x00 x 11 ]
//! ```
//!
//! This is a documented design decision resolving the ambiguity in §5.6.

use crate::error::ObjectError;

/// Supported Git/Forge hash algorithms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HashAlgorithm {
    /// SHA-1, 20-byte digest. Interop only.
    Sha1,
    /// SHA-256, 32-byte digest. Forge-native default.
    Sha256,
}

/// Leading version tag used when padding a SHA-1 digest into 32 bytes.
pub const SHA1_VERSION_TAG: u8 = 0x01;

impl HashAlgorithm {
    /// The lowercase tag used in algorithm-qualified strings (`sha1:` / `sha256:`).
    pub const fn tag(self) -> &'static str {
        match self {
            HashAlgorithm::Sha1 => "sha1",
            HashAlgorithm::Sha256 => "sha256",
        }
    }

    /// The raw digest length in bytes.
    pub const fn digest_len(self) -> usize {
        match self {
            HashAlgorithm::Sha1 => 20,
            HashAlgorithm::Sha256 => 32,
        }
    }

    /// Parses an algorithm from its lowercase tag.
    ///
    /// # Errors
    /// Returns [`ObjectError::InvalidOidString`] for an unknown tag.
    pub fn from_tag(tag: &str) -> Result<Self, ObjectError> {
        match tag {
            "sha1" => Ok(HashAlgorithm::Sha1),
            "sha256" => Ok(HashAlgorithm::Sha256),
            other => Err(ObjectError::InvalidOidString(format!(
                "unknown hash algorithm tag: {other}"
            ))),
        }
    }

    /// Computes the digest of `bytes` with this algorithm.
    pub fn digest(self, bytes: &[u8]) -> Vec<u8> {
        match self {
            HashAlgorithm::Sha1 => {
                use sha1::Digest as _;
                sha1::Sha1::digest(bytes).to_vec()
            }
            HashAlgorithm::Sha256 => {
                use sha2::Digest as _;
                sha2::Sha256::digest(bytes).to_vec()
            }
        }
    }
}

/// An algorithm-tagged cryptographic digest.
///
/// Used for Git object ids, message hashes, the repository history root, and
/// the repo root fingerprint.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Oid {
    algorithm: HashAlgorithm,
    bytes: Vec<u8>,
}

impl Oid {
    /// Builds an oid from raw digest bytes, validating the length.
    ///
    /// # Errors
    /// Returns [`ObjectError::InvalidOidLength`] if `bytes` does not match the
    /// digest length of `algorithm`.
    pub fn new(algorithm: HashAlgorithm, bytes: impl Into<Vec<u8>>) -> Result<Self, ObjectError> {
        let bytes = bytes.into();
        if bytes.len() != algorithm.digest_len() {
            return Err(ObjectError::InvalidOidLength(bytes.len(), algorithm.tag()));
        }
        Ok(Self { algorithm, bytes })
    }

    /// Parses an oid from a hex digest of the given algorithm.
    ///
    /// # Errors
    /// Returns [`ObjectError::InvalidHex`] or [`ObjectError::InvalidOidLength`].
    pub fn from_hex(algorithm: HashAlgorithm, hex: &str) -> Result<Self, ObjectError> {
        let bytes = hex::decode(hex).map_err(|e| ObjectError::InvalidHex(e.to_string()))?;
        Self::new(algorithm, bytes)
    }

    /// The all-zero oid for `algorithm` (used for empty refs).
    pub fn zero(algorithm: HashAlgorithm) -> Self {
        Self {
            algorithm,
            bytes: vec![0u8; algorithm.digest_len()],
        }
    }

    /// Parses an algorithm-qualified string such as `sha256:ab12...`.
    ///
    /// # Errors
    /// Returns [`ObjectError::InvalidOidString`] when the tag is missing or
    /// unknown, or propagates hex/length errors.
    pub fn parse_tagged(value: &str) -> Result<Self, ObjectError> {
        let (tag, hex) = value
            .split_once(':')
            .ok_or_else(|| ObjectError::InvalidOidString(value.to_string()))?;
        let algorithm = HashAlgorithm::from_tag(tag)?;
        Self::from_hex(algorithm, hex)
    }

    /// The hash algorithm of this digest.
    pub const fn algorithm(&self) -> HashAlgorithm {
        self.algorithm
    }

    /// The raw digest bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Whether every digest byte is zero.
    pub fn is_zero(&self) -> bool {
        self.bytes.iter().all(|b| *b == 0)
    }

    /// Lowercase hex encoding of the raw digest (no algorithm tag).
    pub fn to_hex(&self) -> String {
        hex::encode(&self.bytes)
    }

    /// Algorithm-qualified string, e.g. `sha256:ab12...`.
    pub fn to_tagged_string(&self) -> String {
        format!("{}:{}", self.algorithm.tag(), self.to_hex())
    }

    /// Canonical fixed 32-byte representation for onchain storage (§4).
    ///
    /// See the module docs for the SHA-1 padding scheme.
    pub fn to_bytes32(&self) -> [u8; 32] {
        let mut out = [0u8; 32];
        match self.algorithm {
            HashAlgorithm::Sha256 => out.copy_from_slice(&self.bytes),
            HashAlgorithm::Sha1 => {
                out[0] = SHA1_VERSION_TAG;
                out[1..=self.bytes.len()].copy_from_slice(&self.bytes);
            }
        }
        out
    }

    /// Ensures this oid uses `algorithm`.
    ///
    /// # Errors
    /// Returns [`ObjectError::AlgorithmMismatch`] otherwise.
    pub fn ensure_algorithm(&self, algorithm: HashAlgorithm) -> Result<(), ObjectError> {
        if self.algorithm == algorithm {
            Ok(())
        } else {
            Err(ObjectError::algorithm_mismatch(algorithm, self.algorithm))
        }
    }
}

impl std::fmt::Display for Oid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_tagged_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_lengths_are_correct() {
        assert_eq!(HashAlgorithm::Sha1.digest(b"abc").len(), 20);
        assert_eq!(HashAlgorithm::Sha256.digest(b"abc").len(), 32);
    }

    #[test]
    fn sha1_of_abc_matches_known_value() {
        assert_eq!(
            HashAlgorithm::Sha1.digest(b"abc"),
            hex::decode("a9993e364706816aba3e25717850c26c9cd0d89d").unwrap()
        );
    }

    #[test]
    fn sha256_of_abc_matches_known_value() {
        assert_eq!(
            HashAlgorithm::Sha256.digest(b"abc"),
            hex::decode("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
                .unwrap()
        );
    }

    #[test]
    fn tagged_roundtrip() {
        let oid = Oid::new(HashAlgorithm::Sha256, vec![7u8; 32]).unwrap();
        let s = oid.to_tagged_string();
        assert_eq!(Oid::parse_tagged(&s).unwrap(), oid);
        assert!(Oid::parse_tagged("md5:00").is_err());
    }

    #[test]
    fn sha1_pads_to_32_with_version_tag() {
        let oid = Oid::new(HashAlgorithm::Sha1, vec![0xABu8; 20]).unwrap();
        let padded = oid.to_bytes32();
        assert_eq!(padded[0], SHA1_VERSION_TAG);
        assert_eq!(&padded[1..21], &[0xABu8; 20]);
        assert_eq!(&padded[21..32], &[0u8; 11]);
    }

    #[test]
    fn wrong_length_is_rejected() {
        assert!(Oid::new(HashAlgorithm::Sha1, vec![0u8; 32]).is_err());
    }
}
