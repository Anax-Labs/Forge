//! Program constants (PDA seed prefixes, limits, enum tags).
//!
//! Seed prefixes are `&[u8]` slices so they can be used directly in Anchor
//! `seeds = [...]` expressions alongside `key().as_ref()` (see
//! `anchor_lang`'s `seeds_compile` test for the supported forms).
//!
//! Spec traceability: §4 (PDA seeds), §5.6 (history domain prefixes live in
//! `forge-object`), §7.5 (permission modes), §8 (storage backends).

use anchor_lang::prelude::*;

/// Repository PDA seed prefix: `["repo", owner, name]`.
pub const REPO_SEED: &[u8] = b"repo";
/// Branch PDA seed prefix: `["branch", repository, name]`.
pub const BRANCH_SEED: &[u8] = b"branch";
/// Commit PDA seed prefix: `["commit", repository, commit_oid]`.
pub const COMMIT_SEED: &[u8] = b"commit";
/// Tag PDA seed prefix: `["tag", repository, name]`.
pub const TAG_SEED: &[u8] = b"tag";
/// Permission PDA seed prefix: `["perm", repository, contributor]`.
pub const PERM_SEED: &[u8] = b"perm";
/// Program-source attestation PDA seed prefix: `["prog", program_id]`.
pub const PROG_SEED: &[u8] = b"prog";

/// Native Ed25519 signature verification program (§9.4).
pub const ED25519_PROGRAM_ID: Pubkey = pubkey!("Ed25519SigVerify111111111111111111111111111");

/// Instructions sysvar (transaction introspection, §9.4).
pub const INSTRUCTIONS_SYSVAR_ID: Pubkey = pubkey!("Sysvar1nstructions1111111111111111111111111");

/// Repository / branch / tag name buffer length.
///
/// Names are padded with NUL bytes to this fixed length. This is capped at 32
/// to satisfy the PDA seed length limit (`MAX_SEED_LEN = 32`, §2.3); the
/// `[u8; 64]` name buffers sketched in §4.4/§4.5 are therefore frozen at
/// `[u8; 32]`. See `docs/adr/0002-name-length-and-pda-seeds.md`.
pub const NAME_LEN: usize = 32;

/// Storage backend tag: IPFS (hot, content-addressed, best-effort). §8.3.
pub const STORAGE_BACKEND_IPFS: u8 = 0;
/// Storage backend tag: Arweave (permanent, tags/checkpoints). §8.3.
pub const STORAGE_BACKEND_ARWEAVE: u8 = 1;
/// Storage backend tag: hybrid IPFS + Arweave (recommended, §8.3/§20).
pub const STORAGE_BACKEND_HYBRID: u8 = 2;

/// Highest valid storage backend tag (bounds check helper).
pub const STORAGE_BACKEND_MAX: u8 = STORAGE_BACKEND_HYBRID;

/// Branch permission mode: only the repository owner may write. MVP default
/// until Phase 9 introduces allowlists and authority PDAs (§16.4).
pub const PERMISSIONS_MODE_OWNER_ONLY: u8 = 0;
/// Branch permission mode: contributor allowlist (Phase 9, §7.5).
pub const PERMISSIONS_MODE_ALLOWLIST: u8 = 1;
/// Branch permission mode: `authority` is a program-owned PDA (Phase 9, §16.2).
pub const PERMISSIONS_MODE_AUTHORITY_PDA: u8 = 2;
