//! `initialize_repository` — create a repository and its default branch (§9.2).
//!
//! End state: a `RepositoryAccount` exists at the deterministic PDA with a
//! genesis `history_root` and `commit_count == 0`, and an empty default
//! `BranchAccount` exists at its own PDA head pointing at the all-zero oid.
//!
//! The default branch account is created here (atomically) rather than by a
//! separate call so a repository is always usable in one transaction.
//!
//! Accounts are created with the guarded helper in [`crate::init`] rather than
//! Anchor `#[account(init)]`, so a duplicate name for the same owner returns
//! [`ForgeError::RepositoryAlreadyExists`] instead of the generic
//! system-program error. The address of each account is still pinned by a
//! `seeds` + `bump` constraint, and reinitialization is rejected.

use crate::constants::{BRANCH_SEED, PERMISSIONS_MODE_OWNER_ONLY, REPO_SEED, STORAGE_BACKEND_MAX};
use crate::errors::ForgeError;
use crate::events::RepositoryInitialized;
use crate::init::create_pda;
use crate::name::validate_name;
use crate::state::{BranchAccount, RepositoryAccount};
use anchor_lang::prelude::*;
use forge_object::{history::genesis_history_root, HashAlgorithm};

/// Accounts for [`crate::forge_repository::initialize_repository`].
///
/// `#[event_cpi]` appends the `event_authority` and `program` accounts used by
/// `emit_cpi!`; clients must therefore pass them after `system_program`.
#[event_cpi]
#[derive(Accounts)]
#[instruction(name: [u8; 32], default_branch: [u8; 32], storage_backend: u8, flags: u16)]
pub struct InitializeRepository<'info> {
    /// Repository owner and rent payer.
    #[account(mut)]
    pub owner: Signer<'info>,

    /// The repository PDA: `["repo", owner, name]`.
    ///
    /// CHECK: created in the handler via [`create_pda`] so duplicate creation
    /// returns [`ForgeError::RepositoryAlreadyExists`]. The address is pinned by
    /// the seeds constraint and the account is validated by `create_account`.
    #[account(
        mut,
        seeds = [REPO_SEED, owner.key().as_ref(), name.as_ref()],
        bump
    )]
    pub repository: UncheckedAccount<'info>,

    /// The empty default branch: `["branch", repository, default_branch]`.
    ///
    /// CHECK: created in the handler via [`create_pda`] so duplicate creation
    /// returns [`ForgeError::BranchAlreadyExists`]. Pinned by the seeds
    /// constraint.
    #[account(
        mut,
        seeds = [BRANCH_SEED, repository.key().as_ref(), default_branch.as_ref()],
        bump
    )]
    pub default_branch_account: UncheckedAccount<'info>,

    /// System program (account creation).
    pub system_program: Program<'info, System>,
}

/// Handler for `initialize_repository`.
///
/// # Errors
/// - [`ForgeError::InvalidName`] if either name is malformed.
/// - [`ForgeError::InvalidStorageBackend`] if the backend tag is unknown.
/// - [`ForgeError::RepositoryAlreadyExists`] / [`ForgeError::BranchAlreadyExists`]
///   if the PDA is already initialized.
pub fn handler(
    ctx: Context<InitializeRepository>,
    name: [u8; 32],
    default_branch: [u8; 32],
    storage_backend: u8,
    flags: u16,
) -> Result<()> {
    validate_name(&name)?;
    validate_name(&default_branch)?;
    require!(
        storage_backend <= STORAGE_BACKEND_MAX,
        ForgeError::InvalidStorageBackend
    );

    let owner_key = ctx.accounts.owner.key();
    let repo_key = ctx.accounts.repository.key();
    let slot = Clock::get()?.slot;

    // §5.6: history_root_0 = H("forge-genesis\0" || repo_pda). The single
    // source of truth for this computation is `forge-object`, never a
    // re-implementation here.
    let history_root =
        genesis_history_root(&repo_key.to_bytes(), HashAlgorithm::Sha256).to_bytes32();

    let repository = RepositoryAccount {
        owner: owner_key,
        repo_id: repo_key,
        name,
        default_branch,
        history_root,
        commit_count: 0,
        contributor_count: 0,
        storage_backend,
        flags,
        bump: ctx.bumps.repository,
        created_slot: slot,
        _reserved: [0u8; 64],
    };
    let repo_seeds: &[&[u8]] = &[
        REPO_SEED,
        owner_key.as_ref(),
        name.as_ref(),
        &[ctx.bumps.repository],
    ];
    create_pda(
        &ctx.accounts.owner.to_account_info(),
        &ctx.accounts.repository.to_account_info(),
        repo_seeds,
        8 + RepositoryAccount::LEN,
        ForgeError::RepositoryAlreadyExists,
        &repository,
    )?;

    let branch = BranchAccount {
        repo: repo_key,
        name: default_branch,
        head_commit: [0u8; 32],
        head_seq: 0,
        authority: owner_key,
        permissions_mode: PERMISSIONS_MODE_OWNER_ONLY,
        protected: 0,
        bump: ctx.bumps.default_branch_account,
        updated_slot: slot,
        _reserved: [0u8; 32],
    };
    let branch_seeds: &[&[u8]] = &[
        BRANCH_SEED,
        repo_key.as_ref(),
        default_branch.as_ref(),
        &[ctx.bumps.default_branch_account],
    ];
    create_pda(
        &ctx.accounts.owner.to_account_info(),
        &ctx.accounts.default_branch_account.to_account_info(),
        branch_seeds,
        8 + BranchAccount::LEN,
        ForgeError::BranchAlreadyExists,
        &branch,
    )?;

    emit_cpi!(RepositoryInitialized {
        repository: repo_key,
        owner: owner_key,
        name,
        default_branch,
        history_root,
        slot,
    });

    Ok(())
}
