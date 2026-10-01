//! Canonical signed messages for branch ref updates (§6.8, §7).
//!
//! Branch refs are advanced by a wallet-signed message rather than by trusting
//! the transaction sender. The message binds the repository, the branch name,
//! the new head commit, and the expected `head_seq` (the optimistic-concurrency
//! token), so a signature cannot be replayed after the ref advances.
//!
//! ```text
//! update = "forge-branch-update\0" || repo || name || new_head32 || expected_head_seq_le
//! reset  = "forge-branch-reset\0"  || repo || name || new_head32 || expected_head_seq_le
//! ```
//!
//! `repo` and `name` are 32-byte Solana addresses / NUL-padded names; `new_head32`
//! is the canonical 32-byte oid form (see [`crate::hash`]); `expected_head_seq`
//! is little-endian `u64`. Update and reset use distinct domains so a
//! fast-forward authorization cannot be replayed as a history rewrite.

use crate::error::ObjectError;
use crate::hash::Oid;

/// Domain-separation prefix for fast-forward/merge branch updates.
pub const BRANCH_UPDATE_DOMAIN: &[u8] = b"forge-branch-update\0";
/// Domain-separation prefix for explicit (non-fast-forward) branch resets.
pub const BRANCH_RESET_DOMAIN: &[u8] = b"forge-branch-reset\0";

fn encode(
    domain: &[u8],
    repo: &[u8; 32],
    name: &[u8; 32],
    new_head: &Oid,
    expected_head_seq: u64,
) -> Vec<u8> {
    let mut message = Vec::with_capacity(domain.len() + 32 + 32 + 32 + 8);
    message.extend_from_slice(domain);
    message.extend_from_slice(repo);
    message.extend_from_slice(name);
    message.extend_from_slice(&new_head.to_bytes32());
    message.extend_from_slice(&expected_head_seq.to_le_bytes());
    message
}

/// The message a wallet signs to fast-forward or merge `branch` to `new_head`.
pub fn branch_update_message(
    repo: &[u8; 32],
    name: &[u8; 32],
    new_head: &Oid,
    expected_head_seq: u64,
) -> Vec<u8> {
    encode(
        BRANCH_UPDATE_DOMAIN,
        repo,
        name,
        new_head,
        expected_head_seq,
    )
}

/// The message a wallet signs to reset `branch` to `new_head` (non-fast-forward).
pub fn branch_reset_message(
    repo: &[u8; 32],
    name: &[u8; 32],
    new_head: &Oid,
    expected_head_seq: u64,
) -> Vec<u8> {
    encode(BRANCH_RESET_DOMAIN, repo, name, new_head, expected_head_seq)
}

/// Convenience wrapper used by callers that validate the head oid separately.
///
/// # Errors
/// Returns [`ObjectError::AlgorithmMismatch`] only if `new_head` is somehow
/// inconsistent; currently infallible, kept for API symmetry with other
/// builders.
pub fn branch_update_message_checked(
    repo: &[u8; 32],
    name: &[u8; 32],
    new_head: &Oid,
    expected_head_seq: u64,
) -> Result<Vec<u8>, ObjectError> {
    Ok(branch_update_message(
        repo,
        name,
        new_head,
        expected_head_seq,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::HashAlgorithm;

    fn oid(byte: u8) -> Oid {
        Oid::new(HashAlgorithm::Sha256, vec![byte; 32]).unwrap()
    }

    #[test]
    fn update_message_is_deterministic_and_domain_separated() {
        let repo = [1u8; 32];
        let name = [2u8; 32];
        let head = oid(3);
        let update = branch_update_message(&repo, &name, &head, 7);
        assert_eq!(update, branch_update_message(&repo, &name, &head, 7));
        let reset = branch_reset_message(&repo, &name, &head, 7);
        assert_ne!(update, reset);
        assert!(update.starts_with(BRANCH_UPDATE_DOMAIN));
        assert!(reset.starts_with(BRANCH_RESET_DOMAIN));
    }

    #[test]
    fn expected_seq_is_bound() {
        let repo = [1u8; 32];
        let name = [2u8; 32];
        let head = oid(3);
        assert_ne!(
            branch_update_message(&repo, &name, &head, 0),
            branch_update_message(&repo, &name, &head, 1)
        );
    }

    #[test]
    fn name_and_head_are_bound() {
        let repo = [1u8; 32];
        let head = oid(3);
        assert_ne!(
            branch_update_message(&repo, &[2u8; 32], &head, 0),
            branch_update_message(&repo, &[4u8; 32], &head, 0)
        );
        assert_ne!(
            branch_update_message(&repo, &[2u8; 32], &head, 0),
            branch_update_message(&repo, &[2u8; 32], &oid(9), 0)
        );
    }

    #[test]
    fn message_layout_is_fixed() {
        let update = branch_update_message(&[0u8; 32], &[0u8; 32], &oid(0), 0);
        assert_eq!(update.len(), BRANCH_UPDATE_DOMAIN.len() + 32 + 32 + 32 + 8);
    }
}
