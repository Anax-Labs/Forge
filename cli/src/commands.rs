//! CLI command dispatch.
//!
//! Phase 6 implements `gc --verify-availability`. Remaining handlers land in
//! Phases 7–9.

use crate::Command;

/// Dispatch a parsed command.
///
/// Takes ownership so later phases can destructure subcommand arguments.
#[allow(clippy::needless_pass_by_value)]
pub fn dispatch(command: Command) -> anyhow::Result<()> {
    match command {
        Command::Gc {
            verify_availability,
        } => gc(verify_availability),
        other => anyhow::bail!("`{other:?}` is not implemented yet (see phase_implementation.md)"),
    }
}

fn gc(verify_availability: bool) -> anyhow::Result<()> {
    if !verify_availability {
        anyhow::bail!(
            "object pruning is not implemented in Phase 6; pass --verify-availability to report unbacked objects (§8.3)"
        );
    }
    let dir = std::env::current_dir()?;
    let report = crate::storage::report_for_forge_dir(&dir, &[])?;
    print!("{}", report.render());
    if !report.fully_available() {
        anyhow::bail!("one or more objects are unbacked, degraded, or corrupt");
    }
    Ok(())
}
