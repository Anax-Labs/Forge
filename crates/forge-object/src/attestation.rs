//! The canonical Forge attestation and its hash (§5.5).
//!
//! A signature cannot live inside the commit object without changing its id, so
//! authorship is carried in a **sidecar attestation** that the wallet signs.
//! The attestation is serialized as canonical CBOR and hashed with domain
//! separation:
//!
//! ```text
//! attestation_hash = H("forge-attestation\0" || canonical_cbor(attestation))
//! ```
//!
//! The sidecar file (`.forge/attestations/<commit>.cbor`) contains exactly
//! `canonical_cbor(attestation)`, so verification is
//! `H("forge-attestation\0" || file_bytes)`.

use crate::cbor::{self, Reader};
use crate::error::ObjectError;
use crate::hash::{HashAlgorithm, Oid};

/// Domain-separation prefix for the attestation hash.
pub const ATTESTATION_DOMAIN: &[u8] = b"forge-attestation\0";

/// Current attestation schema version.
pub const ATTESTATION_VERSION: u8 = 1;

/// A wallet-signed statement binding a commit to its repository and author.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attestation {
    /// Schema version (`v`).
    pub version: u8,
    /// Base58 repository PDA.
    pub repo: String,
    /// The commit object id.
    pub commit: Oid,
    /// Parent commit object ids in first-parent order.
    pub parents: Vec<Oid>,
    /// Top-level tree object id.
    pub tree: Oid,
    /// Base58 author wallet pubkey.
    pub author: String,
    /// Commit timestamp in seconds.
    pub authored_at: i64,
    /// Hash of the commit message.
    pub message_hash: Oid,
    /// Base58 random nonce for replay resistance.
    pub nonce: String,
}

impl Attestation {
    /// Builds and validates an attestation.
    ///
    /// # Errors
    /// Returns [`ObjectError::InvalidIdentity`] for empty/non-opaque string
    /// fields and [`ObjectError::AlgorithmMismatch`] when the commit, tree,
    /// parent, and message-hash oids do not share one hash algorithm.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        version: u8,
        repo: impl Into<String>,
        commit: Oid,
        parents: Vec<Oid>,
        tree: Oid,
        author: impl Into<String>,
        authored_at: i64,
        message_hash: Oid,
        nonce: impl Into<String>,
    ) -> Result<Self, ObjectError> {
        let attestation = Self {
            version,
            repo: repo.into(),
            commit,
            parents,
            tree,
            author: author.into(),
            authored_at,
            message_hash,
            nonce: nonce.into(),
        };
        attestation.validate()?;
        Ok(attestation)
    }

    /// The hash algorithm used for the attestation (taken from the commit oid).
    pub const fn algorithm(&self) -> HashAlgorithm {
        self.commit.algorithm()
    }

    /// Validates string fields and algorithm consistency.
    ///
    /// # Errors
    /// See [`Attestation::new`].
    pub fn validate(&self) -> Result<(), ObjectError> {
        for (label, value) in [
            ("repo", &self.repo),
            ("author", &self.author),
            ("nonce", &self.nonce),
        ] {
            if value.is_empty() || value.contains('\0') {
                return Err(ObjectError::InvalidIdentity(format!(
                    "attestation {label} must be non-empty and NUL-free"
                )));
            }
        }
        let algorithm = self.commit.algorithm();
        self.tree.ensure_algorithm(algorithm)?;
        self.message_hash.ensure_algorithm(algorithm)?;
        for parent in &self.parents {
            parent.ensure_algorithm(algorithm)?;
        }
        Ok(())
    }

    fn field_entries(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
        fn key(name: &str) -> Vec<u8> {
            let mut out = Vec::new();
            cbor::encode_text(&mut out, name);
            out
        }
        fn text(value: &str) -> Vec<u8> {
            let mut out = Vec::new();
            cbor::encode_text(&mut out, value);
            out
        }

        let mut parents = Vec::new();
        cbor::encode_array_head(&mut parents, self.parents.len());
        for parent in &self.parents {
            cbor::encode_text(&mut parents, &parent.to_tagged_string());
        }

        let mut version = Vec::new();
        cbor::encode_uint(&mut version, u64::from(self.version));
        let mut authored_at = Vec::new();
        cbor::encode_int(&mut authored_at, self.authored_at);

        vec![
            (key("v"), version),
            (key("repo"), text(&self.repo)),
            (key("commit"), text(&self.commit.to_tagged_string())),
            (key("parents"), parents),
            (key("tree"), text(&self.tree.to_tagged_string())),
            (key("author"), text(&self.author)),
            (key("authoredAt"), authored_at),
            (
                key("messageHash"),
                text(&self.message_hash.to_tagged_string()),
            ),
            (key("nonce"), text(&self.nonce)),
        ]
    }

    /// Encodes the attestation as canonical CBOR with sorted map keys.
    pub fn to_canonical_cbor(&self) -> Vec<u8> {
        let mut entries = self.field_entries();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        let mut out = Vec::new();
        cbor::encode_map_head(&mut out, entries.len());
        for (key, value) in entries {
            out.extend_from_slice(&key);
            out.extend_from_slice(&value);
        }
        out
    }

    /// Decodes a canonical CBOR attestation, rejecting non-canonical encodings.
    ///
    /// # Errors
    /// Returns [`ObjectError::Cbor`] for malformed/unknown/missing fields and
    /// [`ObjectError::NonCanonicalCbor`] when the bytes re-encode differently.
    pub fn from_canonical_cbor(bytes: &[u8]) -> Result<Self, ObjectError> {
        let attestation = Self::decode(bytes)?;
        if attestation.to_canonical_cbor() != bytes {
            return Err(ObjectError::NonCanonicalCbor);
        }
        attestation.validate()?;
        Ok(attestation)
    }

    /// The attestation hash that the wallet signs.
    pub fn attestation_hash(&self) -> Oid {
        let mut preimage = Vec::with_capacity(ATTESTATION_DOMAIN.len() + 128);
        preimage.extend_from_slice(ATTESTATION_DOMAIN);
        preimage.extend_from_slice(&self.to_canonical_cbor());
        Oid::new(self.algorithm(), self.algorithm().digest(&preimage))
            .expect("digest length matches algorithm")
    }

    fn decode(bytes: &[u8]) -> Result<Self, ObjectError> {
        let mut reader = Reader::new(bytes);
        let count = reader.read_map_head()?;
        let mut version: Option<u8> = None;
        let mut repo: Option<String> = None;
        let mut commit: Option<Oid> = None;
        let mut parents: Option<Vec<Oid>> = None;
        let mut tree: Option<Oid> = None;
        let mut author: Option<String> = None;
        let mut authored_at: Option<i64> = None;
        let mut message_hash: Option<Oid> = None;
        let mut nonce: Option<String> = None;

        for _ in 0..count {
            let key = reader.read_text()?;
            match key.as_str() {
                "v" => {
                    let raw = reader.read_uint()?;
                    let parsed = u8::try_from(raw)
                        .map_err(|_| ObjectError::Cbor("version out of range".into()))?;
                    set_once(&mut version, parsed)?;
                }
                "repo" => set_once(&mut repo, reader.read_text()?)?,
                "commit" => set_once(&mut commit, Oid::parse_tagged(&reader.read_text()?)?)?,
                "parents" => {
                    let len = reader.read_array_head()?;
                    let mut list = Vec::new();
                    for _ in 0..len {
                        list.push(Oid::parse_tagged(&reader.read_text()?)?);
                    }
                    set_once(&mut parents, list)?;
                }
                "tree" => set_once(&mut tree, Oid::parse_tagged(&reader.read_text()?)?)?,
                "author" => set_once(&mut author, reader.read_text()?)?,
                "authoredAt" => set_once(&mut authored_at, reader.read_int()?)?,
                "messageHash" => {
                    set_once(&mut message_hash, Oid::parse_tagged(&reader.read_text()?)?)?;
                }
                "nonce" => set_once(&mut nonce, reader.read_text()?)?,
                other => {
                    return Err(ObjectError::Cbor(format!("unknown field: {other}")));
                }
            }
        }
        if !reader.is_done() {
            return Err(ObjectError::Cbor("trailing bytes after map".into()));
        }

        let attestation = Self {
            version: require(version, "v")?,
            repo: require(repo, "repo")?,
            commit: require(commit, "commit")?,
            parents: require(parents, "parents")?,
            tree: require(tree, "tree")?,
            author: require(author, "author")?,
            authored_at: require(authored_at, "authoredAt")?,
            message_hash: require(message_hash, "messageHash")?,
            nonce: require(nonce, "nonce")?,
        };
        Ok(attestation)
    }
}

fn set_once<T>(slot: &mut Option<T>, value: T) -> Result<(), ObjectError> {
    if slot.is_some() {
        return Err(ObjectError::Cbor("duplicate map key".into()));
    }
    *slot = Some(value);
    Ok(())
}

fn require<T>(slot: Option<T>, name: &str) -> Result<T, ObjectError> {
    slot.ok_or_else(|| ObjectError::Cbor(format!("missing field: {name}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Attestation {
        Attestation::new(
            ATTESTATION_VERSION,
            "7xKrepo",
            Oid::new(HashAlgorithm::Sha256, vec![0xAAu8; 32]).unwrap(),
            vec![Oid::new(HashAlgorithm::Sha256, vec![0xBBu8; 32]).unwrap()],
            Oid::new(HashAlgorithm::Sha256, vec![0xCCu8; 32]).unwrap(),
            "9aBauthor",
            1_790_001_234,
            Oid::new(HashAlgorithm::Sha256, vec![0xDDu8; 32]).unwrap(),
            "nonceNonceNonce",
        )
        .unwrap()
    }

    #[test]
    fn cbor_roundtrip() {
        let attestation = sample();
        let bytes = attestation.to_canonical_cbor();
        let decoded = Attestation::from_canonical_cbor(&bytes).unwrap();
        assert_eq!(attestation, decoded);
    }

    #[test]
    fn cbor_is_deterministic() {
        assert_eq!(sample().to_canonical_cbor(), sample().to_canonical_cbor());
    }

    #[test]
    fn keys_are_sorted_and_shortest_first() {
        let bytes = sample().to_canonical_cbor();
        assert_eq!(bytes[0], 0xA9); // map(9)
        assert_eq!(&bytes[1..3], &[0x61, b'v']); // shortest key first
        let repo = bytes.windows(4).position(|w| w == b"repo").unwrap();
        let tree = bytes.windows(4).position(|w| w == b"tree").unwrap();
        assert!(repo < tree); // same-length keys sorted bytewise
    }

    #[test]
    fn non_canonical_rejected() {
        // Encode the same fields in insertion order (unsorted keys), which must
        // be rejected as non-canonical.
        let attestation = sample();
        let entries = attestation.field_entries();
        let mut bytes = Vec::new();
        cbor::encode_map_head(&mut bytes, entries.len());
        for (key, value) in &entries {
            bytes.extend_from_slice(key);
            bytes.extend_from_slice(value);
        }
        assert!(matches!(
            Attestation::from_canonical_cbor(&bytes),
            Err(ObjectError::NonCanonicalCbor)
        ));
    }

    #[test]
    fn trailing_bytes_rejected() {
        let mut bytes = sample().to_canonical_cbor();
        bytes.push(0x00);
        assert!(Attestation::from_canonical_cbor(&bytes).is_err());
    }

    #[test]
    fn algorithm_mismatch_rejected() {
        let sha1 = Oid::new(HashAlgorithm::Sha1, vec![0u8; 20]).unwrap();
        assert!(Attestation::new(
            ATTESTATION_VERSION,
            "r",
            Oid::new(HashAlgorithm::Sha256, vec![0u8; 32]).unwrap(),
            vec![],
            sha1,
            "a",
            0,
            Oid::new(HashAlgorithm::Sha256, vec![0u8; 32]).unwrap(),
            "n",
        )
        .is_err());
    }

    #[test]
    fn hash_domain_separation() {
        let a = sample();
        let mut b = sample();
        b.nonce = "different".into();
        assert_ne!(a.attestation_hash(), b.attestation_hash());
    }
}
