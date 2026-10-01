# Forge Threat Model

> Placeholder. Tracked against §11 of the architecture spec; mitigations are
> marked MVP/Future there.

## Cross-cutting program checklist (§11)

For every instruction: validate owner, signer, writable; PDA seeds + canonical
bump; no arbitrary CPI; checked math; duplicate-mutable-account guard; secure
close; treat `remaining_accounts` as untrusted; test each error path.

## Explicit MVP non-goals (§11, §19.4)

Confidentiality / private repos, anonymity, storage-provider economic
incentives, fully trustless onchain rebuild of every historical commit,
custom merge engine, general rule VM, onchain file storage.

## Open threats tracked for implementation

- Forged commits → onchain Ed25519 verification in `create_commit` (Phase 4, `programs/forge_repository/src/ed25519.rs`).
- Unauthorized branch update / replay → authority + `head_seq` CAS (Phases 4–5).
- History rewrite → append-only `history_root` + logged reset (Phases 4–5).
- Storage disappearance → multi-pin + Arweave + `gc --verify-availability` (Phase 6).
- Path traversal / symlink payloads → safe tree construction (Phase 2).
- Malicious metadata → length/charset limits, no log parsing (Phases 3, 9).

## Phase 3 status (implemented)

- **Malicious metadata:** repository/branch names are validated as non-empty
  printable ASCII with NUL-only padding (`src/name.rs`, ADR 0002); bounded
  32-byte buffers are stored onchain.
- **Unauthorized writes:** `initialize_repository` and `create_branch` enforce
  owner-only authorization (`src/auth.rs`, §16.4).
- **PDA confusion / squatting:** every `init` account uses explicit seeds with
  the canonical bump, and the canonical bump is stored in account data
  (`src/pda.rs`, §11). `create_branch` validates any `from_commit` account's
  program owner, discriminator, PDA, repository binding, and oid.
- **Log injection:** events are emitted via `emit_cpi!` call data, never parsed
  from string logs (ADR 0001, §11 #12).
- **Reinitialization / revival:** PDA accounts are created through the guarded
  `create_pda` helper, which rejects a non-empty account before creating it, and
  addresses are pinned by `seeds` + `bump`. `init_if_needed` is forbidden.
  Duplicate repository/branch creation returns `RepositoryAlreadyExists` /
  `BranchAlreadyExists`.
- **Overflow:** no arithmetic existed in Phase 3; Phase 4 uses `checked_add`
  (`commit_count + 1`, Ed25519 offset arithmetic) returning `MathOverflow`.

## Phase 4 status (implemented)

- **Forged commits:** `create_commit` verifies the author's Ed25519 signature over
  `attestation_hash` via Instructions-sysvar introspection (`src/ed25519.rs`,
  ADR 0004). Wrong pubkey/message → `BadSignature`; a valid signature over the
  wrong message, a forged-author signature, and a non-owner author are all
  rejected (tests: `create_commit_rejects_forged_author_signature`,
  `create_commit_rejects_unauthorized_author`).
- **Multiple / malformed Ed25519 instructions:** exactly one Ed25519 instruction
  is required in the transaction; header, indices, offsets, and non-empty message
  are validated. Parser edge cases are covered by unit tests (the native program
  rejects malformed instructions before they reach the program).
- **Replay / duplicates:** a second `create_commit` for the same
  `(repo, commit_oid)` fails at Anchor `init` (idempotent by construction, §6.7);
  `attestation_hash` binds `repo` and `commit`, preventing cross-repo replay.
- **Invalid parents:** `parent_count ∈ {0,1,2}`, zero-oid rejection, self-parent,
  missing/unknown parent account, and root-on-nonempty are rejected with distinct
  codes (`InvalidParentCount`, `InvalidCommitOid`, `SelfParent`, `InvalidParent`,
  `UnknownCommit`, `InvalidPda`, `RootOnNonemptyRepo`).
- **Compute budget:** `create_commit` ≈ 32.7k CU including the precompile,
  asserted `< 200_000` in `create_commit_compute_units_within_default_limit`.
