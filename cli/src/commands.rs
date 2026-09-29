//! CLI command dispatch.
//!
//! Phase 1 scaffolding. Command handlers are implemented in Phases 7–9.

use crate::Command;

/// Dispatch a parsed command. Returns an error until the owning phase lands.
///
/// Takes ownership so later phases can destructure subcommand arguments.
#[allow(clippy::needless_pass_by_value)]
pub fn dispatch(command: Command) -> anyhow::Result<()> {
    anyhow::bail!("`{command:?}` is not implemented yet (see phase_implementation.md)")
}
