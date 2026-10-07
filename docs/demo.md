# Forge Demo Script

> The §19.1 MVP demo: **verifiable authorship + anchored history + program→commit
> provenance**, with negative cases. Each step maps to an automated test, so the
> demo is reproducible, not a one-off.

## Prerequisites

- `anchor build` (produces `target/deploy/forge_repository.so`).
- Two wallets: A (owner/author) and B (reviewer/cloner).
- Storage configured (IPFS pinning) or the in-repo test CAS.
- RPC: `FORGE_RPC` (local validator/Surfpool) or devnet via Helius.

## Narrative (§18.4)

```text
A$ forge init
A$ forge add .
A$ forge commit -m "Initial protocol"      # builds objects, signs attestation
A$ forge push                              # upload bundle; create_commit; update_branch
   ✓ uploaded objects (≥2 pins)
   ✓ create_commit <oid>  signed by A
   ✓ update_branch main (seq 0 → 1)

B$ forge clone <repo-pda> forge
B$ forge verify                            # recompute OIDs, check A's signature,
                                           # walk history_root inclusion

A$ forge branch feature/x
A$ forge merge feature/x                   # git merge --no-ff, then push
A$ forge tag v1.0.0 --checkpoint           # signed create_tag + release checkpoint

A$ forge anchor-program <commit> <program-id> ...   # provenance claim
B$ forge verify-program <program-id>       # VERIFIED / MISMATCH / UNVERIFIED_CLAIM
```

## Negative cases (the security story)

| Scenario | Expected | Proven by |
|---|---|---|
| Forged commit (B signs, claims A) | rejected (`BadSignature`) | `programs/forge_repository/tests/phase4.rs::create_commit_rejects_forged_author_signature` |
| Unauthorized author | rejected (`Unauthorized`) | `phase4::create_commit_rejects_unauthorized_author` |
| Duplicate commit | rejected at init | `phase4::create_commit_rejects_duplicate_commit` |
| Stale push (CAS lost) | `StaleBranchHead`; retry after `forge pull` | `phase5::update_branch_rejects_stale_head_and_replay` |
| Non-fast-forward without reset | rejected (`NonFastForward`) | `phase5::update_branch_rejects_non_fast_forward` |
| History rewrite | visible (`BranchReset`), `history_root` unchanged | `phase5::reset_branch_allows_non_fast_forward_and_preserves_history` |
| Allowlist: reader cannot commit | rejected (`InsufficientRole`) | `phase9::allowlist_writer_can_commit_but_reader_cannot` |
| Fake provenance (bad program) | rejected (`ProgramNotUpgradeable`) | `phase9::anchor_program_source_rejects_non_upgradeable_program` |

## CLI end-to-end (no external services)

`cli/tests/onchain_workflow.rs` and `cli/tests/phase9_cli.rs` run the whole flow
against LiteSVM + the in-repo CAS, including `push → clone → verify` with
`history_root` recomputation, `permissions`, `tag`, and `verify-program`.

## `forge verify-program` statuses / exit codes (§12.4)

| Output | Exit | Meaning |
|---|---:|---|
| `VERIFIED` | 0 | claim present, commit anchored, rebuilt artifact hash matches |
| `MISMATCH` | 1 | claim present but commit unanchored or artifact hash differs |
| `UNVERIFIED_CLAIM` | 2 | claim present, no artifact to compare (set `FORGE_VERIFY_SO`) |
| (missing) | 3 | no provenance claim for the program |

## Run everything

```bash
anchor build
cargo test --workspace
node --test 'sdk/test/**/*.test.ts'   # TS SDK reproduces the Rust golden vectors
```
