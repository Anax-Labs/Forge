# ADR 0001 — Event emission via `emit_cpi!`

- **Status:** Accepted (Phase 3)
- **Resolves:** Open Question 13 in `phase_implementation.md` (`emit_cpi!` vs noop-CPI vs legacy logs).
- **Spec:** §9.1 (events for indexers), §11 #12 (never parse logs as authoritative).

## Context

Phase 3 must emit `RepositoryInitialized` / `BranchCreated` for indexers. Anchor
offers three mechanisms:

1. `emit!` with `#[event]` — structured Borsh payload written via `sol_log_data`
   (`Program data: <base64>`). Simple, but RPCs may truncate program logs.
2. `emit_cpi!` with `#[event_cpi]` — a self-CPI carrying the event in call data.
   Not truncated, but adds `event_authority` and `program` accounts to every
   annotated instruction and requires a self-CPI.
3. Raw string logs — explicitly forbidden as an authoritative data source.

## Decision

Use `emit_cpi!` for all program events. The `event-cpi` feature is enabled on
`anchor-lang` in `programs/forge_repository/Cargo.toml`, and instruction account
structs carry `#[event_cpi]` so Anchor appends the two required accounts.

## Consequences

- Events are durable in transaction metadata and cannot be silently truncated.
- Clients/integrations must pass `event_authority`
  (`["__event_authority"]` PDA) and the program account after `system_program`.
  This is part of the frozen instruction interface.
- Every event-emitting instruction pays a small self-CPI cost.
- The program now requires **Anchor ≥ 0.30**; the workspace already pins a
  compatible `anchor-lang`.
