//! Blob objects (§5.2).
//!
//! A blob is the raw content of a file with no filename attached. Identical
//! bytes anywhere in a repository produce the same blob oid, so deduplication
//! is automatic.

use crate::hash::{HashAlgorithm, Oid};
use crate::object::{self, ObjectType};

/// Computes the blob object id of `content`.
pub fn blob_oid(content: &[u8], algorithm: HashAlgorithm) -> Oid {
    object::oid(ObjectType::Blob, content, algorithm)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_newline_blob_matches_git_sha1() {
        // `printf 'hello\n' | git hash-object --stdin`
        assert_eq!(
            blob_oid(b"hello\n", HashAlgorithm::Sha1).to_hex(),
            "ce013625030ba8dba906f756967f9e9ca394464a"
        );
    }

    #[test]
    fn hello_newline_blob_matches_git_sha256() {
        // In a `git init --object-format=sha256` repository.
        assert_eq!(
            blob_oid(b"hello\n", HashAlgorithm::Sha256).to_hex(),
            "2cf8d83d9ee29543b34a87727421fdecb7e3f3a183d337639025de576db9ebb4"
        );
    }

    #[test]
    fn identical_content_deduplicates() {
        assert_eq!(
            blob_oid(b"same", HashAlgorithm::Sha256),
            blob_oid(b"same", HashAlgorithm::Sha256)
        );
    }
}
