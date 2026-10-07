//! Forge repository program.
//!
//! Onchain anchor for Git-like repository history, branch refs, authorship
//! attestations, and deployed-program source provenance.
//!
//! # Status
//!
//! Implemented: account state model (§4), PDAs, `initialize_repository`,
//! `create_branch`, `create_commit` (Ed25519 attestation), `update_branch`,
//! `reset_branch`, `delete_branch`, `merge`, `create_tag`,
//! `update_permissions`, `transfer_repository`, and `anchor_program_source`
//! (§9.2). See `phase_implementation.md` and `docs/adr/`.
//!
//! # One source of truth
//!
//! All canonical hashing (object ids, attestation, history root) lives in
//! `forge-object`; this program never re-implements serialization (§5, §5.6).

// Pedantic lints that conflict with Anchor's generated code or the protocol
// style (mirrors the crate-level allows in `forge-object`):
// - `needless_pass_by_value`: Anchor handlers must take `Context<T>` by value.
// - `wildcard_imports`: `use super::*` is required inside `#[program]`.
// - `must_use_candidate`: low-level constructors are often called for effect.
// - `pub_underscore_fields`: `_reserved` is the Anchor upgrade-headroom idiom.
#![allow(
    clippy::needless_pass_by_value,
    clippy::wildcard_imports,
    clippy::must_use_candidate,
    clippy::pub_underscore_fields
)]

pub mod auth;
pub mod constants;
pub mod ed25519;
pub mod errors;
pub mod events;
pub mod instructions;
pub mod name;
pub mod pda;
pub mod refs;
pub mod state;

mod init;

use anchor_lang::prelude::*;

// Bring instruction account structs to the crate root, which is where Anchor's
// `#[program]` macro resolves them (it reads only the first path segment of the
// `Context<T>` type). The derive-generated `__client_accounts_*` modules are
// crate-visible; re-export them at the root as well so the macro can find
// them.
pub(crate) use instructions::anchor_program_source::__client_accounts_anchor_program_source;
pub(crate) use instructions::create_branch::__client_accounts_create_branch;
pub(crate) use instructions::create_commit::__client_accounts_create_commit;
pub(crate) use instructions::create_tag::__client_accounts_create_tag;
pub(crate) use instructions::delete_branch::__client_accounts_delete_branch;
pub(crate) use instructions::initialize_repository::__client_accounts_initialize_repository;
pub(crate) use instructions::merge::__client_accounts_merge;
pub(crate) use instructions::reset_branch::__client_accounts_reset_branch;
pub(crate) use instructions::transfer_repository::__client_accounts_transfer_repository;
pub(crate) use instructions::update_branch::__client_accounts_update_branch;
pub(crate) use instructions::update_permissions::__client_accounts_update_permissions;
pub use instructions::{
    AnchorProgramSource, CreateBranch, CreateCommit, CreateTag, DeleteBranch, InitializeRepository,
    Merge, ResetBranch, TransferRepository, UpdateBranch, UpdatePermissions,
};

declare_id!("4smCAEoycSXSvVsyic8ircQmHmENPCHn17Fma83SYVbf");

#[program]
pub mod forge_repository {
    use super::*;

    /// Creates a repository and its empty default branch (§9.2).
    ///
    /// Accounts are supplied in the order declared by
    /// [`instructions::InitializeRepository`].
    ///
    /// # Errors
    /// Returns [`errors::ForgeError::InvalidName`] or
    /// [`errors::ForgeError::InvalidStorageBackend`] on malformed input.
    pub fn initialize_repository(
        ctx: Context<InitializeRepository>,
        name: [u8; 32],
        default_branch: [u8; 32],
        storage_backend: u8,
        flags: u16,
    ) -> Result<()> {
        instructions::initialize_repository::handler(
            ctx,
            name,
            default_branch,
            storage_backend,
            flags,
        )
    }

    /// Creates a branch ref from an existing commit or from empty (§9.2, §7.2).
    ///
    /// The signing `authority` account becomes the branch's recorded update
    /// authority.
    ///
    /// # Errors
    /// Returns [`errors::ForgeError::InvalidName`],
    /// [`errors::ForgeError::Unauthorized`],
    /// [`errors::ForgeError::BranchAlreadyExists`], or
    /// [`errors::ForgeError::UnknownCommit`] as documented on the handler.
    pub fn create_branch(
        ctx: Context<CreateBranch>,
        name: [u8; 32],
        from_commit: [u8; 32],
    ) -> Result<()> {
        instructions::create_branch::handler(ctx, name, from_commit)
    }

    /// Anchors a wallet-signed commit and advances the repository history root (§9.2).
    ///
    /// The transaction must prepend an Ed25519 native verify instruction whose
    /// message is the 32-byte `attestation_hash` (§9.4).
    ///
    /// # Errors
    /// See [`instructions::create_commit::handler`] for validation failures.
    #[allow(clippy::too_many_arguments)]
    pub fn create_commit(
        ctx: Context<CreateCommit>,
        commit_oid: [u8; 32],
        parent_count: u8,
        parent_a: [u8; 32],
        parent_b: [u8; 32],
        tree_oid: [u8; 32],
        authored_at: i64,
        message_hash: [u8; 32],
        attestation_hash: [u8; 32],
    ) -> Result<()> {
        instructions::create_commit::handler(
            ctx,
            commit_oid,
            parent_count,
            parent_a,
            parent_b,
            tree_oid,
            authored_at,
            message_hash,
            attestation_hash,
        )
    }

    /// Fast-forwards or merges a branch head (§9.2, §7.2, §7.4).
    ///
    /// The transaction must prepend an Ed25519 verify over the branch-update
    /// message (`forge_object::branch::branch_update_message`).
    ///
    /// # Errors
    /// See [`instructions::update_branch::handler`] for validation failures.
    pub fn update_branch(
        ctx: Context<UpdateBranch>,
        new_head: [u8; 32],
        expected_head_seq: u64,
    ) -> Result<()> {
        instructions::update_branch::handler(ctx, new_head, expected_head_seq)
    }

    /// Explicitly resets a branch to a non-descendant commit, emitting
    /// `BranchReset` (§7.3). The repository history root is not affected.
    ///
    /// # Errors
    /// See [`instructions::reset_branch::handler`] for validation failures.
    pub fn reset_branch(
        ctx: Context<ResetBranch>,
        new_head: [u8; 32],
        expected_head_seq: u64,
    ) -> Result<()> {
        instructions::reset_branch::handler(ctx, new_head, expected_head_seq)
    }

    /// Deletes a non-default branch and refunds its rent (§7.2).
    ///
    /// # Errors
    /// See [`instructions::delete_branch::handler`] for validation failures.
    pub fn delete_branch(ctx: Context<DeleteBranch>) -> Result<()> {
        instructions::delete_branch::handler(ctx)
    }

    /// Advances a target branch to a two-parent merge commit (§6.6).
    ///
    /// # Errors
    /// See [`instructions::merge::handler`] for validation failures.
    pub fn merge(
        ctx: Context<Merge>,
        merge_commit_oid: [u8; 32],
        expected_target_seq: u64,
    ) -> Result<()> {
        instructions::merge::handler(ctx, merge_commit_oid, expected_target_seq)
    }

    /// Creates an immutable, tagger-signed release tag (§9.2).
    ///
    /// The transaction must prepend an Ed25519 verify over
    /// `forge_object::tag::tag_message`.
    ///
    /// # Errors
    /// See [`instructions::create_tag::handler`] for validation failures.
    pub fn create_tag(
        ctx: Context<CreateTag>,
        name: [u8; 32],
        target_commit: [u8; 32],
        message_hash: [u8; 32],
    ) -> Result<()> {
        instructions::create_tag::handler(ctx, name, target_commit, message_hash)
    }

    /// Grants or updates a contributor's role (§9.2, §4.6).
    ///
    /// # Errors
    /// See [`instructions::update_permissions::handler`] for validation failures.
    pub fn update_permissions(
        ctx: Context<UpdatePermissions>,
        contributor: Pubkey,
        role: u8,
        expires_slot: u64,
    ) -> Result<()> {
        instructions::update_permissions::handler(ctx, contributor, role, expires_slot)
    }

    /// Transfers repository ownership (§9.2, §15).
    ///
    /// # Errors
    /// See [`instructions::transfer_repository::handler`] for validation failures.
    pub fn transfer_repository(ctx: Context<TransferRepository>, new_owner: Pubkey) -> Result<()> {
        instructions::transfer_repository::handler(ctx, new_owner)
    }

    /// Anchors a program→commit source-provenance claim (§9.2, §12.2).
    ///
    /// # Errors
    /// See [`instructions::anchor_program_source::handler`] for validation
    /// failures.
    pub fn anchor_program_source(
        ctx: Context<AnchorProgramSource>,
        program_id: Pubkey,
        commit_oid: [u8; 32],
        artifact_hash: [u8; 32],
        build_metadata_hash: [u8; 32],
    ) -> Result<()> {
        instructions::anchor_program_source::handler(
            ctx,
            program_id,
            commit_oid,
            artifact_hash,
            build_metadata_hash,
        )
    }
}
