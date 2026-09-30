//! `RepositoryAccount` — repository identity, ownership, config, and the
//! append-only history root (§4.1).
//!
//! Field order is frozen (Phase 3); changing it requires a migration. The
//! layout keeps fixed-size fields first and reserves trailing headroom so new
//! fields can be appended without breaking existing accounts.

use anchor_lang::prelude::*;

/// Onchain repository state.
#[account]
pub struct RepositoryAccount {
    /// Owning wallet, or a program-owned PDA (e.g. a Squads vault, §15).
    pub owner: Pubkey,
    /// The repository PDA itself, stored for indexer convenience.
    pub repo_id: Pubkey,
    /// Owner-scoped name, NUL-padded (`[u8; 32]`).
    pub name: [u8; 32],
    /// Default branch name, NUL-padded (`[u8; 32]`).
    pub default_branch: [u8; 32],
    /// Append-only commitment over every accepted commit (§5.6).
    pub history_root: [u8; 32],
    /// Monotonic count of commits anchored for this repository.
    pub commit_count: u64,
    /// Number of contributors with a permission entry (maintained in Phase 9).
    pub contributor_count: u32,
    /// Storage backend tag (§8.3): see `crate::constants`.
    pub storage_backend: u8,
    /// Bitfield of repository flags: bit0 require signed commits, bit1 private.
    pub flags: u16,
    /// Canonical PDA bump (stored, never re-searched, §11 bump canonicalization).
    pub bump: u8,
    /// Slot the repository was created in.
    pub created_slot: u64,
    /// Reserved headroom for forward-compatible upgrades.
    pub _reserved: [u8; 64],
}

impl RepositoryAccount {
    /// Serialized length excluding the 8-byte Anchor discriminator.
    pub const LEN: usize = 32   // owner
        + 32                    // repo_id
        + 32                    // name
        + 32                    // default_branch
        + 32                    // history_root
        + 8                     // commit_count
        + 4                     // contributor_count
        + 1                     // storage_backend
        + 2                     // flags
        + 1                     // bump
        + 8                     // created_slot
        + 64; // _reserved
}
