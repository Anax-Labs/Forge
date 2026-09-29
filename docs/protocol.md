# Forge Protocol — Canonical Encodings

> **Status:** frozen in Phase 2. The authoritative implementation is
> [`crates/forge-object`](../crates/forge-object), and the checked-in golden
> vectors are [`tests/vectors/golden.json`](../tests/vectors/golden.json).
> Changing anything on this page is a deliberate protocol change and requires
> regenerating the vectors and updating the onchain/CLI consumers.
>
> Source of truth: [`ONCHAIN_VERSION_CONTROL_ARCHITECTURE.md`](../ONCHAIN_VERSION_CONTROL_ARCHITECTURE.md)
> §4 (data model), §5 (hashing & Merkle model), §6 (commit model).

Determinism is a protocol requirement: two independent implementations must
produce byte-identical ids. `forge-object` is the single source of truth for all
canonical bytes; the Anchor program and CLI must never re-implement this logic.

## 1. Object framing and object ids

```
oid(type, payload) = H( "<type> " + decimal_byte_length(payload) + "\0" + payload )
```

- `H` is SHA-256 for Forge-native repositories, SHA-1 for Git-interop reads.
- The type string (`blob` / `tree` / `commit` / `tag`) is part of the hashed
  header, giving inherent domain separation.

**Hash algorithm tagging.** Strings carry an algorithm prefix: `sha256:<hex>` or
`sha1:<hex>`. `HashAlgorithm::tag()` returns `sha256` / `sha1`.

**32-byte canonical oid form** (for onchain `[u8; 32]` fields):

```
SHA-256 -> digest[0..32]
SHA-1   -> 0x01 || digest[0..20] || 0x00 x 11
```

`0x01` is the SHA-1 version tag; `0x00 x 11` pads to 32 bytes. (Resolves the
"pad SHA-1 to 32 with a version tag" notation in §4.)

## 2. Blob

```
blob_oid = oid("blob", raw_file_bytes)
```

Identical bytes anywhere produce the same oid, so deduplication is automatic.

## 3. Tree

Entry serialization:

```
mode_ascii + 0x20 + name_bytes + 0x00 + oid_raw_bytes
```

`oid_raw_bytes` is the raw digest (`20` bytes SHA-1, `32` bytes SHA-256).

Entry sort key:

```
key(name, is_tree) = name_bytes + (is_tree ? b"/" : b"")
sort by key in unsigned byte order
```

This reproduces Git exactly, including the important edge cases:

| Names | Order | Reason |
|---|---|---|
| `foo` (file) vs `foo/` (tree) | `foo` first | `\0` (0x00) < `/` (0x2F) |
| `foo.txt` vs `foo/` (tree) | `foo.txt` first | `.` (0x2E) < `/` (0x2F) |
| `foo/` (tree) vs `foo0` | `foo/` first | `/` (0x2F) < `0` (0x30) |

**Modes:** `100644` regular, `100755` executable, `120000` symlink, `40000`
subtree, `160000` gitlink. `040000` is accepted on read and normalized.

**Path safety (documented deviation from Git, §5.3/§11 #14):** entry names must
be valid UTF-8, NFC-normalized, non-empty, and must not be `.`, `..`, contain
`/` or NUL, or be absolute. Names are normalized to NFC before hashing so
implementations agree. Symlink targets are blob content and are never resolved;
submodules are commit links only (no URL).

## 4. Commit

```
tree <hex-oid>\n
parent <hex-oid>\n        (zero or more, first-parent order)
author <identity> <unix-ts> <tz>\n
committer <identity> <unix-ts> <tz>\n
\n
<message bytes>
```

- `<hex-oid>` is untagged lowercase hex (40 chars SHA-1, 64 chars SHA-256).
- `<identity>` is `Name <email>`; name/email reject `<`, `>`, NUL, and line
  breaks.
- `<tz>` is `+HHMM` / `-HHMM` (e.g. `+0000`, `-0500`).
- Message is arbitrary bytes, LF conventions only.
- Author/committer strings are display/interop only; **authorship is the wallet
  signature over the commit id** (see attestation).

## 5. Attestation (what the wallet signs)

The signature cannot live inside the commit object without changing its id, so
it travels in a sidecar attestation, serialized as **canonical CBOR**
(RFC 8949 §4.2.1 core deterministic encoding: shortest forms, definite lengths,
map keys sorted bytewise-lexicographically by their encoded bytes).

Schema (`v = 1`):

```
{
  "v": 1,
  "repo": "<base58 repo PDA>",
  "commit": "<sha256|sha1:hex>",
  "parents": ["<sha256|sha1:hex>", ...],   // first-parent order
  "tree": "<sha256|sha1:hex>",
  "author": "<base58 wallet pubkey>",
  "authoredAt": <int>,
  "messageHash": "<sha256|sha1:hex>",
  "nonce": "<base58 random 16 bytes>"
}
```

```
attestation_hash = H( "forge-attestation\0" || canonical_cbor(attestation) )
signature        = Ed25519_sign(wallet_sk, attestation_hash)
```

The sidecar file `.forge/attestations/<commit_oid>.cbor` contains exactly
`canonical_cbor(attestation)`, so verification is
`H("forge-attestation\0" || file_bytes)`.

Decoding is strict: unknown/duplicate/missing fields, trailing bytes,
non-minimal length encodings, and indefinite-length items are all rejected, and
the bytes are re-encoded and compared to guarantee canonical form.

## 6. Repository history root and repo root

```
history_root_0 = H( "forge-genesis\0" || repo_pda )
history_root_n = H( "forge-append\0" || history_root_{n-1} || commit_oid_n || seq_n )
repo_root      = H( "forge-repo\0" || repo_pda || default_branch_head || history_root || commit_count )
```

- `repo_pda` is a 32-byte Solana address.
- Oids are folded in through their canonical 32-byte form (§1).
- `seq_n` and `commit_count` are **little-endian `u64`**.
- `default_branch_head` may be the all-zero oid for an empty repository.
- Domains (`forge-genesis` / `forge-append` / `forge-repo`) prevent
  cross-context reuse.

`history_root` is appended on every accepted `create_commit`, never on branch
updates, so previously seen commits can never be erased even by a force-push.

## 7. Golden vectors and cross-compatibility

- [`tests/vectors/golden.json`](../tests/vectors/golden.json) pins the expected
  oids (blobs, trees, commit), attestation CBOR and hash, history root, and repo
  root for a fixed fixture under both SHA-1 and SHA-256.
- `crates/forge-object/tests/git_compat.rs` independently reproduces the same
  blobs, trees, and commit with the real `git` binary (`git hash-object`,
  `git mktree`, `git commit-tree`) and asserts byte-identical ids for both
  object formats.
- `crates/forge-object/src/*.rs` unit tests cover tree-ordering edge cases,
  path safety/NFC, canonical-CBOR rejection, and hash domains.

Regenerate vectors (only as a deliberate protocol change):

```bash
cargo run -p forge-object --example gen_vectors > tests/vectors/golden.json
```

Run all engine checks:

```bash
cargo test -p forge-object
```
