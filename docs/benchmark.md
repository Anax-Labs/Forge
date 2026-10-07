# Cost & Compute Benchmark

> Phase 10 artifact. Figures are **measured** where marked and otherwise
> **arithmetic on cited constants** (§10). Rent is a refundable deposit, not a
> fee. Recompute against live network parameters before relying on them.

## Compute units (measured)

Measured with the LiteSVM program tests (`programs/forge_repository/tests/`),
which include the native Ed25519 precompile in the transaction:

| Instruction | CU (incl. Ed25519) | Notes |
|---|---:|---|
| `create_commit` | **32,764** | asserted `< 200_000` in `create_commit_compute_units_within_default_limit` |
| `initialize_repository` | ~12k | from LiteSVM logs (two account inits + SHA-256 genesis root) |
| `update_branch` | ~12k | CAS + Ed25519 introspection |

All are far below the **200k** per-instruction default (and the 1.4M cap), so no
`SetComputeUnitLimit` is required for the MVP. Reproduce:

```bash
cargo test -p forge_repository create_commit_compute_units_within_default_limit -- --nocapture
```

## Account rent (arithmetic, §10)

`min_balance = (128 + data_size) × lamports_per_byte`, at the current mainnet
**5,080 lamports/byte** (SIMD-0437 phase 2) and the target **696** (Agave 4.4):

| Account | Size (bytes) | Rent @5,080 (SOL) | Rent @696 (SOL) |
|---|---:|---:|---:|
| Repository | 256 (8 + 248) | 0.001966 | 0.000269 |
| Branch | 187 (8 + 179) | 0.001437 | 0.000197 |
| Commit | 282 (8 + 274) | 0.002167 | 0.000297 |
| Tag | 178 (8 + 170) | 0.001368 | 0.000187 |
| Permission | 90 (8 + 82) | 0.000692 | 0.000095 |
| Provenance | 210 (8 + 202) | 0.001614 | 0.000221 |

Repository creation ≈ repository + branch ≈ **0.0034 SOL** now (~$0.34 at
SOL ≈ $100, **uncertain**), fully recoverable on close.

## Per-commit cost (MVP, per-commit `CommitAccount`)

| Commits | Rent @5,080 (SOL) | @5,080 USD* | Rent @696 (SOL) | @696 USD* |
|---:|---:|---:|---:|---:|
| 100 | 0.2167 | ~$21.67 | 0.0297 | ~$2.97 |
| 1,000 | 2.167 | ~$216.7 | 0.297 | ~$29.7 |
| 10,000 | 21.67 | ~$2,167 | 2.97 | ~$297 |

\* SOL ≈ $100 assumption from §10 (uncertain). Plus ~10,000 lamports/tx in base
fees (~$0.001), negligible next to rent.

## Storage (offchain)

Object bytes live in IPFS/Arweave; the chain stores only hashes, so per-commit
onchain cost is **independent of file size**. Availability, not size, is the
scaling constraint (§8.3).

## Scaling path (§10.4)

1. **MVP:** per-commit `CommitAccount` (this benchmark).
2. **Step 1:** emit `CommitCreated` events + indexer; keep branch heads +
   `history_root` onchain; add periodic checkpoint roots.
3. **Step 2:** ZK-compressed commit accounts (~5,000 lamports each) or an
   append-only Merkle log for O(log n) inclusion proofs.

## Notes / caveats

- CU figures include the Ed25519 precompile and vary slightly with account count
  and RPC. They are recorded from LiteSVM, not a live validator.
- `lamports_per_byte` is a live parameter; re-verify before deployment.
- Rent is refunded when accounts close (e.g. `delete_branch`).
