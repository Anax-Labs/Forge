//! `CommitAccount` — per-commit authorship record (§4.2).
//!
//! Defined in Phase 3 (layout is frozen here); created by `create_commit` in
//! Phase 4. The authoritative field is `commit_oid`: the Git commit id
//! transitively commits to the tree, blobs, parents, and message. The other
//! fields are denormalized for cheap onchain queries and are re-validated
//! against the offchain commit object by clients.

use anchor_lang::prelude::*;

/// Onchain commit metadata.
#[account]
pub struct CommitAccount {
    /// Repository this commit belongs to.
    pub repo: Pubkey,
    /// Git commit object id (authoritative).
    pub commit_oid: [u8; 32],
    /// Number of parents: 0 = root, 1 = normal, 2 = merge.
    pub parent_count: u8,
    /// First parent oid (all-zero if none).
    pub parent_a: [u8; 32],
    /// Second (merge) parent oid (all-zero if none).
    pub parent_b: [u8; 32],
    /// Top-level tree oid.
    pub tree_oid: [u8; 32],
    /// Wallet that signed the commit attestation.
    pub author: Pubkey,
    /// Commit timestamp (seconds, as authored).
    pub authored_at: i64,
    /// Hash of the commit message (message itself is offchain, §4.2).
    pub message_hash: [u8; 32],
    /// Hash of the offchain signed attestation (§5.5).
    pub attestation_hash: [u8; 32],
    /// Position in the repository history log.
    pub seq: u64,
    /// Canonical PDA bump.
    pub bump: u8,
}

impl CommitAccount {
    /// Serialized length excluding the 8-byte Anchor discriminator.
    pub const LEN: usize = 32   // repo
        + 32                    // commit_oid
        + 1                     // parent_count
        + 32                    // parent_a
        + 32                    // parent_b
        + 32                    // tree_oid
        + 32                    // author
        + 8                     // authored_at
        + 32                    // message_hash
        + 32                    // attestation_hash
        + 8                     // seq
        + 1; // bump
}
