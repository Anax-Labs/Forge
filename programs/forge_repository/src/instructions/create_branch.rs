//! `create_branch` — create a mutable branch ref (§9.2, §7.2).
//!
//! MVP authorization is owner-only. The `authority` argument records the
//! intended update authority (a wallet, or a program-owned PDA in Phase 9) but
//! is not yet enforced beyond owner checks; Phase 9 adds allowlist and
//! authority-PDA modes (§16.4).
//!
//! When `from_commit` is non-zero it must point at an existing `CommitAccount`
//! for the repository. The commit account is supplied through
//! `remaining_accounts`; Phase 3 has no commits yet, so only the empty-branch
//! path (`from_commit == 0`) is exercisable until Phase 4. Using
//! `remaining_accounts` keeps the fixed account interface stable across phases.

use crate::constants::{BRANCH_SEED, PERMISSIONS_MODE_OWNER_ONLY};
use crate::errors::ForgeError;
use crate::events::BranchCreated;
use crate::name::validate_name;
use crate::state::{BranchAccount, CommitAccount, RepositoryAccount};
use anchor_lang::prelude::*;

/// Accounts for [`crate::forge_repository::create_branch`].
#[event_cpi]
#[derive(Accounts)]
#[instruction(name: [u8; 32], from_commit: [u8; 32], authority: Pubkey)]
pub struct CreateBranch<'info> {
    /// Signer; must be the repository owner for the MVP.
    #[account(mut)]
    pub signer: Signer<'info>,

    /// Repository the branch belongs to.
    pub repository: Account<'info, RepositoryAccount>,

    /// The branch PDA: `["branch", repository, name]`.
    #[account(
        init,
        payer = signer,
        space = 8 + BranchAccount::LEN,
        seeds = [BRANCH_SEED, repository.key().as_ref(), name.as_ref()],
        bump
    )]
    pub branch: Account<'info, BranchAccount>,

    /// System program (account creation).
    pub system_program: Program<'info, System>,
}

/// Handler for `create_branch`.
///
/// # Errors
/// - [`ForgeError::InvalidName`] if the name is malformed.
/// - [`ForgeError::Unauthorized`] if the signer is not the repository owner.
/// - [`ForgeError::UnknownCommit`] / [`ForgeError::InvalidPda`] if
///   `from_commit` is non-zero and the commit account is missing or wrong.
pub fn handler(
    ctx: Context<CreateBranch>,
    name: [u8; 32],
    from_commit: [u8; 32],
    authority: Pubkey,
) -> Result<()> {
    validate_name(&name)?;

    let signer_key = ctx.accounts.signer.key();
    let repo_key = ctx.accounts.repository.key();

    // Owner-only authorization (§16.4). Phase 9 replaces this with role checks.
    crate::auth::require_repo_owner(&ctx.accounts.repository, &signer_key)?;

    // If branching from a commit, validate the commit account passed via
    // remaining_accounts. Validate owner + discriminator (via Account::try_from),
    // canonical PDA, repository binding, and oid match (§6.4, §11).
    if from_commit != [0u8; 32] {
        require_eq!(ctx.remaining_accounts.len(), 1, ForgeError::UnknownCommit);
        let commit_info = &ctx.remaining_accounts[0];
        let (expected_commit, _bump) = crate::pda::commit_pda(&repo_key, &from_commit);
        require_keys_eq!(*commit_info.key, expected_commit, ForgeError::InvalidPda);

        let commit = Account::<CommitAccount>::try_from(commit_info)?;
        require_keys_eq!(commit.repo, repo_key, ForgeError::UnknownCommit);
        require!(commit.commit_oid == from_commit, ForgeError::UnknownCommit);
    }

    let slot = Clock::get()?.slot;
    // The authority records the intended writer; default to the signer.
    let branch_authority = if authority == Pubkey::default() {
        signer_key
    } else {
        authority
    };

    {
        let branch = &mut ctx.accounts.branch;
        branch.repo = repo_key;
        branch.name = name;
        branch.head_commit = from_commit;
        branch.head_seq = 0;
        branch.authority = branch_authority;
        branch.permissions_mode = PERMISSIONS_MODE_OWNER_ONLY;
        branch.protected = 0;
        branch.bump = ctx.bumps.branch;
        branch.updated_slot = slot;
    }

    emit_cpi!(BranchCreated {
        repository: repo_key,
        name,
        from_commit,
        head_seq: 0,
        authority: branch_authority,
        slot,
    });

    Ok(())
}
