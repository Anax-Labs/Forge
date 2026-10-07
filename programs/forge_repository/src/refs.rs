//! Shared validation helpers for branch-ref operations (§6, §7).
//!
//! Branch refs are mutated by `update_branch`, `reset_branch`, `merge`, and
//! `delete_branch`. All of them must (a) confirm the passed branch account is
//! the canonical `["branch", repo, name]` PDA and (b) confirm any referenced
//! commit is a real commit of the repository.

use anchor_lang::prelude::*;
use anchor_lang::{AccountDeserialize, Discriminator};

use crate::errors::ForgeError;
use crate::state::{CommitAccount, PermissionAccount};

/// Requires `branch_key` to be the canonical `["branch", repo, name]` PDA.
///
/// # Errors
/// Returns [`ForgeError::InvalidPda`] if the seeds do not derive `branch_key`.
pub fn require_branch_binding(repo: &Pubkey, name: &[u8; 32], branch_key: &Pubkey) -> Result<()> {
    let (expected, _) = crate::pda::branch_pda(repo, name);
    require_keys_eq!(*branch_key, expected, ForgeError::InvalidPda);
    Ok(())
}

/// Loads and validates the `CommitAccount` for `expected_oid` in `repo`.
///
/// # Errors
/// Returns [`ForgeError::InvalidPda`] / [`ForgeError::UnknownCommit`].
pub fn load_commit(
    repo: &Pubkey,
    expected_oid: &[u8; 32],
    info: &AccountInfo,
) -> Result<CommitAccount> {
    let (expected_pda, _) = crate::pda::commit_pda(repo, expected_oid);
    require_keys_eq!(*info.key, expected_pda, ForgeError::InvalidPda);
    require_keys_eq!(*info.owner, crate::ID, ForgeError::UnknownCommit);

    let data = info.try_borrow_data()?;
    require!(
        data.len() >= 8 + CommitAccount::LEN,
        ForgeError::UnknownCommit
    );
    require!(
        &data[..8] == CommitAccount::DISCRIMINATOR,
        ForgeError::UnknownCommit
    );
    // `try_deserialize` expects the discriminator-prefixed buffer, so pass the
    // full slice (it skips the 8-byte discriminator itself).
    let commit = CommitAccount::try_deserialize(&mut &data[..])?;
    require_keys_eq!(commit.repo, *repo, ForgeError::UnknownCommit);
    require!(
        commit.commit_oid == *expected_oid,
        ForgeError::UnknownCommit
    );
    Ok(commit)
}

/// Loads and validates the `PermissionAccount` for `contributor` in `repo`.
///
/// # Errors
/// Returns [`ForgeError::InvalidPda`] / [`ForgeError::Unauthorized`] when the
/// account is not the canonical permission PDA for this contributor.
pub fn load_permission(
    repo: &Pubkey,
    contributor: &Pubkey,
    info: &AccountInfo,
) -> Result<PermissionAccount> {
    let (expected_pda, _) = crate::pda::permission_pda(repo, contributor);
    require_keys_eq!(*info.key, expected_pda, ForgeError::InvalidPda);
    require_keys_eq!(*info.owner, crate::ID, ForgeError::Unauthorized);

    let data = info.try_borrow_data()?;
    require!(
        data.len() >= 8 + PermissionAccount::LEN,
        ForgeError::Unauthorized
    );
    require!(
        &data[..8] == PermissionAccount::DISCRIMINATOR,
        ForgeError::Unauthorized
    );
    let permission = PermissionAccount::try_deserialize(&mut &data[..])?;
    require_keys_eq!(permission.repo, *repo, ForgeError::Unauthorized);
    require_keys_eq!(
        permission.contributor,
        *contributor,
        ForgeError::Unauthorized
    );
    Ok(permission)
}

/// Authorization for a repository action requiring at least `min_role` (§7.5).
///
/// The repository owner is always authorized. Otherwise the optional permission
/// account (passed as a remaining account) must be the signer's canonical
/// [`PermissionAccount`], unexpired, with `role >= min_role`.
///
/// # Errors
/// - [`ForgeError::Unauthorized`] if the signer has no valid permission entry.
/// - [`ForgeError::InsufficientRole`] if the role is below `min_role`.
pub fn require_min_role(
    repo: &Pubkey,
    owner: &Pubkey,
    signer: &Pubkey,
    min_role: u8,
    permission: Option<&AccountInfo>,
) -> Result<()> {
    if owner == signer {
        return Ok(());
    }
    let info = permission.ok_or(ForgeError::Unauthorized)?;
    let permission = load_permission(repo, signer, info)?;
    let clock = Clock::get()?;
    if permission.expires_slot != 0 && clock.slot > permission.expires_slot {
        return Err(ForgeError::Unauthorized.into());
    }
    require!(permission.role >= min_role, ForgeError::InsufficientRole);
    Ok(())
}

/// Whether a new commit is a valid fast-forward or merge of `old_head`.
///
/// An all-zero `old_head` (empty branch) accepts any existing commit, matching
/// `create_branch from_commit` semantics (§7.2). Otherwise the new commit must
/// have `old_head` as its first parent (fast-forward) or as one of its two
/// parents (merge, §7.2/§7.3).
pub fn is_fast_forward_or_merge(old_head: &[u8; 32], commit: &CommitAccount) -> bool {
    if *old_head == [0u8; 32] {
        return true;
    }
    match commit.parent_count {
        1 => commit.parent_a == *old_head,
        2 => commit.parent_a == *old_head || commit.parent_b == *old_head,
        _ => false,
    }
}
