//! Append-only repository history commitment and repo-root fingerprint (§5.6).
//!
//! ```text
//! history_root_0 = H("forge-genesis\0" || repo_pda)
//! history_root_n = H("forge-append\0" || history_root_{n-1} || commit_oid_n || seq_n)
//! repo_root      = H("forge-repo\0" || repo_pda || default_branch_head || history_root || commit_count)
//! ```
//!
//! `repo_pda` is a 32-byte Solana address. Oids are folded in through their
//! canonical 32-byte form (see [`crate::hash`]), and `seq` / `commit_count` are
//! encoded as little-endian `u64`. This endianness and the SHA-1 padding scheme
//! are frozen protocol decisions (the spec leaves them open).

use crate::error::ObjectError;
use crate::hash::{HashAlgorithm, Oid};

/// Domain-separation prefix for the genesis history root.
pub const GENESIS_DOMAIN: &[u8] = b"forge-genesis\0";
/// Domain-separation prefix for each append step.
pub const APPEND_DOMAIN: &[u8] = b"forge-append\0";
/// Domain-separation prefix for the repository root fingerprint.
pub const REPO_ROOT_DOMAIN: &[u8] = b"forge-repo\0";

/// Computes `history_root_0` for a repository.
pub fn genesis_history_root(repo_pda: &[u8; 32], algorithm: HashAlgorithm) -> Oid {
    let mut preimage = Vec::with_capacity(GENESIS_DOMAIN.len() + 32);
    preimage.extend_from_slice(GENESIS_DOMAIN);
    preimage.extend_from_slice(repo_pda);
    Oid::new(algorithm, algorithm.digest(&preimage)).expect("digest length matches algorithm")
}

/// Advances the history root by one commit.
///
/// # Errors
/// Returns [`ObjectError::AlgorithmMismatch`] when `previous` or `commit_oid`
/// does not use `algorithm`.
pub fn append_history_root(
    algorithm: HashAlgorithm,
    previous: &Oid,
    commit_oid: &Oid,
    seq: u64,
) -> Result<Oid, ObjectError> {
    previous.ensure_algorithm(algorithm)?;
    commit_oid.ensure_algorithm(algorithm)?;
    let mut preimage = Vec::with_capacity(APPEND_DOMAIN.len() + 32 + 32 + 8);
    preimage.extend_from_slice(APPEND_DOMAIN);
    preimage.extend_from_slice(&previous.to_bytes32());
    preimage.extend_from_slice(&commit_oid.to_bytes32());
    preimage.extend_from_slice(&seq.to_le_bytes());
    Ok(Oid::new(algorithm, algorithm.digest(&preimage)).expect("digest length matches algorithm"))
}

/// Recomputes the full history root from the genesis plus `(commit, seq)` pairs
/// in insertion order. Used for O(n) inclusion proofs and verification.
///
/// # Errors
/// Propagates [`ObjectError::AlgorithmMismatch`] from
/// [`append_history_root`].
pub fn history_root_chain(
    algorithm: HashAlgorithm,
    repo_pda: &[u8; 32],
    commits: &[(Oid, u64)],
) -> Result<Oid, ObjectError> {
    let mut root = genesis_history_root(repo_pda, algorithm);
    for (commit, seq) in commits {
        root = append_history_root(algorithm, &root, commit, *seq)?;
    }
    Ok(root)
}

/// Computes the single 32-byte repository fingerprint.
///
/// `default_branch_head` may be [`Oid::zero`] for an empty repository.
///
/// # Errors
/// Returns [`ObjectError::AlgorithmMismatch`] when the head or history root
/// does not use `algorithm`.
#[allow(clippy::too_many_arguments)]
pub fn repo_root(
    algorithm: HashAlgorithm,
    repo_pda: &[u8; 32],
    default_branch_head: &Oid,
    history_root: &Oid,
    commit_count: u64,
) -> Result<Oid, ObjectError> {
    default_branch_head.ensure_algorithm(algorithm)?;
    history_root.ensure_algorithm(algorithm)?;
    let mut preimage = Vec::with_capacity(REPO_ROOT_DOMAIN.len() + 32 + 32 + 32 + 8);
    preimage.extend_from_slice(REPO_ROOT_DOMAIN);
    preimage.extend_from_slice(repo_pda);
    preimage.extend_from_slice(&default_branch_head.to_bytes32());
    preimage.extend_from_slice(&history_root.to_bytes32());
    preimage.extend_from_slice(&commit_count.to_le_bytes());
    Ok(Oid::new(algorithm, algorithm.digest(&preimage)).expect("digest length matches algorithm"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oid(byte: u8) -> Oid {
        Oid::new(HashAlgorithm::Sha256, vec![byte; 32]).unwrap()
    }

    #[test]
    fn genesis_is_deterministic() {
        let repo = [0x11u8; 32];
        assert_eq!(
            genesis_history_root(&repo, HashAlgorithm::Sha256),
            genesis_history_root(&repo, HashAlgorithm::Sha256)
        );
    }

    #[test]
    fn chain_matches_stepwise_append() {
        let repo = [0x22u8; 32];
        let c1 = oid(1);
        let c2 = oid(2);
        let stepwise = append_history_root(
            HashAlgorithm::Sha256,
            &append_history_root(
                HashAlgorithm::Sha256,
                &genesis_history_root(&repo, HashAlgorithm::Sha256),
                &c1,
                0,
            )
            .unwrap(),
            &c2,
            1,
        )
        .unwrap();
        let chained =
            history_root_chain(HashAlgorithm::Sha256, &repo, &[(c1, 0), (c2, 1)]).unwrap();
        assert_eq!(stepwise, chained);
    }

    #[test]
    fn seq_changes_root() {
        let repo = [0x33u8; 32];
        let base = genesis_history_root(&repo, HashAlgorithm::Sha256);
        let commit = oid(9);
        let a = append_history_root(HashAlgorithm::Sha256, &base, &commit, 0).unwrap();
        let b = append_history_root(HashAlgorithm::Sha256, &base, &commit, 1).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn cross_algorithm_rejected() {
        let repo = [0x44u8; 32];
        let genesis = genesis_history_root(&repo, HashAlgorithm::Sha256);
        let sha1_commit = Oid::new(HashAlgorithm::Sha1, vec![0u8; 20]).unwrap();
        assert!(append_history_root(HashAlgorithm::Sha256, &genesis, &sha1_commit, 0).is_err());
    }

    #[test]
    fn repo_root_changes_with_count() {
        let repo = [0x55u8; 32];
        let head = oid(3);
        let history = genesis_history_root(&repo, HashAlgorithm::Sha256);
        let a = repo_root(HashAlgorithm::Sha256, &repo, &head, &history, 0).unwrap();
        let b = repo_root(HashAlgorithm::Sha256, &repo, &head, &history, 1).unwrap();
        assert_ne!(a, b);
    }
}
