# Golden vectors

Cross-implementation determinism fixtures for the Forge object/hashing engine.

Populated in **Phase 2** (see `../../phase_implementation.md`). Each vector pins:

- input files (bytes) and expected Git blob/tree/commit OIDs, checked against
  `git hash-object` / `git cat-file`;
- tree-ordering edge cases (`foo` vs `foo/` vs `foo.txt`);
- canonical attestation bytes and `attestation_hash`;
- `history_root` chain values and `repo_root`.

A mismatch here means the protocol is not reproducible and must not be merged.
