# ADR 0007 — Local-first CLI (Git object store, wallet file, sidecar split)

- **Status:** Accepted (Phase 7)
- **Spec:** §13 (CLI), §14.1 (`.forge/` layout), §5.5 (attestation CBOR),
  §12.4 (verify exit codes), §15 (wallet identity).
- **Resolves:** Open questions #1 (`gix` vs `git2`) and #5 (wallet path).

## Context

Phase 7 must give developers a fully offline `forge` workflow on top of Git.
`gix` 0.88 can open SHA-256 repositories (via the `sha256` feature) but the
battle-tested writer for SHA-256 objects remains the `git` CLI, which Phase 2
already uses for golden-vector cross-checks.

The wallet that signs `attestation_hash` was unspecified. The sidecar file is
specified as *exactly* canonical CBOR, so a signature cannot be stuffed into
`.cbor` without breaking `H("forge-attestation\0" || file_bytes)`.

## Decision

### Git is the object store; `forge-object` is the hasher

- `forge init` runs `git init --object-format=sha256 -b main`.
- `add` / `checkout` / `branch` / `status` / `log` / `update-ref` go through
  the `git` CLI.
- Commit trees and commit payloads are built with `forge-object` and written
  with `git hash-object -w`. `git write-tree` must equal the Forge tree oid
  or the commit is aborted.
- `gix::open` is used after init as a SHA-256 sanity check.

This keeps `git rev-parse HEAD` byte-identical to the Forge commit oid
(§13 validation) without re-implementing packfiles.

### Wallet file

1. `FORGE_WALLET` (Solana CLI JSON keypair) if set.
2. Else `.forge/config.wallet`, default `.forge/id.json` created on init.
3. `.forge/id.json` is appended to the repo `.gitignore`.

Hardware wallets are out of scope until Phase 8/10.

### Sidecar split

| File | Contents |
|---|---|
| `.forge/attestations/<oid-hex>.cbor` | `canonical_cbor(attestation)` only |
| `.forge/attestations/<oid-hex>.sig` | 64-byte Ed25519 signature over `attestation_hash` |

Local `forge verify` checks OIDs, attestation binding, and the signature.
History inclusion is **not** checked (exit code 0 = `LOCAL_VERIFIED`).
§12.4 codes: `0` verified, `1` mismatch, `2` claim-only (unused locally),
`3` missing sidecar/HEAD.

Commits made before a remote exists use attestation `repo = "local"`.
`forge remote add` records the PDA in config for later commits (Phase 8).

## Consequences

- A Git binary is required for Phase 7 (already required by `forge-object`
  git-compat tests).
- Phase 8 `push` reads `.cbor` + `.sig` to prepend the Ed25519 instruction.
- Display `author`/`committer` strings in the Git commit are `Forge
  <pubkey@forge>`; authorship is the wallet signature.
