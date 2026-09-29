# End-to-end tests

Full-protocol flows:

- Offline local workflow (`init → add → commit → log → verify`) — Phase 7.
- Onchain workflow (`init → add → commit → push → clone → log → verify`) against
  a local validator — Phase 8.
- Negative cases: forged commit rejected, stale push rejected, history rewrite
  visible — Phases 8/10.

Status: **placeholder** until Phase 7/8. `run.sh` is invoked by `../run.sh` and
by CI; it currently reports that no e2e suite exists yet.
