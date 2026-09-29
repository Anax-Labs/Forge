# Forge Protocol Notes

> Canonical byte formats, PDA derivations, and invariants. Placeholder until
> Phase 2 freezes the encoding.

## Status

Phase 1 scaffold. The authoritative source is
[`../ONCHAIN_VERSION_CONTROL_ARCHITECTURE.md`](../ONCHAIN_VERSION_CONTROL_ARCHITECTURE.md)
(§5 hashing & Merkle model, §4 data model, §6 commit model, §7 branches).

## To be frozen in Phase 2

- Object ID algorithm: SHA-256 default, SHA-1 read-only interop (§5.1).
- Tree entry ordering (name + trailing `/` for subtrees) (§5.3).
- Path safety rules: no `/`, NUL, `.`, `..`, absolute paths; NFC normalize (§5.3).
- Commit serialization and canonical timezone/encoding (§5.4).
- Attestation encoding choice (canonical CBOR vs. JCS) and `attestation_hash`
  domain separation (§5.5).
- `history_root` genesis/append, including `seq` byte order (§5.6).
- `repo_root` composition (§5.6).
