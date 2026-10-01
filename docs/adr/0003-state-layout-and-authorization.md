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

`space = 8 + T::LEN` for initialization. Fixed-size fields come first;
`_reserved` headroom is trailing so fields can be appended without a layout
break. `RepositoryAccount.repo_id` denormalizes the PDA for indexers.

### Initialization: guarded manual create

PDA accounts are created by the `create_pda` helper (`src/init.rs`) rather than
Anchor `#[account(init)]`. Anchor `init` is robust but reports duplicates with
the system program's generic `AccountAlreadyInUse`; Forge requires
protocol-specific errors (`RepositoryAlreadyExists`, `BranchAlreadyExists`) so
clients can tell "this name is taken" from other failures. The helper:

- rejects a non-empty account with the caller's error (reinitialization guard),
- creates the account via `system_program::create_account` with `space = 8 + LEN`
  and the program as owner, and
- serializes the account state.

The address of every such account is still pinned by a `seeds` + `bump`
constraint on an `UncheckedAccount`, so it cannot be substituted, and
`init_if_needed` remains forbidden.

### Authorization

MVP is **owner-only**. `auth::require_repo_owner` compares the signer to
`RepositoryAccount.owner`. In `create_branch` the signer account is named
`authority` (matching §9.2) and becomes the branch's recorded update authority;
it is not yet possible to differ from the owner. Phase 9 adds
`permissions_mode = 1` (allowlist) and `= 2` (program-owned authority PDA,
§16.2).

### Read-only access

There are no getter instructions. Repository and branch state is read by
deserializing the account at its deterministic PDA (the standard Solana model);
the account structs are public and exported, and the IDL describes their
layouts. Tests assert the stored fields directly.

### Scope

- Implemented instructions: `initialize_repository`, `create_branch`.
- The other four account structs are defined now (layout frozen) but have no
  instruction until their scheduled phases. This was Open Question 7; the
  resolution is "define the struct in Phase 3, defer the instruction".
- Default branch creation is part of `initialize_repository` (atomic), per
  §9.2, enabled by `#[instruction(...)]` seeds (ADR 0002).
- Canonical PDA bumps are stored in each account (§11 bump canonicalization).
- `create_branch` accepts an optional existing commit via `remaining_accounts`
  when `from_commit != 0`; the success path is covered by `tests/phase3.rs` using
  a fabricated `CommitAccount`, and the real producer lands in Phase 4.
- Checked-math / overflow error handling is deferred until Phase 4 introduces
  arithmetic; there is no arithmetic in Phase 3.

## Consequences

- Later phases must not reorder fields; they append and shrink `_reserved`.
- Instructions inherit `event_authority` + `program` accounts from ADR 0001.
- Exact byte sizes are asserted in `tests/phase3.rs`; changing `LEN` without a
  migration is a protocol break.
- Duplicate repository/branch creation returns a stable Forge error code
  (6005 / 6003) instead of the generic system error.
