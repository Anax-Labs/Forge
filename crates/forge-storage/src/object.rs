//! Git objects as stored in CAS: framed bytes, OID verification on read (§8.3).

use forge_object::hash::{HashAlgorithm, Oid};
use forge_object::object::{self, ObjectType};
use forge_object::parse_framed;

use crate::error::StorageError;

/// A Git object whose identifier has been recomputed from its payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitObject {
    /// Blob / tree / commit / tag.
    pub object_type: ObjectType,
    /// Payload without the Git framing header.
    pub payload: Vec<u8>,
    /// Recomputed object id. Never taken from a storage hint.
    pub oid: Oid,
}

impl GitObject {
    /// Frames `payload` and computes the oid with `forge-object` (§5.1).
    pub fn from_payload(
        object_type: ObjectType,
        payload: Vec<u8>,
        algorithm: HashAlgorithm,
    ) -> Self {
        let oid = object::oid(object_type, &payload, algorithm);
        Self {
            object_type,
            payload,
            oid,
        }
    }

    /// Parses Git-framed bytes and recomputes the oid.
    ///
    /// # Errors
    /// Returns framing errors from `forge-object`.
    pub fn from_framed(bytes: &[u8], algorithm: HashAlgorithm) -> Result<Self, StorageError> {
        let (object_type, payload) = parse_framed(bytes)?;
        Ok(Self::from_payload(object_type, payload.to_vec(), algorithm))
    }

    /// Git framing (`<type> <len>\0<payload>`).
    pub fn framed(&self) -> Vec<u8> {
        object::serialize(self.object_type, &self.payload)
    }

    /// Recomputes the oid and rejects a mismatch.
    ///
    /// # Errors
    /// Returns [`StorageError::OidMismatch`] when the stored oid field does
    /// not match the bytes (should be unreachable for objects built through
    /// this type, but is the verify-on-read gate for untrusted input).
    pub fn verify(&self) -> Result<(), StorageError> {
        let actual = object::oid(self.object_type, &self.payload, self.oid.algorithm());
        if actual == self.oid {
            Ok(())
        } else {
            Err(StorageError::OidMismatch {
                expected: self.oid.to_tagged_string(),
                actual: actual.to_tagged_string(),
            })
        }
    }

    /// Parses framed bytes and requires the recomputed oid to equal `expected`.
    ///
    /// # Errors
    /// Returns [`StorageError::OidMismatch`] or a framing error.
    pub fn verify_framed(bytes: &[u8], expected: &Oid) -> Result<Self, StorageError> {
        let obj = Self::from_framed(bytes, expected.algorithm())?;
        if obj.oid == *expected {
            Ok(obj)
        } else {
            Err(StorageError::OidMismatch {
                expected: expected.to_tagged_string(),
                actual: obj.oid.to_tagged_string(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_object::blob::blob_oid;

    #[test]
    fn verify_framed_accepts_matching_blob() {
        let content = b"hello\n";
        let oid = blob_oid(content, HashAlgorithm::Sha256);
        let framed = object::serialize(ObjectType::Blob, content);
        let obj = GitObject::verify_framed(&framed, &oid).unwrap();
        assert_eq!(obj.payload, content);
    }

    #[test]
    fn verify_framed_rejects_corruption() {
        let oid = blob_oid(b"hello\n", HashAlgorithm::Sha256);
        let framed = object::serialize(ObjectType::Blob, b"hallo\n");
        let err = GitObject::verify_framed(&framed, &oid).unwrap_err();
        assert!(matches!(err, StorageError::OidMismatch { .. }));
    }
}
