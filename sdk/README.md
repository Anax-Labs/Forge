# @onchain-forge/sdk

TypeScript SDK for the Forge protocol, built on `@solana/kit` v8+ with a
Codama-generated program client.

Status: **scaffold** — implemented in Phase 10 (see `../phase_implementation.md`).

Planned modules (§22):

- `src/client.ts` — configured `@solana/kit` client
- `src/repo.ts` — repository/branch read + create helpers
- `src/commit.ts` — commit creation/read
- `src/verify.ts` — client-side re-verification of OIDs, signatures, history
- `src/storage.ts` — content-addressed fetch with hash verification

Every hashed value read from storage or RPC must be re-verified client-side
(§23); the SDK must never trust an indexer or gateway.
