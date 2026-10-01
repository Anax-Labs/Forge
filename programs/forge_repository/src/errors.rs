//! Program error codes.
//!
//! Each variant is a distinct failure point so tests can assert the exact
//! error path (§9.2 tasks 5, §11 cross-cutting checklist). Anchor assigns
//! custom codes starting at 6000 in declaration order; add new variants at the
//! end to keep existing codes stable.

use anchor_lang::prelude::*;

/// Errors returned by the Forge repository program.
#[error_code]
pub enum ForgeError {
    /// Name was empty, non-UTF-8, contained control characters, or had
    /// non-zero padding after the NUL terminator.
    #[msg("name is empty or contains invalid characters")]
    InvalidName,

    /// Signer is not authorized for this repository or branch.
    #[msg("signer is not authorized for this repository")]
    Unauthorized,

    /// The referenced commit does not exist in this repository.
    #[msg("referenced commit does not exist in this repository")]
    UnknownCommit,

    /// A branch with this name already exists for the repository.
    #[msg("a branch with this name already exists")]
    BranchAlreadyExists,

    /// A supplied account is not the canonical PDA for the expected seeds.
    #[msg("account is not the canonical PDA for the given seeds")]
    InvalidPda,

    /// The repository name is already taken by this owner.
    #[msg("repository name is already taken by this owner")]
    RepositoryAlreadyExists,

    /// The storage backend tag is not recognized.
    #[msg("unsupported storage backend")]
    InvalidStorageBackend,

    /// An arithmetic operation overflowed (checked math, §11).
    #[msg("arithmetic overflow")]
    MathOverflow,

    /// `parent_count` must be 0, 1, or 2 (§6.4).
    #[msg("parent_count must be 0, 1, or 2")]
    InvalidParentCount,

    /// Commit or tree oid, or attestation hash, was all-zero or malformed.
    #[msg("commit oid, tree oid, or attestation hash is invalid")]
    InvalidCommitOid,

    /// A parent oid equals the new commit oid (§6.4).
    #[msg("commit cannot be its own parent")]
    SelfParent,

    /// Root commit (`parent_count == 0`) on a repository that already has commits.
    #[msg("only the first commit may have no parent")]
    RootOnNonemptyRepo,

    /// Ed25519 pubkey or message did not match the expected attestation (§9.4).
    #[msg("Ed25519 signature verification failed")]
    BadSignature,

    /// Preceding instruction was not a valid Ed25519 verify ix (§9.4).
    #[msg("invalid or missing Ed25519 program instruction")]
    InvalidEd25519Instruction,

    /// Parent oid arguments or accounts are inconsistent (§6.4).
    #[msg("invalid parent commit reference")]
    InvalidParent,

    /// `expected_head_seq` does not match the branch's current `head_seq` (§7.4).
    #[msg("stale branch head; refetch and retry")]
    StaleBranchHead,

    /// A branch update is not a fast-forward and no reset was requested (§7.3).
    #[msg("non-fast-forward branch update")]
    NonFastForward,

    /// Merge commit parents do not match the target and source branch heads (§6.6).
    #[msg("invalid merge: parents do not match branch heads")]
    InvalidMerge,

    /// The default branch cannot be deleted (§7.2).
    #[msg("the default branch cannot be deleted")]
    CannotDeleteDefaultBranch,
}
