//! `update_branch` — fast-forward or merge a branch head (§9.2, §7.2, §7.4).
//!
//! The only mutable ref path in the protocol. Concurrency is controlled by
//! comparing `expected_head_seq` to the stored `head_seq` (optimistic
//! concurrency), so racing pushes are resolved by a compare-and-swap: the loser
//! must refetch, rebase locally, re-sign, and retry (§7.4).
//!
//! The update is authorized by a wallet signature over
//! [`forge_object::branch::branch_update_message`] binding the repository,
//! branch name, new head, and expected sequence.

use anchor_lang::prelude::*;
use forge_object::{branch::branch_update_message, HashAlgorithm, Oid};

use crate::constants::{ED25519_PROGRAM_ID, INSTRUCTIONS_SYSVAR_ID};
use crate::ed25519::verify_ed25519_instruction_preceding;
use crate::errors::ForgeError;
use crate::events::BranchUpdated;
use crate::refs::{is_fast_forward_or_merge, load_commit, require_branch_binding};
use crate::state::{BranchAccount, RepositoryAccount};

/// Accounts for [`crate::forge_repository::update_branch`].
#[event_cpi]
#[derive(Accounts)]
#[instruction(new_head: [u8; 32], expected_head_seq: u64)]
pub struct UpdateBranch<'info> {
    /// Branch update authority (MVP: the repository owner).
    pub authority: Signer<'info>,

    /// Repository the branch belongs to.
    pub repository: Account<'info, RepositoryAccount>,

    /// The branch to advance; must be the canonical `["branch", repo, name]` PDA.
    #[account(mut)]
    pub branch: Account<'info, BranchAccount>,

    /// New head commit account.
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

/// Handler for `update_branch`.
///
/// # Errors
/// - [`ForgeError::Unauthorized`] if the authority is not the repository owner.
/// - [`ForgeError::InvalidPda`] / [`ForgeError::UnknownCommit`] for a bad branch
///   or commit account.
/// - [`ForgeError::StaleBranchHead`] if `expected_head_seq` is out of date.
/// - [`ForgeError::NonFastForward`] if the new head is not a descendant/merge.
/// - [`ForgeError::InvalidEd25519Instruction`] / [`ForgeError::BadSignature`]
///   if the authorization signature is missing or wrong.
/// - [`ForgeError::MathOverflow`] on `head_seq` overflow.
pub fn handler(
    ctx: Context<UpdateBranch>,
    new_head: [u8; 32],
    expected_head_seq: u64,
) -> Result<()> {
    let authority_key = ctx.accounts.authority.key();
    let repo_key = ctx.accounts.repository.key();
    crate::refs::require_min_role(
        &repo_key,
        &ctx.accounts.repository.owner,
        &authority_key,
        crate::state::permission::ROLE_MAINTAINER,
        ctx.remaining_accounts.first(),
    )?;

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
    let new_commit_info = ctx.accounts.new_commit.to_account_info();
    let commit = load_commit(&repo_key, &new_head, &new_commit_info)?;
    require!(
        is_fast_forward_or_merge(&old_head, &commit),
        ForgeError::NonFastForward
    );

    let new_head_oid = Oid::new(HashAlgorithm::Sha256, new_head.to_vec())
        .map_err(|_| error!(ForgeError::InvalidCommitOid))?;
    let message = branch_update_message(
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

    emit_cpi!(BranchUpdated {
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
