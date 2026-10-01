//! `reset_branch` — explicit, logged non-fast-forward branch rewrite (§7.3).
//!
//! A reset moves a branch head to a commit that is **not** a descendant, e.g.
//! after a local rebase. It is deliberately a distinct instruction from
//! [`crate::forge_repository::update_branch`] so history rewrites are visible:
//! it emits [`crate::events::BranchReset`] and never touches the append-only
//! repository `history_root` (§11 #4/#9). MVP authorization is owner-only;
//! Phase 9 restricts it to maintainer/admin roles.

use anchor_lang::prelude::*;
use forge_object::{branch::branch_reset_message, HashAlgorithm, Oid};

use crate::constants::{ED25519_PROGRAM_ID, INSTRUCTIONS_SYSVAR_ID};
use crate::ed25519::verify_ed25519_instruction_preceding;
use crate::errors::ForgeError;
use crate::events::BranchReset;
use crate::refs::{load_commit, require_branch_binding};
use crate::state::{BranchAccount, RepositoryAccount};

/// Accounts for [`crate::forge_repository::reset_branch`].
#[event_cpi]
#[derive(Accounts)]
#[instruction(new_head: [u8; 32], expected_head_seq: u64)]
pub struct ResetBranch<'info> {
    /// Reset authority (MVP: the repository owner).
    pub authority: Signer<'info>,

    /// Repository the branch belongs to.
    pub repository: Account<'info, RepositoryAccount>,

    /// The branch to reset; must be the canonical `["branch", repo, name]` PDA.
    #[account(mut)]
    pub branch: Account<'info, BranchAccount>,

    /// New head commit (need not descend from the current head).
    /// CHECK: validated in the handler (canonical PDA, repository, oid).
    pub new_commit: UncheckedAccount<'info>,

    /// Instructions sysvar (Ed25519 introspection, §9.4).
    /// CHECK: address constraint
    #[account(address = INSTRUCTIONS_SYSVAR_ID)]
    pub instructions_sysvar: UncheckedAccount<'info>,

    /// Ed25519 native verifier program id (§9.4).
    /// CHECK: address constraint
    #[account(address = ED25519_PROGRAM_ID)]
    pub ed25519_program: UncheckedAccount<'info>,
}

/// Handler for `reset_branch`.
///
/// # Errors
/// - [`ForgeError::Unauthorized`] if the authority is not the repository owner.
/// - [`ForgeError::InvalidPda`] / [`ForgeError::UnknownCommit`] for a bad branch
///   or commit account.
/// - [`ForgeError::StaleBranchHead`] if `expected_head_seq` is out of date.
/// - [`ForgeError::InvalidEd25519Instruction`] / [`ForgeError::BadSignature`]
///   if the authorization signature is missing or wrong.
/// - [`ForgeError::MathOverflow`] on `head_seq` overflow.
pub fn handler(
    ctx: Context<ResetBranch>,
    new_head: [u8; 32],
    expected_head_seq: u64,
) -> Result<()> {
    let authority_key = ctx.accounts.authority.key();
    let repo_key = ctx.accounts.repository.key();
    crate::auth::require_repo_owner(&ctx.accounts.repository, &authority_key)?;

    let (branch_name, old_head) = {
        let branch = &ctx.accounts.branch;
        require_keys_eq!(branch.repo, repo_key, ForgeError::InvalidPda);
        require_branch_binding(&repo_key, &branch.name, &branch.key())?;
        require_eq!(
            branch.head_seq,
            expected_head_seq,
            ForgeError::StaleBranchHead
        );
        (branch.name, branch.head_commit)
    };

    require!(new_head != [0u8; 32], ForgeError::InvalidCommitOid);
    // The reset target must still be a real commit of this repository.
    let new_commit_info = ctx.accounts.new_commit.to_account_info();
    let _commit = load_commit(&repo_key, &new_head, &new_commit_info)?;

    let new_head_oid = Oid::new(HashAlgorithm::Sha256, new_head.to_vec())
        .map_err(|_| error!(ForgeError::InvalidCommitOid))?;
    let message = branch_reset_message(
        &repo_key.to_bytes(),
        &branch_name,
        &new_head_oid,
        expected_head_seq,
    );
    verify_ed25519_instruction_preceding(
        &ctx.accounts.instructions_sysvar.to_account_info(),
        &authority_key,
        &message,
    )?;

    let slot = Clock::get()?.slot;
    let branch = &mut ctx.accounts.branch;
    branch.head_commit = new_head;
    branch.head_seq = expected_head_seq
        .checked_add(1)
        .ok_or(ForgeError::MathOverflow)?;
    branch.updated_slot = slot;

    emit_cpi!(BranchReset {
        repository: repo_key,
        name: branch_name,
        old_head,
        new_head,
        head_seq: branch.head_seq,
        actor: authority_key,
        slot,
    });

    Ok(())
}
