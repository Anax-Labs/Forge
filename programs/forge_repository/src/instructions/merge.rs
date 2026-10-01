//! `merge` — advance a target branch to a two-parent merge commit (§9.2, §6.6).
//!
//! The merge *content* is computed offchain by Git; the chain only validates the
//! topology: the merge commit must have exactly two parents, one equal to the
//! current target head and the other equal to the current source head. The
//! target is then advanced with the same optimistic-concurrency rule as
//! [`crate::forge_repository::update_branch`].

use anchor_lang::prelude::*;
use forge_object::{branch::branch_update_message, HashAlgorithm, Oid};

use crate::constants::{ED25519_PROGRAM_ID, INSTRUCTIONS_SYSVAR_ID};
use crate::ed25519::verify_ed25519_instruction_preceding;
use crate::errors::ForgeError;
use crate::events::BranchUpdated;
use crate::refs::{load_commit, require_branch_binding};
use crate::state::{BranchAccount, RepositoryAccount};

/// Accounts for [`crate::forge_repository::merge`].
#[event_cpi]
#[derive(Accounts)]
#[instruction(merge_commit_oid: [u8; 32], expected_target_seq: u64)]
pub struct Merge<'info> {
    /// Merge authority (MVP: the repository owner).
    pub authority: Signer<'info>,

    /// Repository both branches belong to.
    pub repository: Account<'info, RepositoryAccount>,

    /// Branch being advanced.
    #[account(mut)]
    pub target: Account<'info, BranchAccount>,

    /// Branch whose head is the second parent of the merge.
    pub source: Account<'info, BranchAccount>,

    /// The two-parent merge commit.
    /// CHECK: validated in the handler (canonical PDA, repository, oid, topology).
    pub merge_commit: UncheckedAccount<'info>,

    /// Instructions sysvar (Ed25519 introspection, §9.4).
    /// CHECK: address constraint
    #[account(address = INSTRUCTIONS_SYSVAR_ID)]
    pub instructions_sysvar: UncheckedAccount<'info>,

    /// Ed25519 native verifier program id (§9.4).
    /// CHECK: address constraint
    #[account(address = ED25519_PROGRAM_ID)]
    pub ed25519_program: UncheckedAccount<'info>,
}

/// Handler for `merge`.
///
/// # Errors
/// - [`ForgeError::Unauthorized`] if the authority is not the repository owner.
/// - [`ForgeError::InvalidPda`] / [`ForgeError::UnknownCommit`] for bad accounts.
/// - [`ForgeError::StaleBranchHead`] if `expected_target_seq` is out of date.
/// - [`ForgeError::InvalidMerge`] if the merge commit's parents do not match the
///   current target/source heads.
/// - [`ForgeError::InvalidEd25519Instruction`] / [`ForgeError::BadSignature`]
///   if the authorization signature is missing or wrong.
/// - [`ForgeError::MathOverflow`] on `head_seq` overflow.
pub fn handler(
    ctx: Context<Merge>,
    merge_commit_oid: [u8; 32],
    expected_target_seq: u64,
) -> Result<()> {
    let authority_key = ctx.accounts.authority.key();
    let repo_key = ctx.accounts.repository.key();
    crate::auth::require_repo_owner(&ctx.accounts.repository, &authority_key)?;

    let (target_name, target_head) = {
        let target = &ctx.accounts.target;
        require_keys_eq!(target.repo, repo_key, ForgeError::InvalidPda);
        require_branch_binding(&repo_key, &target.name, &target.key())?;
        require_eq!(
            target.head_seq,
            expected_target_seq,
            ForgeError::StaleBranchHead
        );
        (target.name, target.head_commit)
    };

    let source_head = {
        let source = &ctx.accounts.source;
        require_keys_eq!(source.repo, repo_key, ForgeError::InvalidPda);
        require_branch_binding(&repo_key, &source.name, &source.key())?;
        require_keys_neq!(
            source.key(),
            ctx.accounts.target.key(),
            ForgeError::InvalidMerge
        );
        source.head_commit
    };

    require!(
        target_head != [0u8; 32] && source_head != [0u8; 32] && merge_commit_oid != [0u8; 32],
        ForgeError::InvalidMerge
    );

    let merge_commit_info = ctx.accounts.merge_commit.to_account_info();
    let merge = load_commit(&repo_key, &merge_commit_oid, &merge_commit_info)?;
    require_eq!(merge.parent_count, 2, ForgeError::InvalidMerge);
    let matches = (merge.parent_a == target_head && merge.parent_b == source_head)
        || (merge.parent_a == source_head && merge.parent_b == target_head);
    require!(matches, ForgeError::InvalidMerge);

    let merge_oid = Oid::new(HashAlgorithm::Sha256, merge_commit_oid.to_vec())
        .map_err(|_| error!(ForgeError::InvalidMerge))?;
    let message = branch_update_message(
        &repo_key.to_bytes(),
        &target_name,
        &merge_oid,
        expected_target_seq,
    );
    verify_ed25519_instruction_preceding(
        &ctx.accounts.instructions_sysvar.to_account_info(),
        &authority_key,
        &message,
    )?;

    let slot = Clock::get()?.slot;
    let target = &mut ctx.accounts.target;
    target.head_commit = merge_commit_oid;
    target.head_seq = expected_target_seq
        .checked_add(1)
        .ok_or(ForgeError::MathOverflow)?;
    target.updated_slot = slot;

    emit_cpi!(BranchUpdated {
        repository: repo_key,
        name: target_name,
        old_head: target_head,
        new_head: merge_commit_oid,
        head_seq: target.head_seq,
        actor: authority_key,
        slot,
    });

    Ok(())
}
