//! `create_commit` — anchor a signed commit and advance the history root (§9.2).
//!
//! Authorship is proven by introspecting the preceding Ed25519 native
//! instruction (§9.4). The signed message is the 32-byte `attestation_hash`;
//! the full attestation CBOR stays offchain (§5.5).

use crate::constants::{COMMIT_SEED, ED25519_PROGRAM_ID, INSTRUCTIONS_SYSVAR_ID};
use crate::ed25519::verify_ed25519_instruction_preceding;
use crate::errors::ForgeError;
use crate::events::CommitCreated;
use crate::state::{CommitAccount, RepositoryAccount};
use anchor_lang::prelude::*;
use forge_object::{history::append_history_root, HashAlgorithm, Oid};

/// Accounts for [`crate::forge_repository::create_commit`].
#[event_cpi]
#[derive(Accounts)]
#[instruction(
    commit_oid: [u8; 32],
    parent_count: u8,
    parent_a: [u8; 32],
    parent_b: [u8; 32],
    tree_oid: [u8; 32],
    authored_at: i64,
    message_hash: [u8; 32],
    attestation_hash: [u8; 32],
)]
pub struct CreateCommit<'info> {
    /// Commit author and rent payer; must be authorized for the repository (MVP: owner).
    #[account(mut)]
    pub author: Signer<'info>,

    /// Repository whose history is extended.
    #[account(mut)]
    pub repository: Account<'info, RepositoryAccount>,

    /// Per-commit metadata PDA: `["commit", repository, commit_oid]`.
    #[account(
        init,
        payer = author,
        space = 8 + CommitAccount::LEN,
        seeds = [COMMIT_SEED, repository.key().as_ref(), commit_oid.as_ref()],
        bump
    )]
    pub commit: Account<'info, CommitAccount>,

    /// First parent commit account when `parent_count >= 1`.
    /// CHECK: validated in the handler when required.
    pub parent_a: UncheckedAccount<'info>,

    /// Second parent commit account when `parent_count == 2`.
    /// CHECK: validated in the handler when required.
    pub parent_b: UncheckedAccount<'info>,

    /// Instructions sysvar (Ed25519 introspection, §9.4).
    /// CHECK: address constraint
    #[account(address = INSTRUCTIONS_SYSVAR_ID)]
    pub instructions_sysvar: UncheckedAccount<'info>,

    /// Ed25519 native verifier program id (§9.4).
    /// CHECK: address constraint
    #[account(address = ED25519_PROGRAM_ID)]
    pub ed25519_program: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

/// Handler for `create_commit`.
///
/// # Errors
/// - [`ForgeError::InvalidParentCount`] if `parent_count > 2`.
/// - [`ForgeError::Unauthorized`] / [`ForgeError::InsufficientRole`] if the
///   author is neither the owner nor a writer-role contributor (pass the
///   contributor's `PermissionAccount` as a remaining account).
/// - [`ForgeError::InvalidCommitOid`] if the commit/tree oid or attestation
///   hash is zero or not 32 bytes.
/// - [`ForgeError::InvalidParent`] / [`ForgeError::SelfParent`] /
///   [`ForgeError::RootOnNonemptyRepo`] for invalid parent linkage.
/// - [`ForgeError::UnknownCommit`] / [`ForgeError::InvalidPda`] if a parent
///   account is missing or not the expected commit PDA.
/// - [`ForgeError::BadSignature`] / [`ForgeError::InvalidEd25519Instruction`] if
///   the preceding Ed25519 instruction does not authorize `attestation_hash`.
/// - [`ForgeError::MathOverflow`] on commit-count overflow.
#[allow(clippy::too_many_arguments)]
pub fn handler(
    ctx: Context<CreateCommit>,
    commit_oid: [u8; 32],
    parent_count: u8,
    parent_a: [u8; 32],
    parent_b: [u8; 32],
    tree_oid: [u8; 32],
    authored_at: i64,
    message_hash: [u8; 32],
    attestation_hash: [u8; 32],
) -> Result<()> {
    require!(parent_count <= 2, ForgeError::InvalidParentCount);

    let author_key = ctx.accounts.author.key();
    let repo_key = ctx.accounts.repository.key();

    crate::refs::require_min_role(
        &repo_key,
        &ctx.accounts.repository.owner,
        &author_key,
        crate::state::permission::ROLE_WRITER,
        ctx.remaining_accounts.first(),
    )?;

    require!(commit_oid != [0u8; 32], ForgeError::InvalidCommitOid);
    require!(tree_oid != [0u8; 32], ForgeError::InvalidCommitOid);
    require!(attestation_hash != [0u8; 32], ForgeError::InvalidCommitOid);

    validate_parent_args(
        parent_count,
        &commit_oid,
        &parent_a,
        &parent_b,
        ctx.accounts.repository.commit_count,
    )?;

    if parent_count >= 1 {
        validate_parent_account(
            &ctx.accounts.parent_a.to_account_info(),
            &repo_key,
            &parent_a,
        )?;
    }
    if parent_count == 2 {
        validate_parent_account(
            &ctx.accounts.parent_b.to_account_info(),
            &repo_key,
            &parent_b,
        )?;
    }

    verify_ed25519_instruction_preceding(
        &ctx.accounts.instructions_sysvar.to_account_info(),
        &author_key,
        &attestation_hash,
    )?;

    let repository = &mut ctx.accounts.repository;
    let seq = repository.commit_count;

    let prev_root = Oid::new(HashAlgorithm::Sha256, repository.history_root.to_vec())
        .map_err(|_| error!(ForgeError::InvalidCommitOid))?;
    let commit_oid_obj = Oid::new(HashAlgorithm::Sha256, commit_oid.to_vec())
        .map_err(|_| error!(ForgeError::InvalidCommitOid))?;
    let new_history = append_history_root(HashAlgorithm::Sha256, &prev_root, &commit_oid_obj, seq)
        .map_err(|_| error!(ForgeError::InvalidCommitOid))?;

    {
        let commit = &mut ctx.accounts.commit;
        commit.repo = repo_key;
        commit.commit_oid = commit_oid;
        commit.parent_count = parent_count;
        commit.parent_a = parent_a;
        commit.parent_b = parent_b;
        commit.tree_oid = tree_oid;
        commit.author = author_key;
        commit.authored_at = authored_at;
        commit.message_hash = message_hash;
        commit.attestation_hash = attestation_hash;
        commit.seq = seq;
        commit.bump = ctx.bumps.commit;
    }

    repository.history_root = new_history.to_bytes32();
    repository.commit_count = repository
        .commit_count
        .checked_add(1)
        .ok_or(ForgeError::MathOverflow)?;

    emit_cpi!(CommitCreated {
        repository: repo_key,
        commit_oid,
        author: author_key,
        seq,
        history_root: repository.history_root,
        slot: Clock::get()?.slot,
    });

    Ok(())
}

fn validate_parent_args(
    parent_count: u8,
    commit_oid: &[u8; 32],
    parent_a: &[u8; 32],
    parent_b: &[u8; 32],
    commit_count: u64,
) -> Result<()> {
    match parent_count {
        0 => {
            require!(
                *parent_a == [0u8; 32] && *parent_b == [0u8; 32],
                ForgeError::InvalidParent
            );
            require!(commit_count == 0, ForgeError::RootOnNonemptyRepo);
        }
        1 => {
            require!(*parent_a != [0u8; 32], ForgeError::InvalidParent);
            require!(*parent_b == [0u8; 32], ForgeError::InvalidParent);
            require!(*commit_oid != *parent_a, ForgeError::SelfParent);
        }
        2 => {
            require!(*parent_a != [0u8; 32], ForgeError::InvalidParent);
            require!(*parent_b != [0u8; 32], ForgeError::InvalidParent);
            require!(*parent_a != *parent_b, ForgeError::InvalidParent);
            require!(*commit_oid != *parent_a, ForgeError::SelfParent);
            require!(*commit_oid != *parent_b, ForgeError::SelfParent);
        }
        _ => return err!(ForgeError::InvalidParentCount),
    }
    Ok(())
}

fn validate_parent_account(
    account: &AccountInfo,
    repo: &Pubkey,
    expected_oid: &[u8; 32],
) -> Result<()> {
    let (expected_pda, _) = crate::pda::commit_pda(repo, expected_oid);
    require_keys_eq!(*account.key, expected_pda, ForgeError::InvalidPda);

    let data = account.try_borrow_data()?;
    require!(
        data.len() >= 8 + CommitAccount::LEN,
        ForgeError::UnknownCommit
    );
    // `try_deserialize` expects the discriminator-prefixed buffer.
    let commit = CommitAccount::try_deserialize(&mut &data[..])?;
    require_keys_eq!(commit.repo, *repo, ForgeError::UnknownCommit);
    require!(
        commit.commit_oid == *expected_oid,
        ForgeError::UnknownCommit
    );
    Ok(())
}
