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

- Forged commits → onchain Ed25519 verification (Phase 4).
- Unauthorized branch update / replay → authority + `head_seq` CAS (Phases 4–5).
- History rewrite → append-only `history_root` + logged reset (Phases 4–5).
- Storage disappearance → multi-pin + Arweave + `gc --verify-availability` (Phase 6).
- Path traversal / symlink payloads → safe tree construction (Phase 2).
- Malicious metadata → length/charset limits, no log parsing (Phases 3, 9).
