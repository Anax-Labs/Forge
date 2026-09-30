//! `ProgramSourceAttestation` — the program → commit → source provenance link
//! (§4.7, §12.2).
//!
//! Defined in Phase 3; created by `anchor_program_source` in Phase 9. A claim
//! (`verified = 0`) records who asserted the link; an independent rebuild
//! promotes it to `verified = 1`. `forge verify-program` distinguishes the two.

use anchor_lang::prelude::*;

/// Onchain source-provenance attestation for a deployed program.
#[account]
pub struct ProgramSourceAttestation {
    /// Deployed program account this attestation is about.
    pub program_id: Pubkey,
    /// Repository that contains the source.
    pub repo: Pubkey,
    /// Exact commit the artifact was built from.
    pub commit_oid: [u8; 32],
    /// Executable hash of the deployed program (e.g. `solana-verify`).
    pub artifact_hash: [u8; 32],
    /// Hash of the build metadata / toolchain description (SLSA-shaped, §12.3).
    pub build_metadata_hash: [u8; 32],
    /// Wallet that asserted the link.
    pub attester: Pubkey,
    /// 0 = claim, 1 = independently verified (§9.2).
    pub verified: u8,
    /// Slot the attestation was created in.
    pub created_slot: u64,
    /// Canonical PDA bump.
    pub bump: u8,
}

impl ProgramSourceAttestation {
    /// Serialized length excluding the 8-byte Anchor discriminator.
    pub const LEN: usize = 32   // program_id
        + 32                    // repo
        + 32                    // commit_oid
        + 32                    // artifact_hash
        + 32                    // build_metadata_hash
        + 32                    // attester
        + 1                     // verified
        + 8                     // created_slot
        + 1; // bump
}
