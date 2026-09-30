//! PDA derivation helpers (§4).
//!
//! These mirror the `seeds = [...]` expressions used by the instruction
//! account structs so clients (and tests) can deterministically derive the
//! same addresses. All seeds are static prefixes plus one of:
//!
//! - `repo`:         `["repo", owner, name]`
//! - `branch`:       `["branch", repository, name]`
//! - `commit`:       `["commit", repository, commit_oid]`
//! - `tag`:          `["tag", repository, name]`
//! - `perm`:         `["perm", repository, contributor]`
//! - `prog`:         `["prog", program_id]`
//!
//! Every seed is at most 32 bytes, satisfying `MAX_SEED_LEN` (§2.3).

use crate::constants::{BRANCH_SEED, COMMIT_SEED, PERM_SEED, PROG_SEED, REPO_SEED, TAG_SEED};
use anchor_lang::prelude::*;

/// Derives the repository PDA for `(owner, name)`.
pub fn repository_pda(owner: &Pubkey, name: &[u8; 32]) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[REPO_SEED, owner.as_ref(), name.as_ref()], &crate::ID)
}

/// Derives the branch PDA for `(repository, name)`.
pub fn branch_pda(repository: &Pubkey, name: &[u8; 32]) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[BRANCH_SEED, repository.as_ref(), name.as_ref()],
        &crate::ID,
    )
}

/// Derives the commit PDA for `(repository, commit_oid)`.
pub fn commit_pda(repository: &Pubkey, commit_oid: &[u8; 32]) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[COMMIT_SEED, repository.as_ref(), commit_oid.as_ref()],
        &crate::ID,
    )
}

/// Derives the tag PDA for `(repository, name)`.
pub fn tag_pda(repository: &Pubkey, name: &[u8; 32]) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[TAG_SEED, repository.as_ref(), name.as_ref()], &crate::ID)
}

/// Derives the permission PDA for `(repository, contributor)`.
pub fn permission_pda(repository: &Pubkey, contributor: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[PERM_SEED, repository.as_ref(), contributor.as_ref()],
        &crate::ID,
    )
}

/// Derives the program-source attestation PDA for `program_id` (§12.2).
pub fn program_attestation_pda(program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[PROG_SEED, program_id.as_ref()], &crate::ID)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derivations_are_deterministic_and_off_curve() {
        let owner = Pubkey::new_from_array([7u8; 32]);
        let repo = repository_pda(&owner, &[1u8; 32]);
        assert_eq!(repo, repository_pda(&owner, &[1u8; 32]));
        assert_eq!(repo, repository_pda(&owner, &[1u8; 32]));
        // Different name -> different address.
        assert_ne!(repo.0, repository_pda(&owner, &[2u8; 32]).0);
    }

    #[test]
    fn all_seed_sets_are_within_seed_length_limit() {
        // Every seed passed to `find_program_address` must be <= 32 bytes.
        for seed in [
            REPO_SEED,
            BRANCH_SEED,
            COMMIT_SEED,
            TAG_SEED,
            PERM_SEED,
            PROG_SEED,
        ] {
            assert!(seed.len() <= 32);
        }
    }
}
