//! `create_tag` — create an immutable, tagger-signed release tag (§9.2).
//!
//! The tagger authorizes the tag with an Ed25519 signature over
//! [`forge_object::tag::tag_message`]; the program verifies it via Instructions
//! sysvar introspection (ADR 0004) and stores `signed = 1`. The `TagAccount` is
//! `init`-only, so a tag name can never be overwritten (§4.5).

use anchor_lang::prelude::*;
use forge_object::{tag::tag_message, HashAlgorithm, Oid};

use crate::constants::{ED25519_PROGRAM_ID, INSTRUCTIONS_SYSVAR_ID, TAG_SEED};
use crate::ed25519::verify_ed25519_instruction_preceding;
use crate::errors::ForgeError;
use crate::events::TagCreated;
use crate::name::validate_name;
use crate::refs::{load_commit, require_min_role};
use crate::state::permission::ROLE_WRITER;
use crate::state::{RepositoryAccount, TagAccount};

/// Accounts for [`crate::forge_repository::create_tag`].
#[event_cpi]
#[derive(Accounts)]
#[instruction(name: [u8; 32], target_commit: [u8; 32], message_hash: [u8; 32])]
pub struct CreateTag<'info> {
    /// Tag author and rent payer (owner, or a writer-role contributor).
    #[account(mut)]
    pub tagger: Signer<'info>,

    /// Repository the tag belongs to.
    pub repository: Account<'info, RepositoryAccount>,

    /// The tag PDA: `["tag", repository, name]`.
    #[account(
        init,
        payer = tagger,
        space = 8 + TagAccount::LEN,
        seeds = [TAG_SEED, repository.key().as_ref(), name.as_ref()],
        bump
    )]
    pub tag: Account<'info, TagAccount>,

    /// Commit the tag points at.
    /// CHECK: validated in the handler (canonical PDA, repository, oid).
    pub target_commit: UncheckedAccount<'info>,

    /// Instructions sysvar (Ed25519 introspection, §9.4).
    /// CHECK: address constraint
    #[account(address = INSTRUCTIONS_SYSVAR_ID)]
    pub instructions_sysvar: UncheckedAccount<'info>,

    /// Ed25519 native verifier program id (§9.4).
    /// CHECK: address constraint
    #[account(address = ED25519_PROGRAM_ID)]
    pub ed25519_program: UncheckedAccount<'info>,

    /// System program (tag account creation).
    pub system_program: Program<'info, System>,
}

/// Handler for `create_tag`.
///
/// # Errors
/// - [`ForgeError::InvalidName`] if the tag name is malformed.
/// - [`ForgeError::Unauthorized`] / [`ForgeError::InsufficientRole`] if the
///   tagger lacks writer permission.
/// - [`ForgeError::UnknownCommit`] / [`ForgeError::InvalidPda`] if the target
///   commit is missing or wrong.
/// - [`ForgeError::InvalidEd25519Instruction`] / [`ForgeError::BadSignature`]
///   if the tagger's signature is missing or wrong.
/// - Anchor's `AccountAlreadyInUse` if the tag name already exists.
pub fn handler(
    ctx: Context<CreateTag>,
    name: [u8; 32],
    target_commit: [u8; 32],
    message_hash: [u8; 32],
) -> Result<()> {
    validate_name(&name)?;

    let tagger_key = ctx.accounts.tagger.key();
    let repo_key = ctx.accounts.repository.key();
    require_min_role(
        &repo_key,
        &ctx.accounts.repository.owner,
        &tagger_key,
        ROLE_WRITER,
        ctx.remaining_accounts.first(),
    )?;

    require!(target_commit != [0u8; 32], ForgeError::InvalidCommitOid);
    let target_info = ctx.accounts.target_commit.to_account_info();
    load_commit(&repo_key, &target_commit, &target_info)?;

    let target_oid = Oid::new(HashAlgorithm::Sha256, target_commit.to_vec())
        .map_err(|_| error!(ForgeError::InvalidCommitOid))?;
    let message_oid = Oid::new(HashAlgorithm::Sha256, message_hash.to_vec())
        .map_err(|_| error!(ForgeError::InvalidCommitOid))?;
    let message = tag_message(&repo_key.to_bytes(), &name, &target_oid, &message_oid);
    verify_ed25519_instruction_preceding(
        &ctx.accounts.instructions_sysvar.to_account_info(),
        &tagger_key,
        &message,
    )?;

    let slot = Clock::get()?.slot;
    {
        let tag = &mut ctx.accounts.tag;
        tag.repo = repo_key;
        tag.name = name;
        tag.target_commit = target_commit;
        tag.tagger = tagger_key;
        tag.message_hash = message_hash;
        tag.created_slot = slot;
        tag.signed = 1;
        tag.bump = ctx.bumps.tag;
    }

    emit_cpi!(TagCreated {
        repository: repo_key,
        name,
        target_commit,
        tagger: tagger_key,
        message_hash,
        signed: 1,
        slot,
    });

    Ok(())
}
