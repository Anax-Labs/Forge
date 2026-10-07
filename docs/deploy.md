# Deploying Forge

End-to-end deployment runbook: onchain program → CLI → explorer.

## 0. Prerequisites

- Rust **1.89.0** (pinned via `rust-toolchain.toml`), Solana CLI 3.x, Anchor CLI
  **1.1.2**, Bun 1.4+.
- A devnet wallet with SOL (upgrade authority + fee payer).
- IPFS pinning endpoint(s) for real storage (optional for a single-machine demo).
- The **program keypair** — see step 2.

## 1. Build

```bash
anchor build     # target/deploy/forge_repository.so + target/idl/forge_repository.json
```

## 2. Program keypair (critical)

`declare_id!`, `Anchor.toml`, and the local keypair must all be
`4smCAEoycSXSvVsyic8ircQmHmENPCHn17Fma83SYVbf`. The keypair is **git-ignored**, so
a fresh clone does not have it.

- **If you have the `4smCAE…` keypair:** place it at
  `target/deploy/forge_repository-keypair.json`.
- **If you do not:** generate a new one and deliberately re-point everything.
  Do **not** run `anchor keys sync` (it silently rewrites the ID):

  ```bash
  solana-keygen new --no-bip39-passphrase -o target/deploy/forge_repository-keypair.json
  solana-keygen pubkey target/deploy/forge_repository-keypair.json
  # update declare_id! and BOTH [programs.localnet] / [programs.devnet] in
  # Anchor.toml to the new pubkey, then rebuild
  anchor build
  ```

`anchor deploy` requires the keypair to match `declare_id!`.

## 3. Fund and deploy to devnet

```bash
solana config set --url devnet
solana airdrop 2          # or the web faucet if rate-limited
anchor deploy --provider.cluster devnet
solana program show 4smCAEoycSXSvVsyic8ircQmHmENPCHn17Fma83SYVbf --url devnet
```

## 4. Configure the CLI

```bash
export FORGE_RPC="https://devnet.helius-rpc.com/?api-key=..."
export FORGE_PROGRAM_ID="4smCAEoycSXSvVsyic8ircQmHmENPCHn17Fma83SYVbf"

# Storage (optional; without these the CLI uses two local CAS directories).
export FORGE_IPFS_API="http://127.0.0.1:5001"        # Kubo HTTP API
export FORGE_IPFS_API_2="https://api.pinata.cloud"   # second pin
export FORGE_ARWEAVE_URL="https://node2.irys.xyz"    # tag checkpoints (optional)
export FORGE_STORAGE="/path/to/shared-cas"           # optional shared CAS root
```

The CLI signs with a **repo-local** wallet at `.forge/id.json` (created by
`forge init`), not `~/.config/solana/id.json`. Fund it for pushes:

```bash
solana airdrop 2 "$(solana-keygen pubkey .forge/id.json)" --url devnet
```

## 5. Run it end-to-end

```bash
cargo build --release -p forge     # binary at target/release/forge

forge init && forge add . && forge commit -m "Initial protocol" && forge push
forge clone <repo-pda> forge2 && (cd forge2 && forge verify)
forge permissions set <wallet> writer
forge tag v1.0.0 --checkpoint
forge verify-program <program-id>  # VERIFIED / MISMATCH / UNVERIFIED_CLAIM
```

## 6. Deploy the explorer (`web/`)

**Vercel (simplest):** import the repo, set **Root Directory** to `web`, build
command `bun run build`, and environment:

```
NEXT_PUBLIC_FORGE_RPC=https://devnet.helius-rpc.com/?api-key=...
NEXT_PUBLIC_FORGE_PROGRAM_ID=4smCAEoycSXSvVsyic8ircQmHmENPCHn17Fma83SYVbf
```

**Self-host:**

```bash
cd web && bun install && bun run build && bun run start   # http://localhost:3000
```

## 7. Optional indexer (`indexer/`)

```bash
FORGE_RPC=... FORGE_PROGRAM_ID=... bun run src/index.ts   # writes .forge-index.json
```

Non-authoritative cache only — never a dependency of verification.

## Gotchas

- **Rent/SOL:** program deploy needs ~2–3 SOL of (recoverable) rent; devnet
  airdrops are rate-limited.
- **Keypair drift:** the most common failure is `declare_id!` ↔ keypair mismatch
  (step 2).
- **Availability, not size, is the storage risk:** if nothing pins the blobs,
  history is provable but not fetchable. Use ≥2 pins (or Arweave for tags).
- **Upgrade authority:** `anchor deploy` uses `~/.config/solana/id.json`; keep it
  safe (or set a Squads multisig as owner).
- **Live parameters:** `lamports_per_byte` and transaction-size activation
  change over time; re-verify before benchmarking (`docs/benchmark.md`).
