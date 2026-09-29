//! Canonical object, hashing and Merkle engine for Forge.
//!
//! This crate is the **single source of truth** for all protocol byte formats.
//! The Anchor program and the CLI must never re-implement this logic; both
//! depend on the canonical encodings defined here so that two independent
//! implementations produce byte-identical ids (§5).
//!
//! # What is frozen here
//!
//! - **Object framing**: `<type> <len>\0<payload>`, `H` over the framed bytes
//!   ([`object::oid`]).
//! - **Hash algorithm**: SHA-256 for Forge-native repositories; SHA-1 accepted
//!   for reading existing Git repositories ([`hash::HashAlgorithm`]).
//! - **Algorithm tagging**: `sha1:` / `sha256:` prefixes on qualified strings.
//! - **32-byte canonical oid form** for onchain fields: SHA-256 verbatim;
//!   SHA-1 as `0x01 || digest || 0x00 x 11` ([`hash::Oid::to_bytes32`]).
//! - **Tree ordering**: entry key is `name` plus a trailing `/` for subtrees,
//!   compared as unsigned bytes ([`tree`]).
//! - **Path safety**: valid UTF-8, NFC-normalized, non-empty, no `/`, NUL,
//!   `.`/`..`, or absolute paths ([`path`]).
//! - **Commit serialization** exactly as Git writes it ([`commit`]).
//! - **Attestation**: canonical CBOR with sorted keys, hashed as
//!   `H("forge-attestation\0" || canonical_cbor)` ([`attestation`]).
//! - **History/repo roots**: domain-separated hash chain; `seq` and
//!   `commit_count` little-endian `u64` ([`history`]).
//!
//! # Determinism
//!
//! All encoders are deterministic. Every decoder that consumes untrusted bytes
//! (e.g. [`attestation::Attestation::from_canonical_cbor`]) re-encodes and
//! rejects non-canonical input, and never panics on malformed data.
//!
//! The crate forbids `unsafe` code.

#![forbid(unsafe_code)]
#![warn(clippy::all, clippy::pedantic)]
// Pure protocol constructors return values callers should not discard; adding
// `#[must_use]` to every one is noise for a low-level protocol library.
#![allow(clippy::must_use_candidate)]
// `expect`/`panic!` occur only for invariants proven by construction (for
// example, a digest always matches its algorithm's length).
#![allow(clippy::missing_panics_doc)]

pub mod attestation;
pub mod blob;
mod cbor;
pub mod commit;
pub mod error;
pub mod hash;
pub mod history;
pub mod object;
pub mod path;
pub mod tree;

pub use attestation::Attestation;
pub use commit::{Commit, Identity};
pub use error::ObjectError;
pub use hash::{HashAlgorithm, Oid};
pub use object::{oid, serialize, ObjectType};
pub use tree::{EntryMode, TreeBuilder, TreeEntry};
