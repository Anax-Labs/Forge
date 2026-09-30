//! Program events emitted for indexers.
//!
//! Events are emitted through `emit_cpi!` (a self-CPI into the event-authority
//! PDA) rather than raw program logs, so RPCs are less likely to truncate them
//! and they are never parsed from string logs (§9.1, §11 #12). The required
//! `event_authority` and `program` accounts are appended automatically by the
//! `#[event_cpi]` attribute on the instruction account structs.
//!
//! `RepositoryInitialized` / `BranchCreated` are informational; the
//! authoritative state lives in the account data.

use anchor_lang::prelude::*;

/// Emitted when a repository and its default branch are created.
#[event]
pub struct RepositoryInitialized {
    /// Repository PDA.
    pub repository: Pubkey,
    /// Owning wallet (or program-owned PDA, §15).
    pub owner: Pubkey,
    /// Owner-scoped repository name (NUL-padded `[u8; 32]`).
    pub name: [u8; 32],
    /// Default branch name (NUL-padded `[u8; 32]`).
    pub default_branch: [u8; 32],
    /// Genesis history root (§5.6).
    pub history_root: [u8; 32],
    /// Slot the repository was created in.
    pub slot: u64,
}

/// Emitted when a branch is created.
#[event]
pub struct BranchCreated {
    /// Repository PDA.
    pub repository: Pubkey,
    /// Branch name (NUL-padded `[u8; 32]`).
    pub name: [u8; 32],
    /// Commit the branch starts at (all-zero for an empty branch).
    pub from_commit: [u8; 32],
    /// Initial head sequence (always 0 on creation).
    pub head_seq: u64,
    /// Branch update authority (wallet or program-owned PDA).
    pub authority: Pubkey,
    /// Slot the branch was created in.
    pub slot: u64,
}

/// Emitted when a commit is anchored and the history root advances (§9.1).
#[event]
pub struct CommitCreated {
    /// Repository PDA.
    pub repository: Pubkey,
    /// Git commit object id (canonical 32-byte form).
    pub commit_oid: [u8; 32],
    /// Wallet that signed the attestation.
    pub author: Pubkey,
    /// Monotonic sequence assigned at creation (`repo.commit_count` before increment).
    pub seq: u64,
    /// Updated append-only history root (§5.6).
    pub history_root: [u8; 32],
    /// Slot the commit was anchored in.
    pub slot: u64,
}
