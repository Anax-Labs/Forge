//! `anchor_program_source` — anchor a program→commit source-provenance claim
//! (§9.2, §12.2).
//!
//! This is the protocol's differentiator: a permissionless, onchain mapping
//! `program_id → repo → commit` that answers *"what source produced this
//! deployed program?"* without a trusted API. The record is a **claim**
//! (`verified = 0`); an independent rebuild promotes it later (ADR 0009). The
//! PDA is keyed by `program_id`, so one program has at most one claim.

use anchor_lang::prelude::*;

use crate::constants::{BPF_LOADER_UPGRADEABLE_ID, PROG_SEED};
use crate::errors::ForgeError;
use crate::events::ProgramSourceAnchored;
use crate::refs::{load_commit, require_min_role};
use crate::state::permission::ROLE_WRITER;
use crate::state::{ProgramSourceAttestation, RepositoryAccount};

/// Accounts for [`crate::forge_repository::anchor_program_source`].
#[event_cpi]
#[derive(Accounts)]
#[instruction(
    program_id: Pubkey,
    commit_oid: [u8; 32],
    artifact_hash: [u8; 32],
    build_metadata_hash: [u8; 32],
)]
pub struct AnchorProgramSource<'info> {
    /// Claimant and rent payer (owner, or a writer-role contributor).
    #[account(mut)]
    pub attester: Signer<'info>,

    /// Repository that contains the source.
    pub repository: Account<'info, RepositoryAccount>,

    /// The provenance PDA: `["prog", program_id]`.
    #[account(
        init,
        payer = attester,
        space = 8 + ProgramSourceAttestation::LEN,
        seeds = [PROG_SEED, program_id.as_ref()],
        bump
    )]
    pub attestation: Account<'info, ProgramSourceAttestation>,

    /// Commit the artifact was built from.
    /// CHECK: validated in the handler (canonical PDA, repository, oid).
    pub commit: UncheckedAccount<'info>,

    /// The deployed program account being attested.
    /// CHECK: validated in the handler (key, upgradeable-loader owner, executable).
    pub program_account: UncheckedAccount<'info>,

    /// System program (attestation account creation).
    pub system_program: Program<'info, System>,
}

/// Handler for `anchor_program_source`.
///
/// # Errors
/// - [`ForgeError::Unauthorized`] / [`ForgeError::InsufficientRole`] if the
///   attester lacks writer permission.
/// - [`ForgeError::InvalidArtifactHash`] if `artifact_hash` is all-zero.
/// - [`ForgeError::UnknownCommit`] / [`ForgeError::InvalidPda`] if the commit is
///   missing or wrong.
/// - [`ForgeError::ProgramNotUpgradeable`] if `program_id` is not a deployed
///   upgradeable program.
/// - Anchor's `AccountAlreadyInUse` if a claim already exists for `program_id`.
pub fn handler(
    ctx: Context<AnchorProgramSource>,
    program_id: Pubkey,
    commit_oid: [u8; 32],
    artifact_hash: [u8; 32],
    build_metadata_hash: [u8; 32],
) -> Result<()> {
    let attester_key = ctx.accounts.attester.key();
    let repo_key = ctx.accounts.repository.key();
    require_min_role(
        &repo_key,
        &ctx.accounts.repository.owner,
        &attester_key,
        ROLE_WRITER,
        ctx.remaining_accounts.first(),
    )?;

    require!(artifact_hash != [0u8; 32], ForgeError::InvalidArtifactHash);

    // The commit must already be part of this repository's history.
    let commit_info = ctx.accounts.commit.to_account_info();
    load_commit(&repo_key, &commit_oid, &commit_info)?;

    // The referenced program must be a deployed upgradeable program.
    require_keys_eq!(
        *ctx.accounts.program_account.key,
        program_id,
        ForgeError::InvalidPda
    );
    require_keys_eq!(
        *ctx.accounts.program_account.owner,
        BPF_LOADER_UPGRADEABLE_ID,
        ForgeError::ProgramNotUpgradeable
    );
    require!(
        ctx.accounts.program_account.executable,
        ForgeError::ProgramNotUpgradeable
    );

    let slot = Clock::get()?.slot;
    {
        let attestation = &mut ctx.accounts.attestation;
        attestation.program_id = program_id;
        attestation.repo = repo_key;
        attestation.commit_oid = commit_oid;
        attestation.artifact_hash = artifact_hash;
        attestation.build_metadata_hash = build_metadata_hash;
        attestation.attester = attester_key;
        attestation.verified = 0;
        attestation.created_slot = slot;
        attestation.bump = ctx.bumps.attestation;
    }

    emit_cpi!(ProgramSourceAnchored {
        program_id,
        repository: repo_key,
        commit_oid,
        artifact_hash,
        attester: attester_key,
        slot,
    });

    Ok(())
}
