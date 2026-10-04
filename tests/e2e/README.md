# End-to-end tests

Full-protocol flows:

- Storage consistency (CAR round-trip, corruption, killed-pin, `gc
  --verify-availability`) — Phase 6, implemented as
  `cargo test -p forge-storage` and `cli/tests/gc_verify.rs`.
- Offline local workflow (`init → add → commit → log → verify`) — Phase 7,
  implemented as `cli/tests/local_workflow.rs`.
- Onchain workflow (`init → add → commit → push → clone → log → verify`) —
  Phase 8, implemented as `cli/tests/onchain_workflow.rs` (LiteSVM).
- Negative cases: forged author, stale `update_branch`, corrupt CAS blob —
  Phase 8 (`onchain_workflow.rs`). History rewrite remains a Phase 5 program
  test (`reset_branch`).

Status: storage tests in `crates/forge-storage` (Phase 6). Local CLI tests in
`cli/tests/local_workflow.rs` (Phase 7). Onchain CLI tests in
`cli/tests/onchain_workflow.rs` (Phase 8). The shell e2e runner remains a
placeholder for a live validator. `run.sh` is invoked by `../run.sh` and by CI.
