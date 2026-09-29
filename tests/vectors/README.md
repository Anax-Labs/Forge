# Golden vectors

Cross-implementation determinism fixtures for the Forge object/hashing engine.

**Populated (Phase 2).** [`golden.json`](golden.json) pins, for a fixed fixture
under both SHA-1 and SHA-256:

- input files (path, content hex, mode);
- blob oids and the root tree oid;
- the fixture commit oid;
- canonical attestation CBOR bytes and `attestation_hash`;
- `genesis_history_root`, `history_root`, and `repo_root`.

A mismatch here means the protocol is not reproducible and must not be merged.

## How they are verified

- `crates/forge-object/tests/golden.rs` asserts `golden.json` equals the values
  recomputed by `forge-object`.
- `crates/forge-object/tests/git_compat.rs` independently reproduces blobs,
  trees, and the commit with the real `git` binary (`git hash-object`,
  `git mktree`, `git commit-tree`) and asserts byte-identical ids.
- `crates/forge-object/src/*.rs` unit tests cover tree-ordering edge cases
  (`foo` vs `foo/` vs `foo.txt`), path safety/NFC, canonical-CBOR rejection, and
  hash domains.

## Regenerating

Only as a deliberate protocol change, then review the diff:

```bash
cargo run -p forge-object --example gen_vectors > tests/vectors/golden.json
```
