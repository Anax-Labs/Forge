# Forge

**Onchain version control on Solana.** Git-like repositories whose history,
authorship, branch state, and source-code provenance are cryptographically
anchored on Solana, while file contents live in content-addressed offchain
storage.

Working name: `Forge` (protocol), `forge` (CLI).

> Status: **Phase 3 — onchain state model, PDAs & repository/branch lifecycle.**
> The full account state model (§4), PDA derivations, and the
> `initialize_repository` / `create_branch` instructions are implemented with
> owner-only authorization and `emit_cpi!` events. Commit creation and later
> phases are pending. See [`phase_implementation.md`](phase_implementation.md)
> for the 10-phase roadmap,
> [`docs/adr/`](docs/adr/) for frozen design decisions, and
> [`ONCHAIN_VERSION_CONTROL_ARCHITECTURE.md`](ONCHAIN_VERSION_CONTROL_ARCHITECTURE.md)
> for the full architecture specification.

## Repository layout

```
programs/forge_repository/   Anchor program (accounts, instructions, events)
crates/forge-object/         Canonical Git-object/hashing/attestation engine (Phase 2)
cli/                         Rust `forge` CLI (gix-backed, built on Git)
sdk/                         TypeScript SDK (@solana/kit + Codama client) (Phase 10)
web/                         Next.js explorer / verify UI (Phase 10)
indexer/                     Optional Helius webhook indexer, non-authoritative (Phase 10)
tests/vectors/               Golden Git OID / attestation vectors (Phase 2)
tests/e2e/                   End-to-end flows (Phases 7–10)
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

`Anchor.toml` and `declare_id!` are set to the localnet program ID
`4smCAEoycSXSvVsyic8ircQmHmENPCHn17Fma83SYVbf`. The matching keypair lives at
`target/deploy/forge_repository-keypair.json` and is **git-ignored** (never
commit keypairs).

On a fresh clone where `target/` is empty, run:

```bash
anchor keys sync   # regenerates the keypair and updates Anchor.toml + declare_id!
```

The devnet program ID is a placeholder until the program is first deployed
(Phase 8); once deployed, update `[programs.devnet]` in `Anchor.toml`.

## Documentation

- [`phase_implementation.md`](phase_implementation.md) — 10-phase execution roadmap,
  dependency graph, requirement coverage matrix, risks, open questions.
- [`docs/protocol.md`](docs/protocol.md) — canonical byte formats (frozen in Phase 2).
- [`docs/threat-model.md`](docs/threat-model.md) — security model checklist.
- [`docs/adr/`](docs/adr/) — architecture decision records (events, names/seeds,
  state layout & authorization).

## MVP scope (spec §19.1)

Program: `initialize_repository`, `create_commit` (Ed25519 verify),
`create_branch`, `update_branch` (CAS via `head_seq`), getters, append-only
history root. CLI: `init/add/commit/push/clone/log/branch/checkout/verify/status`.
Storage: IPFS bundles with ≥2 pins. Identity: wallet-signed commit attestations.
Network: Solana devnet.
