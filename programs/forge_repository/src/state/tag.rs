//! `TagAccount` — an immutable release reference (§4.5).
//!
//! Defined in Phase 3; created by `create_tag` in Phase 9. Tags are immutable:
//! the account is `init`-only, so a name can never be overwritten.

use anchor_lang::prelude::*;

/// Onchain annotated tag.
#[account]
pub struct TagAccount {
    /// Repository this tag belongs to.
    pub repo: Pubkey,
    /// Tag name, NUL-padded (`[u8; 32]`).
    pub name: [u8; 32],
    /// Commit the tag points at.
    pub target_commit: [u8; 32],
    /// Wallet that created the tag.
    pub tagger: Pubkey,
    /// Hash of the tag message (message itself is offchain).
    pub message_hash: [u8; 32],
    /// Slot the tag was created in.
    pub created_slot: u64,
    /// 1 if the tagger's signature is anchored, else 0.
    pub signed: u8,
    /// Canonical PDA bump.
    pub bump: u8,
}

impl TagAccount {
    /// Serialized length excluding the 8-byte Anchor discriminator.
    pub const LEN: usize = 32   // repo
        + 32                    // name
        + 32                    // target_commit
        + 32                    // tagger
        + 32                    // message_hash
        + 8                     // created_slot
        + 1                     // signed
        + 1; // bump
}
