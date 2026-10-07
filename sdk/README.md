# @onchain-forge/sdk

TypeScript SDK for the Forge protocol.

Status: **core implemented (Phase 10a).** The canonical encoding/verification
primitives are dependency-free TypeScript and run under `node --test` with **no
`npm install`**. The onchain RPC/transaction layer (`@solana/kit` v8+ + Codama
client) is Phase 10b.

## Modules

- `src/cbor.ts` — canonical CBOR (RFC 8949 core deterministic encoding)
- `src/oid.ts` — algorithm-tagged OIDs, canonical 32-byte form, hex helpers
- `src/object.ts` — Git object framing and object-id computation
- `src/history.ts` — `history_root` chain and `repo_root`
- `src/attestation.ts` — canonical attestation + `attestation_hash`
- `src/discriminator.ts` — Anchor `global:`/`account:` discriminators

## Determinism

`test/golden.test.ts` cross-checks `../tests/vectors/golden.json` — the exact
values the Rust `forge-object` engine produces (and which are cross-checked
against real `git`). The TS implementation must stay byte-identical for both
SHA-1 and SHA-256.

```bash
node --test 'test/**/*.test.ts'
```

Every hashed value read from storage or RPC must be re-verified client-side
(§23); the SDK never trusts an indexer or gateway.

## Phase 10b (planned)

- `@solana/kit` client + Codama-generated program client
- instruction builders (create/update branch, commit, tag, permissions, provenance)
- PDA derivation, RPC reads, transaction assembly
- `getFile` with hash verification against the tree
