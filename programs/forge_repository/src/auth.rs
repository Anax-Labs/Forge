//! Authorization helpers (§11, §16.4).
//!
//! MVP authorization is owner-only. Phase 9 extends this module with
//! contributor allowlists and program-owned authority PDAs; keeping the check
//! behind a function means the enforcement sites do not change later.

use crate::errors::ForgeError;
use crate::state::RepositoryAccount;
use anchor_lang::prelude::*;

/// Requires `signer` to be the repository owner.
///
/// # Errors
/// Returns [`ForgeError::Unauthorized`] when the keys differ.
pub fn require_repo_owner(repository: &RepositoryAccount, signer: &Pubkey) -> Result<()> {
    require_keys_eq!(repository.owner, *signer, ForgeError::Unauthorized);
    Ok(())
}
