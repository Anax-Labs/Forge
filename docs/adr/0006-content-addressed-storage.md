# ADR 0006 — Content-addressed storage (IPFS hot / Arweave cold)

- **Status:** Accepted (Phase 6)
- **Spec:** §8.2 (limits), §8.3 (hybrid), §13 (`gc --verify-availability`),
  §14.1 (`.forge/storage-index`), §24 (storage consistency), §25 #11 (hints).
- **Related:** ADR 0002/0003 (`storage_backend` onchain tag), Phase 2
  (`forge-object` is the only OID engine).
- **Resolves open question:** #4 (storage providers).

## Context

File bytes never go onchain. History is provable from Git OIDs and
`history_root`, but retrieval needs an offchain CAS. IPFS is fast and cheap
with no permanence; Arweave is pay-once and better for tags/checkpoints, with
a reported ~10 MiB per-transaction ceiling. The protocol must not trust a CID,
TXID, or path.

## Decision

### Address vs locator

- **Address:** Git OID from `forge-object` (`oid(type, payload)`).
- **Locator:** CIDv1-raw-sha256, Arweave/Irys TXID, or a filename under
  `.forge/cas`. Locators are optional hints. Every fetch recomputes the Git
  OID and rejects mismatches (`StorageError::OidMismatch`).

For SHA-256 repositories the Git OID digest equals the CIDv1 raw SHA-256
multihash of the **Git-framed** bytes. That equality is a convenience, not a
trust assumption: SHA-1 interop objects still verify via SHA-1 OIDs while
IPFS locators remain SHA-256 CIDs.

### Bundle format

Missing objects are packed as **IPLD CARv1**. Each block is Git-framed
(`<type> <len>\0<payload>`). Block CIDs are re-hashed on decode. The CAR root
is the CID of the tip object's framed bytes. A Git bundle is not required for
MVP; CAR is the interchange unit `forge push` will upload in Phase 8.

### Providers (open question #4)

| Role | Implementation | Config |
|---|---|---|
| Hot pin 1 | Kubo HTTP RPC (`/api/v0/add`, `/api/v0/cat`) | `FORGE_IPFS_API` (default `http://127.0.0.1:5001`) |
| Hot pin 2 | Second Kubo or pinning-service origin | `FORGE_IPFS_API_2` |
| Cold | Irys-compatible bundler `POST /tx`, `GET /{id}` | `FORGE_IRYS_URL` |
| Tests / local | `MemoryBackend` and `.forge/cas` (`FsBackend`) | path only |

A `MultiPin` wrapper **requires ≥2 backends** on upload. Fetch tries each
locator until one serves bytes that hash-verify. Killing a single pin must
not make a push irretrievable.

### Arweave aggregation

Items larger than 9_500_000 bytes (under the ~10 MiB ceiling) are split,
uploaded as separate bundler transactions, and referenced by a JSON
manifest `{v, chunks, sha256}`. On read the payload SHA-256 is checked.
Live fees are not paid in unit tests; HTTP is mocked.

### `.forge/storage-index`

JSON v1 map of tagged oid → `{object_type, locators[]}`. The file is a cache.
`forge gc --verify-availability` walks it, fetches each locator, and reports
`available` (≥2 verified pins), `degraded` (1), `missing` (0), or `corrupt`.

### Onchain

Unchanged: `RepositoryAccount.storage_backend` is a tag (`0` IPFS, `1`
Arweave, `2` hybrid). Optional hint hashes may be added later without
trusting them (see `hint_hash`).

## Consequences

- `crates/forge-storage` is the only storage implementation; the CLI wraps it.
- Phase 8 `push` must call `upload_bundle` **before** `create_commit`.
- Phase 9 `forge tag` reuses `ArweaveBundler` for checkpoints.
- Alternate CAS mirrors implement `StorageBackend`; no IPFS-specific types
  leak into verify-on-read.
