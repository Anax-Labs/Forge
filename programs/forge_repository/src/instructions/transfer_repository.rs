//! `transfer_repository` — transfer repository ownership (§9.2, §15).
//!
//! The `owner` field is mutable; the repository PDA itself is not. Because the
//! PDA seeds include the *original* owner (`["repo", owner, name]`), clients must
//! persist `RepositoryAccount.repo_id` and treat the PDA as stable across a
//! transfer (see ADR 0009). New owners therefore use the stored `repo_id`
//! rather than re-deriving from their key.

use anchor_lang::prelude::*;

use crate::events::RepositoryTransferred;
use crate::state::RepositoryAccount;

/// Accounts for [`crate::forge_repository::transfer_repository`].
#[event_cpi]
#[derive(Accounts)]
pub struct TransferRepository<'info> {
    /// Current owner; must sign.
    pub owner: Signer<'info>,

    /// Repository account being transferred.
    #[account(mut)]
    pub repository: Account<'info, RepositoryAccount>,
}

/// Handler for `transfer_repository`.
///
/// # Errors
/// - [`crate::errors::ForgeError::Unauthorized`] if the signer is not the owner.
pub fn handler(ctx: Context<TransferRepository>, new_owner: Pubkey) -> Result<()> {
    let actor = ctx.accounts.owner.key();
    crate::auth::require_repo_owner(&ctx.accounts.repository, &actor)?;

    let old_owner = ctx.accounts.repository.owner;
    ctx.accounts.repository.owner = new_owner;

    emit_cpi!(RepositoryTransferred {
        repository: ctx.accounts.repository.key(),
        old_owner,
        new_owner,
        actor,
        slot: Clock::get()?.slot,
    });

    Ok(())
}
