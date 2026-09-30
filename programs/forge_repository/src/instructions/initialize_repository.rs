//! `initialize_repository` — create a repository and its default branch (§9.2).
//!
//! End state: a `RepositoryAccount` exists at the deterministic PDA with a
//! genesis `history_root` and `commit_count == 0`, and an empty default
//! `BranchAccount` exists at its own PDA head pointing at the all-zero oid.
//!
//! The default branch account is created here (atomically) rather than by a
//! separate call so a repository is always usable in one transaction. Anchor
//! `init` is used for both accounts; `init_if_needed` is forbidden (§9.2).

use crate::constants::{BRANCH_SEED, PERMISSIONS_MODE_OWNER_ONLY, REPO_SEED, STORAGE_BACKEND_MAX};
use crate::errors::ForgeError;
use crate::events::RepositoryInitialized;
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
    #[account(
        init,
        payer = owner,
        space = 8 + RepositoryAccount::LEN,
        seeds = [REPO_SEED, owner.key().as_ref(), name.as_ref()],
        bump
    )]
    pub repository: Account<'info, RepositoryAccount>,

    /// The empty default branch: `["branch", repository, default_branch]`.
    #[account(
        init,
        payer = owner,
        space = 8 + BranchAccount::LEN,
        seeds = [BRANCH_SEED, repository.key().as_ref(), default_branch.as_ref()],
        bump
    )]
    pub default_branch_account: Account<'info, BranchAccount>,

    /// System program (account creation).
    pub system_program: Program<'info, System>,
}

/// Handler for `initialize_repository`.
///
/// # Errors
/// - [`ForgeError::InvalidName`] if either name is malformed.
/// - [`ForgeError::InvalidStorageBackend`] if the backend tag is unknown.
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

    {
        let repository = &mut ctx.accounts.repository;
        repository.owner = owner_key;
        repository.repo_id = repo_key;
        repository.name = name;
        repository.default_branch = default_branch;
        repository.history_root = history_root;
        repository.commit_count = 0;
        repository.contributor_count = 0;
        repository.storage_backend = storage_backend;
        repository.flags = flags;
        repository.bump = ctx.bumps.repository;
        repository.created_slot = slot;
    }

    {
        let branch = &mut ctx.accounts.default_branch_account;
        branch.repo = repo_key;
        branch.name = default_branch;
        branch.head_commit = [0u8; 32];
        branch.head_seq = 0;
        branch.authority = owner_key;
        branch.permissions_mode = PERMISSIONS_MODE_OWNER_ONLY;
        branch.protected = 0;
        branch.bump = ctx.bumps.default_branch_account;
        branch.updated_slot = slot;
    }

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
