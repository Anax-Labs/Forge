//! Program account state (one module per account type).
//!
//! All account layouts are frozen in Phase 3 (§4). Each struct exposes a `LEN`
//! const equal to the Borsh-serialized size **excluding** Anchor's 8-byte
//! discriminator; instruction `space` is therefore `8 + T::LEN`.

pub mod attestation;
pub mod branch;
pub mod commit;
pub mod permission;
pub mod repository;
pub mod tag;

pub use attestation::ProgramSourceAttestation;
pub use branch::BranchAccount;
pub use commit::CommitAccount;
pub use permission::PermissionAccount;
pub use repository::RepositoryAccount;
pub use tag::TagAccount;
