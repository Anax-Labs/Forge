//! Git object framing and object-id computation (§5.1).
//!
//! Every object is serialized as `<type> <byte-length>\0<payload>` and
//! identified by the hash of that serialization. The type string is part of
//! the hashed header, giving inherent domain separation between blob, tree,
//! commit and tag.

use crate::error::ObjectError;
use crate::hash::{HashAlgorithm, Oid};

/// The four Git object types Forge understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjectType {
    /// Raw file bytes.
    Blob,
    /// A directory listing.
    Tree,
    /// A commit.
    Commit,
    /// An annotated tag.
    Tag,
}

impl ObjectType {
    /// The lowercase type string used in the object header.
    pub const fn as_str(self) -> &'static str {
        match self {
            ObjectType::Blob => "blob",
            ObjectType::Tree => "tree",
            ObjectType::Commit => "commit",
            ObjectType::Tag => "tag",
        }
    }

    /// Parses an object type from its lowercase type string.
    ///
    /// Named `parse` rather than `from_str` to avoid confusion with the
    /// `std::str::FromStr` trait.
    ///
    /// # Errors
    /// Returns [`ObjectError::InvalidObjectType`] for unknown strings.
    pub fn parse(value: &str) -> Result<Self, ObjectError> {
        match value {
            "blob" => Ok(ObjectType::Blob),
            "tree" => Ok(ObjectType::Tree),
            "commit" => Ok(ObjectType::Commit),
            "tag" => Ok(ObjectType::Tag),
            other => Err(ObjectError::InvalidObjectType(other.to_string())),
        }
    }
}

/// Serializes the Git framing header and payload: `<type> <len>\0<payload>`.
pub fn serialize(object_type: ObjectType, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 32);
    out.extend_from_slice(object_type.as_str().as_bytes());
    out.push(b' ');
    out.extend_from_slice(payload.len().to_string().as_bytes());
    out.push(0);
    out.extend_from_slice(payload);
    out
}

/// Computes the object id of `payload` using the Git object framing.
///
/// The digest length is guaranteed by [`HashAlgorithm::digest`], so this
/// function is infallible.
pub fn oid(object_type: ObjectType, payload: &[u8], algorithm: HashAlgorithm) -> Oid {
    let framed = serialize(object_type, payload);
    Oid::new(algorithm, algorithm.digest(&framed)).expect("digest length matches algorithm")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framing_includes_type_and_length() {
        assert_eq!(serialize(ObjectType::Blob, b"hello"), b"blob 5\0hello");
        assert_eq!(serialize(ObjectType::Blob, b""), b"blob 0\0");
    }

    #[test]
    fn object_type_roundtrip() {
        for t in [
            ObjectType::Blob,
            ObjectType::Tree,
            ObjectType::Commit,
            ObjectType::Tag,
        ] {
            assert_eq!(ObjectType::parse(t.as_str()).unwrap(), t);
        }
        assert!(ObjectType::parse("sparse").is_err());
    }

    #[test]
    fn empty_blob_sha1_is_well_known() {
        let oid = oid(ObjectType::Blob, b"", HashAlgorithm::Sha1);
        assert_eq!(oid.to_hex(), "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391");
    }
}
