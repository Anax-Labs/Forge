//! Forge repository program.
//!
//! Onchain anchor for Git-like repository history, branch refs, authorship
//! attestations, and deployed-program source provenance.
//!
//! # Phase 3 status
//!
//! Implemented: the full account state model (§4), PDA derivations (§4), and
//! the `initialize_repository` / `create_branch` lifecycle instructions
//! (§9.2), with owner-only authorization (§16.4) and `emit_cpi!` events
//! (§9.1). Commit creation, branch advancement/merge, storage, provenance and
//! permissions land in Phases 4–9 (see `phase_implementation.md`).
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
pub mod errors;
pub mod events;
pub mod instructions;
pub mod name;
pub mod pda;
pub mod state;

use anchor_lang::prelude::*;

// Bring instruction account structs to the crate root, which is where Anchor's
// `#[program]` macro resolves them (it reads only the first path segment of the
// `Context<T>` type). The derive-generated `__client_accounts_*` modules are
// crate-visible; re-export them at the root as well so the macro can find
// them.
pub(crate) use instructions::create_branch::__client_accounts_create_branch;
pub(crate) use instructions::initialize_repository::__client_accounts_initialize_repository;
pub use instructions::{CreateBranch, InitializeRepository};

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
    /// # Errors
    /// Returns [`errors::ForgeError::InvalidName`],
    /// [`errors::ForgeError::Unauthorized`], or
    /// [`errors::ForgeError::UnknownCommit`] as documented on the handler.
    pub fn create_branch(
        ctx: Context<CreateBranch>,
        name: [u8; 32],
        from_commit: [u8; 32],
        authority: Pubkey,
    ) -> Result<()> {
        instructions::create_branch::handler(ctx, name, from_commit, authority)
    }
}
