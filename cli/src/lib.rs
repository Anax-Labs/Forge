//! Forge CLI library (Phases 6–8).
//!
//! The `forge` binary is a thin clap wrapper around this crate so LiteSVM
//! integration tests can call push/clone/verify in-process.

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::similar_names)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::module_name_repetitions)]

pub mod attest;
pub mod chain;
pub mod commands;
pub mod config;
pub mod git;
pub mod storage;
pub mod sync;
pub mod wallet;

use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// Command surface from §13.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Initialize a local Forge repository
    Init { dir: Option<String> },
    /// Stage paths in the local repository
    Add { paths: Vec<String> },
    /// Create a signed commit locally
    Commit {
        #[arg(short, long)]
        message: String,
    },
    /// Push commits to the onchain repository
    Push {
        remote: Option<String>,
        branch: Option<String>,
    },
    /// Pull/fast-forward from the onchain repository
    Pull { remote: Option<String> },
    /// Clone an onchain repository
    Clone { repo: String, dir: Option<String> },
    /// Show anchored commit history
    Log { branch: Option<String> },
    /// Show local / chain divergence
    Status,
    /// List or create branches
    Branch { name: Option<String> },
    /// Check out a ref
    Checkout { r#ref: String },
    /// Manage remotes
    Remote {
        #[command(subcommand)]
        action: RemoteAction,
    },
    /// Verify a commit (OIDs, signature, history inclusion)
    Verify { commit: Option<String> },
    /// Verify a deployed program's source provenance
    VerifyProgram { program_id: String },
    /// Create a release tag
    Tag {
        name: String,
        /// Also upload a release checkpoint to Arweave (requires config).
        #[arg(long)]
        checkpoint: bool,
    },
    /// Merge a branch
    Merge { branch: String },
    /// Manage contributor roles
    Permissions {
        #[command(subcommand)]
        action: PermissionsAction,
    },
    /// Garbage-collect and verify storage availability
    Gc {
        #[arg(long)]
        verify_availability: bool,
    },
}

/// Remote subcommands.
#[derive(Debug, Subcommand)]
pub enum RemoteAction {
    /// Record a repository PDA
    Add { name: String, repo: String },
    /// List remotes
    List,
    /// Remove a remote
    Remove { name: String },
}

/// Permission subcommands.
#[derive(Debug, Subcommand)]
pub enum PermissionsAction {
    /// Grant or update a contributor's role
    Set {
        contributor: String,
        role: String,
        /// Slot the role expires at (0 = never)
        #[arg(long, default_value_t = 0)]
        expires_slot: u64,
    },
    /// Show a contributor's role
    Get { contributor: String },
}

#[derive(Parser)]
#[command(
    name = "forge",
    version,
    about = "Forge — onchain version control on Solana",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

/// `forge verify` exit codes per §12.4 (0/1/2/3).
#[derive(Debug, Clone, Copy)]
pub enum VerifyStatus {
    /// OID, signature, or history mismatch.
    Mismatch = 1,
    /// Claim recorded but not independently rebuilt (Phase 9).
    #[allow(dead_code)]
    ClaimOnly = 2,
    /// Sidecar, HEAD, or onchain account missing.
    MissingData = 3,
}

/// Process exit with a specific code (used by `verify`).
#[derive(Debug)]
pub struct CliExit {
    /// Exit code.
    pub code: u8,
    /// Message written to stderr.
    pub message: String,
}

impl VerifyStatus {
    /// Attach a human-readable reason.
    pub fn with(self, message: impl Into<String>) -> CliExit {
        CliExit {
            code: self as u8,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for CliExit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CliExit {}

/// Parse argv and run a command.
#[must_use]
pub fn run() -> ExitCode {
    let cli = Cli::parse();
    let result = if let Some(command) = cli.command {
        commands::dispatch(command)
    } else {
        use clap::CommandFactory;
        match Cli::command().print_help() {
            Ok(()) => Ok(()),
            Err(err) => Err(anyhow::Error::from(err)),
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err:#}");
            if let Some(exit) = err.downcast_ref::<CliExit>() {
                ExitCode::from(exit.code)
            } else {
                ExitCode::from(1)
            }
        }
    }
}
