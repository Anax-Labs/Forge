# Forge Threat Model

> Placeholder. Tracked against §11 of the architecture spec; mitigations are
> marked MVP/Future there.

## Cross-cutting program checklist (§11)

For every instruction: validate owner, signer, writable; PDA seeds + canonical
bump; no arbitrary CPI; checked math; duplicate-mutable-account guard; secure
close; treat `remaining_accounts` as untrusted; test each error path.

## Explicit MVP non-goals (§11, §19.4)

Confidentiality / private repos, anonymity, storage-provider economic
incentives, fully trustless onchain rebuild of every historical commit,
custom merge engine, general rule VM, onchain file storage.

## Open threats tracked for implementation

- Forged commits → onchain Ed25519 verification in `create_commit` (Phase 4, `programs/forge_repository/src/ed25519.rs`).
- Unauthorized branch update / replay → authority + `head_seq` CAS (Phases 4–5).
- History rewrite → append-only `history_root` + logged reset (Phases 4–5).
- Storage disappearance → multi-pin + Arweave + `gc --verify-availability` (Phase 6, implemented).
- Path traversal / symlink payloads → safe tree construction (Phase 2).
- Malicious metadata → length/charset limits, no log parsing (Phases 3, 9).

## Phase 3 status (implemented)

- **Malicious metadata:** repository/branch names are validated as non-empty
  printable ASCII with NUL-only padding (`src/name.rs`, ADR 0002); bounded
  32-byte buffers are stored onchain.
- **Unauthorized writes:** `initialize_repository` and `create_branch` enforce
  owner-only authorization (`src/auth.rs`, §16.4).
- **PDA confusion / squatting:** every `init` account uses explicit seeds with
  the canonical bump, and the canonical bump is stored in account data
  (`src/pda.rs`, §11). `create_branch` validates any `from_commit` account's
  program owner, discriminator, PDA, repository binding, and oid.
- **Log injection:** events are emitted via `emit_cpi!` call data, never parsed
  from string logs (ADR 0001, §11 #12).
- **Reinitialization / revival:** PDA accounts are created through the guarded
  `create_pda` helper, which rejects a non-empty account before creating it, and
  addresses are pinned by `seeds` + `bump`. `init_if_needed` is forbidden.
  Duplicate repository/branch creation returns `RepositoryAlreadyExists` /
  `BranchAlreadyExists`.
- **Overflow:** no arithmetic existed in Phase 3; Phase 4 uses `checked_add`
  (`commit_count + 1`, Ed25519 offset arithmetic) returning `MathOverflow`.

## Phase 4 status (implemented)

- **Forged commits:** `create_commit` verifies the author's Ed25519 signature over
  `attestation_hash` via Instructions-sysvar introspection (`src/ed25519.rs`,
  ADR 0004). Wrong pubkey/message → `BadSignature`; a valid signature over the
  wrong message, a forged-author signature, and a non-owner author are all
  rejected (tests: `create_commit_rejects_forged_author_signature`,
  `create_commit_rejects_unauthorized_author`).
- **Multiple / malformed Ed25519 instructions:** exactly one Ed25519 instruction
  is required in the transaction; header, indices, offsets, and non-empty message
  are validated. Parser edge cases are covered by unit tests (the native program
  rejects malformed instructions before they reach the program).
- **Replay / duplicates:** a second `create_commit` for the same
  `(repo, commit_oid)` fails at Anchor `init` (idempotent by construction, §6.7);
  `attestation_hash` binds `repo` and `commit`, preventing cross-repo replay.
- **Invalid parents:** `parent_count ∈ {0,1,2}`, zero-oid rejection, self-parent,
  missing/unknown parent account, and root-on-nonempty are rejected with distinct
  codes (`InvalidParentCount`, `InvalidCommitOid`, `SelfParent`, `InvalidParent`,
  `UnknownCommit`, `InvalidPda`, `RootOnNonemptyRepo`).
- **Compute budget:** `create_commit` ≈ 32.7k CU including the precompile,
  asserted `< 200_000` in `create_commit_compute_units_within_default_limit`.

## Phase 5 status (implemented)

- **Unauthorized branch update:** `update_branch` / `reset_branch` / `merge`
  require the repository owner and a wallet signature over a domain-separated
  branch message (`src/refs.rs`, `forge_object::branch`, ADR 0005). Non-owner →
  `Unauthorized`; wrong signature → `BadSignature`.
- **Replay:** every head move is a compare-and-swap on `head_seq`; a replayed or
  racing update fails with `StaleBranchHead` (§7.4). Update and reset use
  different domains so an update signature cannot authorize a rewrite.
- **History rewrite:** `reset_branch` is a distinct, logged instruction that
  emits `BranchReset` and never modifies the append-only `history_root`; a test
  asserts the root is unchanged (§11 #4/#9).
- **Malicious merge:** a merge commit must have exactly two parents matching the
  current target and source heads, or it is rejected (`InvalidMerge`).
- **Accidental ref deletion:** deleting the default branch is forbidden
  (`CannotDeleteDefaultBranch`); deleting another branch closes its account and
  emits `BranchDeleted`.

## Phase 6 status (implemented)

- **Untrusted locators:** CIDs, Arweave TXIDs, and `.forge/cas` paths are
  hints only (`crates/forge-storage`, ADR 0006). `fetch_object` /
  `GitObject::verify_framed` recompute the Git OID via `forge-object` and
  reject `OidMismatch`. CAR block CIDs are re-hashed independently.
- **Storage disappearance:** `MultiPin` requires ≥2 backends on upload; tests
  kill one pin and still fetch. `forge gc --verify-availability` reports
  `available` / `degraded` / `missing` / `corrupt` from `.forge/storage-index`.
- **Corrupt bytes:** flipped CAR/object bytes fail CID or OID verification
  (`tests/storage_consistency.rs`). A matching locator string is never enough.
- **Arweave size limit:** payloads over 9_500_000 bytes are chunked through an
  Irys-compatible bundler; the manifest SHA-256 is checked on read (mock HTTP
  in tests; no live fees).
- **Path traversal in local CAS:** `FsBackend` rejects locator ids containing
  `/`, `\\`, or `..`.

## Phase 7 status (implemented)

- **Forged / unsigned local commits:** `forge commit` writes a canonical
  attestation sidecar plus a detached Ed25519 signature (ADR 0007).
  `forge verify` recomputes Git OIDs via `forge-object`, re-hashes the CBOR
  file, and checks the signature. Tampered CBOR fails (exit 1). Missing
  sidecar/HEAD is exit 3. History inclusion is Phase 8.
- **Path traversal / symlink payloads:** `forge add` and commit-time index
  walks reject `..`, absolute paths, and unsafe tree names (`path::sanitize_name`).
- **Wallet identity:** authorship is the Solana JSON keypair
  (`FORGE_WALLET` or `.forge/id.json`, gitignored). Git `author`/`committer`
  strings are display-only (`Forge <pubkey@forge>`).
- **No network:** Phase 7 commands do not contact Solana or storage providers.

## Phase 8 status (implemented)

- **Forged author:** `create_commit` still requires the repository owner
  (MVP). Wallet B cannot push to wallet A's repo (`Unauthorized` / failed tx).
- **Stale push:** `update_branch` CAS on `head_seq`; a replayed expected seq
  fails (`StaleBranchHead` 6015). The CLI tells the user to `forge pull`.
- **Untrusted CAS:** clone fetches a CAR and re-hashes every Git OID.
  Flipped pin bytes fail CID/OID checks (clone abort). `forge verify` walks
  onchain commits and recomputes `history_root` with `forge-object`.
- **No indexer:** PDAs are derived client-side; account bytes are parsed
  against frozen layouts (ADR 0008).

## Phase 9 status (implemented — program + CLI)

- **Tag forgery:** `create_tag` verifies the tagger's Ed25519 signature over a
  domain-separated tag message; the `TagAccount` is `init`-only, so a name cannot
  be overwritten (`TagCreated.signed = 1`).
- **Privilege escalation:** roles are enforced by `refs::require_min_role` —
  owner always, otherwise an unexpired `PermissionAccount` with sufficient role.
  Non-owners without a permission account → `Unauthorized`; insufficient role →
  `InsufficientRole` (`allowlist_writer_can_commit_but_reader_cannot`).
- **Ownership hijack:** `transfer_repository` requires the current owner's
  signature. The PDA is stable across transfer (clients use `repo_id`).
- **Fake provenance:** `anchor_program_source` requires the commit to already be
  in the repository history and the attested account to be a loader-owned
  executable program; the record is a claim (`verified = 0`), not a proof.
- **Unbounded roles:** `update_permissions` rejects `role > admin` (`InvalidRole`)
  and never uses `init_if_needed`.

## Phase 10 status (partial)

Phase 10 adds **client-side interfaces only** (TypeScript SDK core, benchmark,
demo docs) and introduces **no new onchain trust assumptions**. The SDK
re-verifies every hashed value against the frozen `forge-object` encodings
(proven by cross-language golden-vector tests), consistent with the "never trust
offchain bytes" rule. The explorer UI and indexer (Phase 10b) are read-only
caches and must remain non-authoritative.
