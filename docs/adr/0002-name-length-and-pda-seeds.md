# ADR 0002 — 32-byte names and PDA seed layout

- **Status:** Accepted (Phase 3)
- **Resolves:** the seed-length/name-padding risk flagged in Phase 3 of
  `phase_implementation.md` (spec §4.4/§4.5 sketch `[u8; 64]` names).
- **Spec:** §4 (data model), §2.3 (`MAX_SEED_LEN = 32`), §5.3 (path safety).

## Context

Solana PDA seeds are limited to 16 seeds of at most 32 bytes each. The spec's
`BranchAccount.name: [u8; 64]` and `TagAccount.name: [u8; 64]` sketches cannot
be used as a single seed, and splitting a 64-byte name into two seeds makes the
default-branch derivation (which receives a 32-byte `default_branch` argument)
inconsistent with `create_branch`.

Anchor seeds **may** reference instruction arguments when those arguments are
declared with `#[instruction(...)]` on the `#[derive(Accounts)]` struct. This
lets `initialize_repository` derive the default branch PDA from the
`default_branch` argument directly.

## Decision

- Cap all repository, branch, and tag names at **32 bytes** (`[u8; 32]`), NUL
  padded. `BranchAccount.name` and `TagAccount.name` are frozen as `[u8; 32]`
  rather than `[u8; 64]`.
- Seed layouts (all single, ≤32-byte seeds):
  - repository: `["repo", owner, name]`
  - branch: `["branch", repository, name]`
  - commit: `["commit", repository, commit_oid]`
  - tag: `["tag", repository, name]`
  - permission: `["perm", repository, contributor]`
  - program attestation: `["prog", program_id]`
- Name validity: non-empty printable ASCII (`0x21..=0x7e`) with NUL padding only
  (`src/name.rs`). This is a documented, stricter deviation from Git refnames.

## Consequences

- Names longer than 32 bytes are unsupported in the MVP. This is sufficient for
  the hackathon and can be extended later only via a migration.
- Because names are protocol identifiers (not filesystem paths), the stricter
  ASCII charset is acceptable and prevents control characters reaching clients.
- `initialize_repository` creates the default branch in the same instruction
  using `#[instruction(...)]` for seeds, so no separate bootstrap call is needed.
- Account sizes changed from the spec's approximations; the exact, frozen sizes
  are documented in ADR 0003 and asserted by tests.
