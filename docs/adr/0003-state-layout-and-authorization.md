# ADR 0003 — Phase 3 account layouts, authorization, and scope

- **Status:** Accepted (Phase 3)
- **Spec:** §4 (state), §7.5/§16.4 (authorization), §9.1/§9.2 (instructions).
- **Related:** ADR 0001 (events), ADR 0002 (names/seeds).

## Context

Phase 3 freezes the onchain account layouts; changing them later requires a
migration. It also establishes MVP authorization and decides which of the six
account types get instructions now.

## Decision

### Frozen layouts (serialized size excluding Anchor's 8-byte discriminator)

| Account | `LEN` | Notes |
|---|---:|---|
| `RepositoryAccount` | 248 | includes `_reserved: [u8; 64]` |
| `BranchAccount` | 179 | includes `_reserved: [u8; 32]` |
| `CommitAccount` | 274 | struct only until Phase 4 |
| `TagAccount` | 170 | struct only until Phase 9 |
| `PermissionAccount` | 82 | struct only until Phase 9 |
| `ProgramSourceAttestation` | 202 | struct only until Phase 9 |

`space = 8 + T::LEN` for `init`. Fixed-size fields come first; `_reserved`
headroom is trailing so fields can be appended without a layout break.
`RepositoryAccount.repo_id` denormalizes the PDA for indexers.

### Authorization

MVP is **owner-only**. `auth::require_repo_owner` compares the signer to
`RepositoryAccount.owner`. `BranchAccount.authority` is recorded but not yet
enforced. Phase 9 adds `permissions_mode = 1` (allowlist) and `= 2`
(program-owned authority PDA, §16.2).

### Scope

- Implemented instructions: `initialize_repository`, `create_branch`.
- The other four account structs are defined now (layout frozen) but have no
  instruction until their scheduled phases. This was Open Question 7; the
  resolution is "define the struct in Phase 3, defer the instruction".
- Default branch creation is part of `initialize_repository` (atomic), per
  §9.2, enabled by `#[instruction(...)]` seeds (ADR 0002).
- Canonical PDA bumps are stored in each account (§11 bump canonicalization).
- `create_branch` accepts an optional existing commit via `remaining_accounts`
  when `from_commit != 0`; Phase 3 has no commits, so only the empty path is
  exercised until Phase 4.

## Consequences

- Later phases must not reorder fields; they append and shrink `_reserved`.
- Instructions inherit `event_authority` + `program` accounts from ADR 0001.
- Exact byte sizes are asserted in `tests/phase3.rs`; changing `LEN` without a
  migration is a protocol break.
