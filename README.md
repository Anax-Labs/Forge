# Forge

**Onchain version control on Solana.** Git-like repositories whose history,
authorship, branch state, and source-code provenance are cryptographically
anchored on Solana, while file contents live in content-addressed offchain
storage.

Working name: `Forge` (protocol), `forge` (CLI).

> Status: **Phase 9 complete — tags, permissions, ownership transfer, and program
> source provenance.** Phases 1–8 (canonical engine, onchain core, storage,
> local + onchain CLI) plus the Phase 9 program instructions (`create_tag`,
> `update_permissions`, `transfer_repository`, `anchor_program_source`), role
> enforcement, and the `forge tag` / `forge merge` / `forge verify-program` /
> `forge permissions` commands. See
> [`phase_implementation.md`](phase_implementation.md) for the 10-phase roadmap,
> [`docs/adr/`](docs/adr/) for frozen design decisions, and
> [`ONCHAIN_VERSION_CONTROL_ARCHITECTURE.md`](ONCHAIN_VERSION_CONTROL_ARCHITECTURE.md)
> for the full architecture specification.

## Repository layout

```
programs/forge_repository/   Anchor program (accounts, instructions, events)
crates/forge-object/         Canonical Git-object/hashing/attestation engine (Phase 2)
crates/forge-storage/        IPFS/Arweave CAS, CAR bundles, storage-index (Phase 6)
cli/                         Rust `forge` CLI (Git SHA-256 + `.forge/` metadata)
sdk/                         TypeScript SDK (@solana/kit + Codama client) (Phase 10)
web/                         Next.js explorer / verify UI (Phase 10)
indexer/                     Optional Helius webhook indexer, non-authoritative (Phase 10)
tests/vectors/               Golden Git OID / attestation vectors (Phase 2)
tests/e2e/                   End-to-end flows (Phases 7–10; local CLI tests in `cli/tests/`)
docs/                        protocol.md, threat-model.md
```

## Prerequisites

- Rust **1.89.0** (pinned by [`rust-toolchain.toml`](rust-toolchain.toml), installed via rustup)
- Solana CLI **3.1.x** (Agave)
- Anchor CLI **1.1.2**
- Node.js 26+ (for the TS workspaces; not needed until Phase 10)

## Build and test

```bash
# Build everything
cargo build --workspace

# Run unit/integration tests (golden vectors + LiteSVM program tests)
cargo test --workspace

# Lint
cargo clippy --workspace --all-targets -- -W clippy::all -W clippy::pedantic

# Build the onchain program + IDL
anchor build

# Full local test run (anchor build + cargo test + e2e placeholder)
bash tests/run.sh
```

## Program ID

The canonical program ID is **`4smCAEoycSXSvVsyic8ircQmHmENPCHn17Fma83SYVbf`**,
and `declare_id!`, `[programs.localnet]`, and `[programs.devnet]` all use it.

The matching keypair lives at `target/deploy/forge_repository-keypair.json` and
is **git-ignored** (never commit keypairs).

- **Building and testing need no keypair.** `anchor build` and the LiteSVM tests
  use `declare_id!`, so a fresh clone works as-is even though `target/` is empty.
- **Do not run `anchor keys sync`.** It regenerates a keypair and rewrites
  `declare_id!` / `Anchor.toml`, changing the program ID (this is how the ID
  drifted before). The ID must stay stable.
- **Deploying** requires the canonical keypair at the path above; obtain it
  out-of-band from the team and restore it before `anchor deploy`.

## Documentation

- [`phase_implementation.md`](phase_implementation.md) — 10-phase execution roadmap,
  dependency graph, requirement coverage matrix, risks, open questions.
- [`docs/architecture.md`](docs/architecture.md) — system architecture, onchain
  program map, push data flow, and Mermaid diagrams.
- [`docs/protocol.md`](docs/protocol.md) — canonical byte formats (frozen in Phase 2).
- [`docs/threat-model.md`](docs/threat-model.md) — security model checklist.
- [`docs/adr/`](docs/adr/) — architecture decision records (events, names/seeds,
  state layout/authorization, commit/Ed25519, branch refs, storage, local CLI,
  onchain CLI).

## MVP scope (spec §19.1)

Program: `initialize_repository`, `create_commit` (Ed25519 verify),
`create_branch`, `update_branch` (CAS via `head_seq`), getters, append-only
history root. CLI: `init/add/commit/push/clone/log/branch/checkout/verify/status`.
Storage: IPFS bundles with ≥2 pins. Identity: wallet-signed commit attestations.
Network: Solana devnet.
