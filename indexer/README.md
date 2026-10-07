# Forge Indexer

Optional, **non-authoritative** cache over the onchain accounts, built on Bun.
It reuses the explorer's decoders (`../web/lib`) and writes a JSON snapshot.

Status: **implemented (Phase 10b).**

This service is a UX cache only and is **never authoritative** (§3.2, §9.1).
Verification paths must not depend on it.

```bash
FORGE_RPC=https://devnet.helius-rpc.com/?api-key=... \
FORGE_PROGRAM_ID=4smCAEoycSXSvVsyic8ircQmHmENPCHn17Fma83SYVbf \
bun run start          # writes .forge-index.json

bun test
```

The MVP uses per-commit accounts, so a plain RPC read suffices; the indexer is
only needed for fast queries over many repositories.
