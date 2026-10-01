//! `delete_branch` — remove a non-default branch ref (§7.2).
//!
//! The branch account is closed and its rent returned to the authority. History
//! is unaffected: the commit accounts and the append-only `history_root` remain.

use anchor_lang::prelude::*;

use crate::errors::ForgeError;
use crate::events::BranchDeleted;
use crate::refs::require_branch_binding;
use crate::state::{BranchAccount, RepositoryAccount};

/// Accounts for [`crate::forge_repository::delete_branch`].
#[event_cpi]
#[derive(Accounts)]
pub struct DeleteBranch<'info> {
    /// Delete authority and rent receiver (MVP: the repository owner).
    #[account(mut)]
    pub authority: Signer<'info>,

    /// Repository the branch belongs to.
    pub repository: Account<'info, RepositoryAccount>,

    /// The branch to delete; closed on success and rent refunded to `authority`.
    #[account(mut, close = authority)]
    pub branch: Account<'info, BranchAccount>,
}

/// Handler for `delete_branch`.
///
/// # Errors
/// - [`ForgeError::Unauthorized`] if the authority is not the repository owner.
/// - [`ForgeError::InvalidPda`] if the branch is not the canonical PDA.
/// - [`ForgeError::CannotDeleteDefaultBranch`] for the default branch.
pub fn handler(ctx: Context<DeleteBranch>) -> Result<()> {
    let authority_key = ctx.accounts.authority.key();
    let repo_key = ctx.accounts.repository.key();
    crate::auth::require_repo_owner(&ctx.accounts.repository, &authority_key)?;

    let (branch_name, old_head) = {
        let branch = &ctx.accounts.branch;
        require_keys_eq!(branch.repo, repo_key, ForgeError::InvalidPda);
        require_branch_binding(&repo_key, &branch.name, &branch.key())?;
        require!(
            branch.name != ctx.accounts.repository.default_branch,
            ForgeError::CannotDeleteDefaultBranch
        );
        (branch.name, branch.head_commit)
    };

    emit_cpi!(BranchDeleted {
        repository: repo_key,
        name: branch_name,
        old_head,
        actor: authority_key,
        slot: Clock::get()?.slot,
    });

    // The `close = authority` constraint zeroes and refunds the account on exit.
    Ok(())
}
