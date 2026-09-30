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
- **Reinitialization / revival:** `init` only; `init_if_needed` is forbidden.
  Duplicate repository/branch creation fails at account init.
- **Overflow:** any future arithmetic will use checked math; no unbounded
  arithmetic exists yet.
