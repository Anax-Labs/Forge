//! `forge` CLI entry point.
//!
//! Phase 1 scaffolding: exposes the command surface from the architecture
//! spec (§13). Handlers are implemented in Phases 7–9.

mod attest;
mod chain;
mod commands;
mod git;
mod storage;

use clap::{Parser, Subcommand};

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

/// Command surface from §13. Tiers: MVP, SHOULD, FUTURE.
#[allow(dead_code)]
#[derive(Debug, Subcommand)]
enum Command {
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
    /// Verify a commit and its anchored history inclusion
    Verify { commit: Option<String> },
    /// Verify a deployed program's source provenance
    VerifyProgram { program_id: String },
    /// Create a release tag
    Tag { name: String },
    /// Merge a branch
    Merge { branch: String },
    /// Garbage-collect and verify storage availability
    Gc {
        #[arg(long)]
        verify_availability: bool,
    },
}

#[allow(dead_code)]
#[derive(Debug, Subcommand)]
enum RemoteAction {
    Add { name: String, repo: String },
    List,
    Remove { name: String },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if let Some(command) = cli.command {
        commands::dispatch(command)
    } else {
        use clap::CommandFactory;
        Cli::command().print_help()?;
        Ok(())
    }
}
