//! `PermissionAccount` — per-contributor role (§4.6).
//!
//! Defined in Phase 3; created and enforced by `update_permissions` and the
//! commit/branch instructions in Phase 9. Kept as a separate fixed-size account
//! so `RepositoryAccount` never needs reallocating and per-contributor writes
//! parallelize.

use anchor_lang::prelude::*;

/// Role constants for [`PermissionAccount::role`].
pub const ROLE_READER: u8 = 0;
/// Writer: may create commits and push to unprotected branches.
pub const ROLE_WRITER: u8 = 1;
/// Maintainer: may update protected branches, merge, and reset.
pub const ROLE_MAINTAINER: u8 = 2;
/// Admin: may manage permissions and transfer the repository.
pub const ROLE_ADMIN: u8 = 3;

/// Onchain contributor permission entry.
#[account]
pub struct PermissionAccount {
    /// Repository this entry applies to.
    pub repo: Pubkey,
    /// Contributor wallet.
    pub contributor: Pubkey,
    /// Role (see `ROLE_*` constants).
    pub role: u8,
    /// Slot the role was granted.
    pub granted_slot: u64,
    /// Slot the role expires at (0 = never).
    pub expires_slot: u64,
    /// Canonical PDA bump.
    pub bump: u8,
}

impl PermissionAccount {
    /// Serialized length excluding the 8-byte Anchor discriminator.
    pub const LEN: usize = 32   // repo
        + 32                    // contributor
        + 1                     // role
        + 8                     // granted_slot
        + 8                     // expires_slot
        + 1; // bump
}
