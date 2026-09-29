//! Program instruction handlers (one module per instruction).

pub mod anchor_program_source;
pub mod create_branch;
pub mod create_commit;
pub mod create_tag;
pub mod initialize_repository;
pub mod merge;
pub mod update_branch;
pub mod update_permissions;

// Re-exports are added alongside handlers in Phases 3–9.
