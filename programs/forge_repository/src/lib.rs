//! Forge repository program.
//!
//! Onchain anchor for Git-like repository history, branch refs, authorship
//! attestations, and deployed-program source provenance.
//!
//! Phase 1 scaffolding: module layout only. Instructions and account state are
//! implemented in later phases (see `phase_implementation.md`).

pub mod constants;
pub mod errors;
pub mod events;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

declare_id!("4smCAEoycSXSvVsyic8ircQmHmENPCHn17Fma83SYVbf");

#[program]
pub mod forge_repository {}
