//! `create_branch` — create a mutable branch ref (§9.2, §7.2).
//!
//! MVP authorization is owner-only: the `authority` signer must be the
//! repository owner. The branch records that signer as its update authority
//! (a program-owned PDA signs through a CPI in Phase 9, §16.2). Phase 9 adds
//! allowlist and authority-PDA modes.
//!
//! When `from_commit` is non-zero it must point at an existing `CommitAccount`
//! for the repository. The commit account is supplied through
//! `remaining_accounts`; Phase 3 has no commits yet, so only the empty-branch
//! path is exercised until Phase 4. Using `remaining_accounts` keeps the fixed
//! account interface stable across phases.

use crate::constants::{BRANCH_SEED, PERMISSIONS_MODE_OWNER_ONLY};
use crate::errors::ForgeError;
use crate::events::BranchCreated;
use crate::init::create_pda;
use crate::name::validate_name;
use crate::state::{BranchAccount, CommitAccount, RepositoryAccount};
use anchor_lang::prelude::*;

/// Accounts for [`crate::forge_repository::create_branch`].
#[event_cpi]
#[derive(Accounts)]
#[instruction(name: [u8; 32], from_commit: [u8; 32])]
pub struct CreateBranch<'info> {
    /// Branch update authority; must be the repository owner for the MVP.
    #[account(mut)]
    pub authority: Signer<'info>,

    /// Repository the branch belongs to.
    pub repository: Account<'info, RepositoryAccount>,

    /// The branch PDA: `["branch", repository, name]`.
    ///
    /// CHECK: created in the handler via [`create_pda`] so duplicate creation
    /// returns [`ForgeError::BranchAlreadyExists`]. Pinned by the seeds
    /// constraint.
    #[account(
        mut,
        seeds = [BRANCH_SEED, repository.key().as_ref(), name.as_ref()],
        bump
    )]
    pub branch: UncheckedAccount<'info>,

    /// System program (account creation).
    pub system_program: Program<'info, System>,
}

/// Handler for `create_branch`.
///
/// # Errors
/// - [`ForgeError::InvalidName`] if the name is malformed.
/// - [`ForgeError::Unauthorized`] if the signer is not the repository owner.
/// - [`ForgeError::BranchAlreadyExists`] if the branch PDA is already initialized.
/// - [`ForgeError::UnknownCommit`] / [`ForgeError::InvalidPda`] if
///   `from_commit` is non-zero and the commit account is missing or wrong.
pub fn handler(ctx: Context<CreateBranch>, name: [u8; 32], from_commit: [u8; 32]) -> Result<()> {
    validate_name(&name)?;

    let authority_key = ctx.accounts.authority.key();
    let repo_key = ctx.accounts.repository.key();

    // Owner-only authorization (§16.4). Phase 9 replaces this with role checks.
    crate::auth::require_repo_owner(&ctx.accounts.repository, &authority_key)?;

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
    let branch = BranchAccount {
        repo: repo_key,
        name,
        head_commit: from_commit,
        head_seq: 0,
        authority: authority_key,
        permissions_mode: PERMISSIONS_MODE_OWNER_ONLY,
        protected: 0,
        bump: ctx.bumps.branch,
        updated_slot: slot,
        _reserved: [0u8; 32],
    };
    let seeds: &[&[u8]] = &[
        BRANCH_SEED,
        repo_key.as_ref(),
        name.as_ref(),
        &[ctx.bumps.branch],
    ];
    create_pda(
        &ctx.accounts.authority.to_account_info(),
        &ctx.accounts.branch.to_account_info(),
        seeds,
        8 + BranchAccount::LEN,
        ForgeError::BranchAlreadyExists,
        &branch,
    )?;

    emit_cpi!(BranchCreated {
        repository: repo_key,
        name,
        from_commit,
        head_seq: 0,
        authority: authority_key,
        slot,
    });

    Ok(())
}
