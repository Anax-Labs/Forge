# ADR 0004 — `create_commit` authorship and Ed25519 introspection

- **Status:** Accepted (Phase 4)
- **Spec:** §5.5 (attestation), §6.4/§6.7/§6.9 (commit/parents/duplicates),
  §9.2 (`create_commit`), §9.4 (Ed25519 introspection), §9.5 (compute budget).
- **Related:** ADR 0001 (events), ADR 0003 (state layout / guarded init).

## Context

`create_commit` anchors a Git commit and must prove *authorship* without trusting
the transaction sender: the wallet signs the canonical attestation hash offchain
(§5.5), and the program verifies that signature onchain. A Solana program cannot
call an arbitrary signature verifier, so it uses the standard
Instructions-sysvar introspection pattern (§9.4).

## Decision

### Verification pattern

The client prepends the **Ed25519 native program** instruction with
`(author, signature, message = attestation_hash)` and passes the Instructions
sysvar. `src/ed25519.rs::verify_ed25519_instruction_preceding` then:

1. requires a non-empty expected message and `current_index > 0`;
2. counts Ed25519-program instructions in `0..=current_index` and requires
   **exactly one** (blocks the cached/duplicate-signature footgun);
3. requires the immediately preceding instruction to be the Ed25519 program;
4. parses the 16-byte header, requires `num_signatures == 1`, zero padding,
   current-instruction indices (`0` or the `u16::MAX` sentinel), offsets at or
   after the data start, and `message_size > 0`;
5. asserts `public_key == author` and `message == attestation_hash`
   (`BadSignature` on mismatch) and that the signature bytes are non-zero.

Because the Ed25519 instruction executes in the same transaction, an invalid
signature makes the transaction fail before `create_commit` succeeds; the
introspection above only proves the *right key* signed the *right message*.

### What the signature binds

Authorship binds `repo` and `commit` transitively: `attestation_hash` is computed
over the canonical attestation, which includes `repo`, `commit`, `parents`,
`tree`, `author`, `authoredAt`, `messageHash`, and a nonce (§5.5). A signature
for repo A cannot be replayed into repo B.

### Scope: head-only onchain verification

Only the commit being created is verified onchain; ancestors are verified
client-side during `forge verify` (Phase 8), because the append-only
`history_root` already binds them (§9.4). This is an explicit MVP simplification.

### Duplicate commits use Anchor `init`

`CommitAccount` is created with Anchor `#[account(init)]`, so a second
`create_commit` for the same `(repo, commit_oid)` fails at account creation
(idempotent by construction, §6.7). This intentionally differs from the guarded
manual init used for repositories/branches (ADR 0003), which exists only to
return protocol-specific "already exists" errors that §6.7 does not require.

### Compute budget

Measured on LiteSVM (including the Ed25519 precompile):
**`create_commit` ≈ 32,764 CU**, far below the 200k default per-instruction
limit (§9.5). No `SetComputeUnitLimit` is required for the MVP; clients may still
set a tighter limit.

### Testing note

The Ed25519 precompile only runs in LiteSVM when its `precompiles` feature is
enabled; `programs/forge_repository/Cargo.toml` turns it on for dev-dependencies,
and the test harness registers the precompile account. Parser edge cases that the
native program would reject before reaching the program are covered by unit tests
in `src/ed25519.rs`.

## Consequences

- `verify_ed25519_instruction_preceding` is generic over
  `(expected_author, expected_message)` and is reused by Phase 5 branch-update
  authorization.
- Ancestor signature verification remains a client responsibility (Phase 8).
- Adding new signed message types requires only a new expected-message value, not
  a new verifier.
