//! `BranchAccount` — a mutable branch ref (§4.4).
//!
//! Branches are the only mutable refs in the protocol. Every update increments
//! `head_seq` (the optimistic-concurrency token, §7.4) and emits an event. The
//! history root is never stored here, so branch movement cannot erase history.

use anchor_lang::prelude::*;

/// Onchain branch state.
#[account]
pub struct BranchAccount {
    /// Repository this branch belongs to.
    pub repo: Pubkey,
    /// Branch name, NUL-padded (`[u8; 32]`).
    pub name: [u8; 32],
    /// Current tip commit oid (all-zero for an empty branch).
    pub head_commit: [u8; 32],
    /// Monotonic update counter used as a compare-and-swap token (§7.4).
    pub head_seq: u64,
    /// Update authority: a wallet or a program-owned PDA (§7.5, §16.2).
    pub authority: Pubkey,
    /// Permission mode (§7.5): see `crate::constants`.
    pub permissions_mode: u8,
    /// Protection bitfield: bit0 require signed commits, bit1 require CI.
    pub protected: u8,
    /// Canonical PDA bump.
    pub bump: u8,
    /// Slot of the last update.
    pub updated_slot: u64,
    /// Reserved headroom.
    pub _reserved: [u8; 32],
}

impl BranchAccount {
    /// Serialized length excluding the 8-byte Anchor discriminator.
    pub const LEN: usize = 32   // repo
        + 32                    // name
        + 32                    // head_commit
        + 8                     // head_seq
        + 32                    // authority
        + 1                     // permissions_mode
        + 1                     // protected
        + 1                     // bump
        + 8                     // updated_slot
        + 32; // _reserved
}
