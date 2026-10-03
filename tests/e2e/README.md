# End-to-end tests

Full-protocol flows:

- Storage consistency (CAR round-trip, corruption, killed-pin, `gc
  --verify-availability`) — Phase 6, implemented as
  `cargo test -p forge-storage` and `cli/tests/gc_verify.rs`.
- Offline local workflow (`init → add → commit → log → verify`) — Phase 7.
- Onchain workflow (`init → add → commit → push → clone → log → verify`) against
  a local validator — Phase 8.
- Negative cases: forged commit rejected, stale push rejected, history rewrite
  visible — Phases 8/10.

Status: storage unit/integration tests live in `crates/forge-storage` (Phase 6).
The shell e2e runner remains a placeholder until Phases 7/8. `run.sh` is
invoked by `../run.sh` and by CI.
