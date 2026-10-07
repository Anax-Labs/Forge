# Forge Explorer (web)

Next.js (App Router) explorer: repository list, commit log with author +
verification badges, tags, and program provenance. Read-only.

Status: **implemented (Phase 10b).**

## How it works

- **No indexer required.** Accounts are read via JSON-RPC `getProgramAccounts`
  with discriminator + `memcmp` filters; PDAs are not re-derived client-side.
- **Client-side verification.** The `history_root` is recomputed from the
  anchored commit set with WebCrypto (`lib/verify.ts`) and compared to
  `RepositoryAccount.history_root`; the badge reflects the result. The explorer
  never trusts the RPC for this.
- **Frozen layouts.** `lib/decode.ts` decodes the account structs from ADR 0003.

## Configuration

```bash
NEXT_PUBLIC_FORGE_RPC=https://devnet.helius-rpc.com/?api-key=... \
NEXT_PUBLIC_FORGE_PROGRAM_ID=4smCAEoycSXSvVsyic8ircQmHmENPCHn17Fma83SYVbf \
bun run dev
```

## Commands (Bun)

```bash
bun install
bun run dev      # http://localhost:3000
bun run build    # production build
bun test         # decode/verify unit tests
```
