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
}
