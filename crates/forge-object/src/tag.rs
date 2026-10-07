//! Canonical signed message for annotated tags (§9.2, §12.2).
//!
//! A tag is an immutable release pointer. The tagger authorizes it by signing a
//! domain-separated message binding the repository, tag name, target commit, and
//! the tag message hash:
//!
//! ```text
//! "forge-tag\0" || repo || name || target_commit32 || message_hash32
//! ```
//!
//! Keeping this in `forge-object` means the CLI and program agree on the exact
//! bytes; `create_tag` verifies the signature and stores `signed = 1`.

use crate::hash::Oid;

/// Domain-separation prefix for tag authorization.
pub const TAG_DOMAIN: &[u8] = b"forge-tag\0";

/// The message a tagger signs to create an annotated tag.
pub fn tag_message(
    repo: &[u8; 32],
    name: &[u8; 32],
    target_commit: &Oid,
    message_hash: &Oid,
) -> Vec<u8> {
    let mut message = Vec::with_capacity(TAG_DOMAIN.len() + 32 + 32 + 32 + 32);
    message.extend_from_slice(TAG_DOMAIN);
    message.extend_from_slice(repo);
    message.extend_from_slice(name);
    message.extend_from_slice(&target_commit.to_bytes32());
    message.extend_from_slice(&message_hash.to_bytes32());
    message
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::HashAlgorithm;

    fn oid(byte: u8) -> Oid {
        Oid::new(HashAlgorithm::Sha256, vec![byte; 32]).unwrap()
    }

    #[test]
    fn tag_message_is_deterministic_and_binds_fields() {
        let repo = [1u8; 32];
        let name = [2u8; 32];
        let msg = tag_message(&repo, &name, &oid(3), &oid(4));
        assert_eq!(msg, tag_message(&repo, &name, &oid(3), &oid(4)));
        assert!(msg.starts_with(TAG_DOMAIN));
        assert_ne!(msg, tag_message(&repo, &name, &oid(9), &oid(4)));
        assert_ne!(msg, tag_message(&repo, &name, &oid(3), &oid(9)));
        assert_eq!(msg.len(), TAG_DOMAIN.len() + 32 * 4);
    }
}
