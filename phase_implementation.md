# Onchain Version Control — Implementation Roadmap (10 Phases)

> Derived from `ONCHAIN_VERSION_CONTROL_ARCHITECTURE.md`. This is a planning artifact, not an implementation.
> The spec's own MVP tiering (§19) drives the phase order: canonical protocol primitives → onchain core → storage → CLI → provenance → interfaces/demo.
> Ten phases, each ending in a runnable, testable state.

---

## Phase 1 — Foundation, Scaffolding & Toolchain Baseline

### Goal
Stand up the `onchain-forge/` monorepo skeleton from §22, pin the toolchain from §21, and establish the build/test/lint commands that every later phase depends on.

### Scope
- Monorepo layout (`programs/forge_repository`, `cli/`, `sdk/`, `web/`, `indexer/`, `tests/`, `docs/`).
- Anchor project initialized with program name `forge_repository`, `Anchor.toml`, workspace `Cargo.toml`.
- Rust workspace for CLI (crate `forge`).
- Devnet configuration, keypair handling conventions, environment variable names.
- CI skeleton that runs `cargo build`, `cargo clippy`, `anchor build` on scaffolding.
- Golden-vector test directory placeholder and test harness wiring.

### Tasks
1. Create the directory tree from §22 with empty but compiling modules; add a root `README.md` (allowed here: it is part of the specified repo structure).
2. Initialize `Anchor.toml` targeting localnet + devnet clusters; set `[programs.devnet]` and `[provider]` placeholders.
3. Initialize the `forge_repository` Anchor program crate (`anchor init`-equivalent) with `lib.rs`, `state/`, `instructions/`, `errors.rs`, `events.rs` module stubs.
4. Initialize the Rust CLI crate with `clap`, `gix` (or `git2`), `solana-sdk`, `reqwest` dependencies per §21; wire a `forge --version` no-op command.
5. Add a `.gitignore` covering `target/`, `node_modules/`, `.anchor/`, keypairs, and `.forge/` fixtures.
6. Configure CI (GitHub Actions or repo-native) to run `cargo test --workspace`, `cargo clippy -- -W clippy::all -W clippy::pedantic`, and `anchor build` (per §24 CI).
7. Document local validator / Surfpool startup and the devnet program-ID capture step.
8. Add placeholder `tests/vectors/` and `tests/e2e/` directories with a runner script.

### Architecture / Technical Decisions
- **Anchor 1.1.x** is the program framework (§9, §21). Pin exact minor version in `Cargo.toml`/`Anchor.toml`.
- **Rust CLI** over TypeScript for object access (§21), with `gix` preferred over `git2`; final choice recorded (see Open Questions).
- Keep the core hashing logic in a separate library crate (e.g. `crates/forge-object`) so both CLI and program-side tests can depend on it in Phase 2.
- Program ID for localnet is fixed/constant; devnet ID captured in config, not hardcoded, to avoid churn.

### Dependencies
None. This is the root phase.

### Files / Modules Likely Affected
`Anchor.toml`, root `Cargo.toml`, `programs/forge_repository/**`, `cli/Cargo.toml`, `cli/src/main.rs`, `sdk/`, `web/`, `indexer/`, `.github/workflows/*`, `tests/**`, `README.md`.

### Deliverables
- Compiling monorepo skeleton.
- `anchor build` produces an IDL/json for an empty program.
- `forge` binary builds and prints version.
- CI runs green on the skeleton.

### Validation
- `anchor build` exits 0.
- `cargo build --workspace && cargo clippy --workspace -- -W clippy::all -W clippy::pedantic` exits 0.
- `./target/debug/forge --version` prints the pinned version.
- CI pipeline executes on a trivial commit.

### Definition of Done
All validation commands pass locally and in CI; the directory structure matches §22; no unimplemented requirement has been silently dropped or renamed.

### Risks / Questions
- `gix` vs `git2` decision is load-bearing for later phases (see Open Questions).
- Anchor 1.1.x / Surfpool-default behavior may differ from older tooling; pin versions.
- Whether the team wants a Rust workspace-level lockfile shared with the Anchor program (Anchor manages its own toolchain).

---

## Phase 2 — Canonical Object, Hashing & Merkle Engine

### Goal
Implement the deterministic, cross-implementable serialization layer from §5: Git-compatible blob/tree/commit OIDs, SHA-256-native (read SHA-1), tree ordering, path safety, canonical Forge attestation, `history_root`, and `repo_root` — proven against golden vectors.

### Scope
- `forge-object` library crate: blob, tree, commit serialization and hashing.
- Tree entry sorting (name + trailing `/` for subtrees) per §5.3.
- Path validation and NFC normalization per §5.3 and §11 #14.
- Algorithm tagging (`sha256:` / `sha1:`) per §5.1.
- Canonical CBOR (or JCS JSON) attestation + `attestation_hash` per §5.5.
- `history_root` genesis + append function per §5.6.
- `repo_root` fingerprint per §5.6.
- Golden-vector test suite against `git hash-object` / `git cat-file`.

### Tasks
1. Implement `oid(object_type, payload)` with the `<type> <len>\0<payload>` header and SHA-256 default, SHA-1 interop mode (§5.1).
2. Implement blob OID (`blob` payload = raw bytes) (§5.2).
3. Implement tree serialization: `mode + b" " + name + b"\0" + raw_oid` entries, sorted by `name + (is_tree ? b"/" : b"")` in unsigned byte order (§5.3).
4. Implement path validation: reject `/`, NUL, `.`, `..`, absolute paths; apply NFC before hashing (§5.3).
5. Represent symlinks as mode `120000` (target stored as data, never resolved) and submodules as mode `160000` links only (§5.3).
6. Implement commit serialization: `tree`, `parent*`, `author`, `committer`, blank line, message; canonical ASCII identity / UTF-8 message / LF endings (§5.4).
7. Implement canonical attestation struct (v, repo, commit, parents, tree, author, authoredAt, messageHash, nonce) with deterministic encoding and `attestation_hash = H("forge-attestation\0" + canonical)` (§5.5).
8. Implement `history_root_0 = H("forge-genesis\0" + repo_pda)` and `history_root_n = H("forge-append\0" + prev + commit_oid + seq_le)` (§5.6).
9. Implement `repo_root` (§5.6).
10. Add golden-vector tests comparing blob/tree/commit OIDs to real `git hash-object` output; include `foo` vs `foo/` vs `foo.txt` ordering edge cases (§24 Unit, §5.7).
11. Add attestation determinism tests (map ordering, nonce handling) and path-safety rejection tests.
12. Add SHA-1 read-mode vector tests.

### Architecture / Technical Decisions
- **Single source of truth:** the `forge-object` crate owns all canonical bytes; the Anchor program must never re-implement serialization logic independently.
- Default new repos to **SHA-256**; SHA-1 is read-only interop (§5.1, §5.8).
- `seq` in `history_root` append is little-endian `u64`; document and test byte order explicitly (spec §5.6 shows `seq_n` without endianness).
- CBOR vs JCS for attestations must be chosen in this phase; pick one canonical form and freeze it (§5.5 offers either).

### Dependencies
Phase 1 scaffold and the `forge-object` crate location.

### Files / Modules Likely Affected
`crates/forge-object/src/{object,blob,tree,commit,attestation,history,path}.rs`, `tests/vectors/**`, `docs/protocol.md`.

### Deliverables
- A library crate that deterministically produces Git-identical OIDs and Forge attestation/history roots.
- Golden-vector test file checked into `tests/vectors/`.
- `docs/protocol.md` documenting the canonical encoding.

### Validation
- `cargo test -p forge-object` passes all golden and edge-case vectors.
- A test constructs the §5.7 worked example and asserts `B1,B2,T1,T2,C1,C2,history_root_2`.
- `git hash-object`/`git mktree` cross-check test passes for identical inputs.
- Path-safety tests reject traversal/absolute/NUL inputs.

### Definition of Done
Two independent code paths (our engine and the `git` CLI in tests) produce byte-identical IDs for all vectors; SHA-256 and SHA-1 modes both verified; serialization documented.

### Risks / Questions
- `gix` SHA-256 repository support may be incomplete; may require hand-rolling object hashing and only using gix for storage/packfiles (Open Questions).
- CBOR canonicalization library choice affects determinism; validate against a second implementation.
- Spec leaves `history_root` endianness and attestation map ordering to us — must be frozen here, not later.

---

## Phase 3 — Onchain State Model, PDAs & Repository/Branch Lifecycle

### Goal
Define the Anchor account structs and PDA derivations from §4, and implement `initialize_repository` and `create_branch` plus read-only getters, with owner-only authorization. End state: a repository and its default branch exist onchain with a genesis `history_root`.

### Scope
- State accounts: `RepositoryAccount`, `BranchAccount`, `CommitAccount` (struct only), `TagAccount` (struct only), `PermissionAccount` (struct only), `ProgramSourceAttestation` (struct only) — exact fields/sizes per §4.
- PDA seed derivations per §4: `repo`, `branch`, `commit`, `tag`, `perm`, `prog`.
- Errors, canonical-bump storage, account size/`_reserved` headroom.
- `initialize_repository` (§9.2) and `create_branch` (§9.2).
- Read-only getters / account deserialization.
- Events module and emission for created repo/branch (§9.1).

### Tasks
1. Define all state structs with exact fields and approximate sizes from §4.1–§4.7; store canonical bump; include `_reserved` headroom where specified.
2. Implement PDA helper functions for every seed set in §4.
3. Implement `initialize_repository(name, default_branch, storage_backend, flags)`: validate name, `init` repo PDA, create empty default branch, set `history_root = genesis`, `commit_count = 0` (§9.2).
4. Implement `create_branch(name, from_commit, authority)`: validate authorization, name uniqueness, `from_commit` exists or zero, `init` branch PDA, set `head`, `head_seq = 0`, `authority` (§9.2, §7.2).
5. Implement `errors.rs` with distinct codes for: name taken, invalid name, unauthorized, unknown commit, duplicate branch.
6. Implement `events.rs` with `RepositoryInitialized` / `BranchCreated` via `emit_cpi!`/noop-CPI preferred over string logs (§9.1, §11 #12).
7. Add owner-only authorization helper for MVP (`permissions_mode = 0`); leave allowlist/authority-PDA stubs for Phase 9.
8. Add Anchor/LiteSVM unit tests: happy path, name collision, invalid name, branch from unknown commit, non-owner signer rejection, canonical bump.
9. Add getter/deserialization tests asserting stored fields.

### Architecture / Technical Decisions
- Account layouts are **frozen** here; changing them later forces migration. MVP uses fixed-size accounts (no realloc) per §9.1.
- Authorization is **owner-only** for MVP; the `authority` field exists but allowlist enforcement is deferred to Phase 9 (§16.4).
- Every instruction validates owner, signer, writable, PDA seeds + canonical bump, per §11 checklist.
- Use Anchor `init` only; `init_if_needed` is explicitly forbidden (§9.2, §11 #20).

### Dependencies
Phase 1 (program scaffold). Phase 2 is needed only if instructions must recompute the genesis hash constant — import the genesis function rather than duplicating.

### Files / Modules Likely Affected
`programs/forge_repository/src/state/{repository,branch,commit,tag,permission,attestation}.rs`, `instructions/{initialize_repository,create_branch}.rs`, `errors.rs`, `events.rs`, `lib.rs`, `programs/forge_repository/tests/**`.

### Deliverables
- Compiling program with two working instructions and six account types.
- PDA helper module with tests.
- Events emitted and decoded in tests.

### Validation
- `anchor test` (Surfpool backend) passes.
- Test derives the same PDA addresses from seeds as documented in §4.
- Test asserts `history_root == H("forge-genesis\0" + repo_pda)` and `commit_count == 0` after init.
- Negative tests return the specific error codes.

### Definition of Done
Repository + default branch can be created and read back on a local validator; every error path has a test; account byte sizes match §4 within reserved headroom.

### Risks / Questions
- Exact `#[account]` size accounting must include Anchor's 8-byte discriminator; spec's "~200 bytes" approximations need finalization.
- PDA seed length limits (≤16 seeds, ≤32 bytes each, §2.3) constrain name padding (`[u8;32]`/`[u8;64]`) — reconcile before freezing.
- Whether `PermissionAccount` ships in MVP or is deferred (§4.6 says MVP may inline a small allowlist).

---

## Phase 4 — Onchain Commit Creation & Ed25519 Authorship Verification

### Goal
Implement `create_commit` with Ed25519 signature verification via the Instructions sysvar + native Ed25519 program, parent validation, append-only `history_root` advancement, and duplicate/replay protection per §6.4 and §9.4.

### Scope
- `create_commit` instruction (§9.2) and `CommitAccount` creation.
- Ed25519 verification helper (shared, reusable in Phase 5) using transaction introspection (§9.4).
- Parent-existence and root/first-commit rules (§6.4).
- `history_root` append + `commit_count` increment + `seq` assignment.
- Events: `CommitCreated`.
- CU measurement/profiling for `create_commit` (§9.5).
- Tests for all failure modes (§6.9, §11).

### Tasks
1. Implement `create_commit(commit_oid, parent_count, parent_a, parent_b, tree_oid, authored_at, message_hash, attestation_hash)` with accounts per §9.2.
2. Implement the Ed25519 introspection helper: locate the preceding Ed25519 native instruction via the Instructions sysvar, parse its data, assert program id, counts, offsets, non-empty message, and exact `(pubkey, signature, message)` match (§9.4).
3. Bind verification to `repo` + `commit_oid` so a signature cannot be replayed across repos (§6.8).
4. Implement parent rules: `parent_count ∈ {0,1,2}`; `0` only when repo empty; parents must exist as `CommitAccount` PDAs or equal known head; reject self-parent and cycles (§6.4).
5. On success: `init` Commit PDA keyed `["commit", repo, commit_oid]`, set `seq = repo.commit_count`, increment `commit_count`, append `history_root` (§9.2, §6.7).
6. Emit `CommitCreated` with `seq` as the event sequence number (§9.1).
7. Implement authorization check (owner-only MVP) for the author (§9.2, §6.9).
8. Add tests: valid signed commit; forged author; bad signature; duplicate commit (idempotency/`init` failure); non-root with no parent; root on non-empty repo; parent not found; self-parent; wrong ed25519 offsets; multiple ed25519 instructions (§24 Integration, §11 #1, #3).
9. Profile CU with `sol_log_compute_units!`/Surfpool and record a `SetComputeUnitLimit` recommendation (§9.5).

### Architecture / Technical Decisions
- **MVP simplification (§9.4):** verify only the head/created commit signature onchain; ancestor verification is client-side in Phase 8.
- Use the audited native Ed25519 verifier; no custom crypto (§5.8, §11).
- `message_hash` and `attestation_hash` are stored, never the message; messages stay offchain (§4.2).
- The signature lives in the sidecar attestation, never inside the commit object, to preserve `commit_oid` (§5.5).
- `create_commit` writes the shared `RepositoryAccount` (`history_root`, `commit_count`) — note the write-lock chokepoint for later scalability (§9.5).

### Dependencies
Phase 2 (canonical `attestation_hash`/`history_root`), Phase 3 (accounts, PDAs, errors, events).

### Files / Modules Likely Affected
`instructions/create_commit.rs`, `state/commit.rs`, a new `utils/ed25519.rs` (or `instructions/verify.rs`), `errors.rs`, `events.rs`, program tests.

### Deliverables
- Working `create_commit` with onchain signature verification.
- Reusable Ed25519 helper.
- CU measurement note.

### Validation
- `anchor test` valid commit path passes; `history_root` matches the Phase 2 client-side recomputation.
- Forged/bad signature, duplicate, and bad-parent tests fail with distinct error codes.
- CU measurement recorded and under the 200k default (§9.5).

### Definition of Done
A signed commit can be anchored and re-derived; every §6.9 malicious-commit case has a passing negative test; Ed25519 helper is documented and reused (not duplicated) in Phase 5.

### Risks / Questions
- Ed25519 introspection is the highest-ergonomics/CU risk (§26 risk 3); budget extra iteration.
- The client must prepend the Ed25519 instruction; the program cannot produce it — integration detail to document for Phase 8.
- `emit_cpi!` requires the event-authority CPI mechanism; confirm Anchor 1.1.x support vs. noop-CPI.

---

## Phase 5 — Branch Advancement, Merge & History-Safe Rewrites

### Goal
Implement `update_branch` with optimistic concurrency (`head_seq` CAS), signed branch-update authorization, fast-forward/merge rules, `merge`, and a logged `reset_branch` that never rolls back `history_root` (§6.4, §7).

### Scope
- `update_branch(new_head, expected_head_seq, auth_nonce)` (§9.2) with Ed25519 verification of the domain-separated branch-update message.
- Fast-forward vs merge acceptance rules (§7.2, §7.3).
- `merge(source_branch, target_branch, merge_commit, expected_target_seq)` (§9.2, §6.6).
- `reset_branch` non-fast-forward explicit rewrite that emits an event and preserves history (§7.3, §11 #4/#9).
- Branch delete with default-branch protection (§7.2).
- Events: `BranchUpdated`, `BranchReset`, `BranchDeleted`.
- Race/CAS tests (§7.4).

### Tasks
1. Define and canonically encode the branch-update signed message `(repo, branch_name, new_head, expected_head_seq)` with a `forge-` domain prefix (§6.8).
2. Implement `update_branch`: require `branch.head_seq == expected_head_seq`; verify authority signature; verify `new_commit` exists; accept fast-forward (first parent == head) or merge (one parent == head); set `head_commit`, increment `head_seq`, update `updated_slot` (§7.2, §7.4).
3. Implement `merge`: validate `merge_commit.parent_a == target.head` and `parent_b == source.head` (or vice-versa), both parents exist, `expected_target_seq` matches, authority + signature; advance target (§9.2, §6.6).
4. Implement `reset_branch`: maintainer/admin only (MVP: owner), emit `BranchReset { old_head, new_head, actor }`, do not modify `history_root` (§7.3).
5. Implement branch delete: authority check, forbid deleting the default branch, emit event (§7.2).
6. Reuse the Phase 4 Ed25519 helper for the branch-update auth message.
7. Add tests: fast-forward accepted; non-FF rejected without `reset_branch`; stale `head_seq` rejected (`StaleBranchHead`); replay of old signed update fails after advance; merge with mismatched parents rejected; concurrent two-client race simulation (§7.4, §11 #2/#3/#11).

### Architecture / Technical Decisions
- **Branches are mutable; history is immutable** (§7.6): only `BranchAccount` mutates via `update_branch`/`reset_branch`, and `history_root` is never decremented.
- Compare-and-swap is the concurrency primitive; no value depends on tx ordering (§7.4).
- `reset_branch` is a *distinct* instruction, logged and non-erasable (§7.3).
- MVP authorization remains owner-only; Phase 9 introduces allowlist and authority-PDA.

### Dependencies
Phase 4 (Ed25519 helper, commit accounts), Phase 3 (branch accounts).

### Files / Modules Likely Affected
`instructions/{update_branch,merge,reset_branch,delete_branch}.rs`, `utils/ed25519.rs`, `errors.rs` (add `StaleBranchHead`, `NonFastForward`), `events.rs`, program tests.

### Deliverables
- Branch advancement, merge, reset, and delete working on a local validator.
- Documented signed-message formats.

### Validation
- `anchor test` proves FF, merge, and logged reset.
- Race test: first `update_branch` succeeds, second with same `expected_head_seq` fails.
- Assert `history_root` unchanged before/after `reset_branch` but advanced after commits.

### Definition of Done
All §7.2 operations implemented with tests; CAS and replay protections verified; `history_root` append-only invariant asserted.

### Risks / Questions
- How to locate the source-branch head for `merge` (remaining_accounts vs. explicit accounts) is unspecified — needs a decision (§9.2 merge accounts are ambiguous).
- `reset_branch` is a design extension beyond the MVP MUST list; confirm it is in hackathon scope or mark SHOULD.
- Whether `merge` belongs to MVP MUST or SHOULD (§19.1 lists only create/update branch; §19.2 lists `forge merge`) — program support here; CLI can be Phase 9.

---

## Phase 6 — Content-Addressed Storage Layer (IPFS hot / Arweave cold)

### Goal
Implement the hybrid storage from §8.3: upload Git object bundles/CARs to IPFS with ≥2 pins, verify content by recomputing OIDs on fetch, maintain a storage index, and provide Arweave upload for tags/checkpoints via a bundler.

### Scope
- Bundle/CAR construction from a set of Git objects (§8.3, §13 push detail).
- IPFS upload + multi-pin; fetch by OID/CID.
- Verify-on-read: recompute `oid(blob/tree/commit)` and compare (§8.3, §24).
- Storage backend abstraction (`storage_backend` hints, backend-agnostic).
- `.forge/storage-index` format (`oid → CID/TXID/local path`) (§14.1).
- Arweave upload path (Irys/ArDrive bundler) for tags/checkpoints.
- `forge gc --verify-availability` availability report (§8.3, §13).

### Tasks
1. Implement CAR/git-bundle builder over a set of objects (walk from a commit/tree root).
2. Implement IPFS client (Kubo HTTP RPC or pinning service) with upload, multi-pin (≥2 providers), and fetch.
3. Implement hash-verified fetch: after retrieval, recompute the object OID via `forge-object` and reject mismatches (§8.3).
4. Implement the `StorageBackend` trait and a storage hint type; persist to `.forge/storage-index`.
5. Implement Arweave upload via a bundler, handling the ~10 MB per-tx limit by aggregation (§8.2, §8.3).
6. Implement `forge gc --verify-availability` to report unpinned/absent objects (§8.3).
7. Add tests: OID/CID round-trip equality; corrupted blob rejection; killed-pin availability simulation; empty-object and large-bundle edge cases (§24 Storage consistency).
8. Document the availability policy (≥2 independent pins on push; optional Arweave for tags).

### Architecture / Technical Decisions
- **Content is addressed by Git OID, not CID**; storage hints are optional and never trusted (§8.3, §25 #11).
- IPFS is hot/best-effort; Arweave is for permanence of tags/checkpoints only (§8.3).
- The program only stores `storage_backend` + optional hint hashes; it never stores content (§4.1, §8.3).
- Backend-agnostic interface so a plain CAS/mirror can substitute in tests.

### Dependencies
Phase 2 (OID recomputation), Phase 1 (reqwest/client scaffolding).

### Files / Modules Likely Affected
`cli/src/storage/**` or a `crates/forge-storage` crate, `cli/src/storage-index.rs`, `tests/e2e/storage_*`, `docs/protocol.md`.

### Deliverables
- Upload/fetch with verification and multi-pin.
- Arweave uploader usable by Phase 9 tags/checkpoints.
- Availability report command.

### Validation
- `cargo test -p forge-storage` round-trip and corruption tests pass.
- Killing/removing one pin still allows fetch from the second.
- `forge gc --verify-availability` correctly lists unbacked objects in a fixture.

### Definition of Done
Every fetched byte is re-hashed before use; no code path trusts a storage hint; Arweave upload is callable and tested against a dry-run/mock endpoint.

### Risks / Questions
- Pinning provider and bundler (Irys vs ArDrive) are unspecified — needs a decision (Open Questions).
- Arweave cost/limits depend on live fees (§2.2); mock in tests, document assumptions.
- IPFS gateway reliability is a known risk (§2.2); the interface must allow alternate providers.

---

## Phase 7 — Local-First `forge` CLI (Git-backed, Offline)

### Goal
Deliver the offline Git+Forge workflow from §13/§14: `init`, `add`, `commit`, `status`, `log`, `branch`, `checkout`, `remote add`, and local `verify` (OID + signature + tree), with wallet-signed attestations and the `.forge/` layout.

### Scope
- Build on Git via `gix`/`git2`; `.forge/` holds only Forge metadata (§14.1, §14.3).
- `forge init` (`git init` + `.forge/config`).
- `forge add`, `forge commit -m` (build blobs/trees/commits, sign attestation, store sidecar CBOR).
- `forge status`, `forge log`, `forge branch`, `forge checkout`.
- `forge remote add` (config mapping).
- `forge verify` local portion: recompute OIDs, verify Ed25519 author signature and attestation binding, verify tree/blob hashes.
- Wallet/keypair loading (§15).
- Path-safety enforcement surfaced to the user.

### Tasks
1. Implement `.forge/config` schema: repo PDA, remote mapping, storage backends, wallet ref (§14.1).
2. Implement `forge init [dir]`: run Git init, write config.
3. Implement `forge add <paths>` as a Git index operation.
4. Implement `forge commit -m`: build blobs (dedup), trees bottom-up, commit object; read parent from branch head; produce `commit_oid`; build canonical attestation; sign `attestation_hash` with the wallet; write `.forge/attestations/<oid>.cbor` (§6.1, §14.1).
5. Implement `forge status`: Git status plus local/onchain divergence placeholders from `.forge/onchain-refs`.
6. Implement `forge log [branch]`: walk commits, load sidecar attestations.
7. Implement `forge branch [name]` and `forge checkout <ref>` (historical checkout from local objects).
8. Implement `forge remote add/...` config mapping.
9. Implement local `forge verify [commit]`: recompute OIDs via `forge-object`, verify `Ed25519_verify(author, attestation_hash, signature)`, verify attestation fields match commit, verify tree/blob hashes (§12.4 steps 3–6 minus history inclusion).
10. Enforce path safety at tree construction and report violations (§5.3, §11 #14).
11. Add tests: golden commit construction, attestation sidecar round-trip, local verify valid/invalid, path rejection.

### Architecture / Technical Decisions
- **Git is the object store**; `.forge/` only holds Forge metadata (§14.1, §20).
- Commit construction must use the Phase 2 engine for exact byte compatibility even when Git writes packfiles.
- `forge verify` distinguishes local verification now and adds onchain history inclusion in Phase 8 (exit codes 0/1/2/3 reserved per §12.4).
- No network calls in this phase.

### Dependencies
Phase 2 (canonical engine), Phase 1 (crate). Storage (Phase 6) is not required for local commands; `push`/`clone` come later.

### Files / Modules Likely Affected
`cli/src/{main,commands,git,attest}.rs`, `.forge/*` handling, `tests/e2e/local_*`.

### Deliverables
- Fully offline `forge` tool that creates Git commits with signed Forge attestations.
- `.forge/` layout per §14.1.
- Local `forge verify`.

### Validation
- End-to-end local test: `forge init && forge add . && forge commit -m "x" && forge log`.
- Commit OID equals `git rev-parse HEAD`; attestation verifies.
- Tampering with the sidecar causes `forge verify` to fail.
- Path-traversal input is rejected.

### Definition of Done
A developer can do a normal local Git workflow through `forge`, authorship is cryptographically attested, and local verification passes; no chain dependency for these commands.

### Risks / Questions
- `gix` API coverage for SHA-256 and custom hashing may force a hybrid approach (Open Questions).
- Sidecar attestation filename/format (`.cbor`) must match what Phase 8/9 and the SDK expect.
- Wallet keypair source (file path vs. CLI keypair vs. hardware) unspecified — needs a convention.

---

## Phase 8 — Onchain CLI Integration (push / clone / pull / verify)

### Goal
Connect the CLI to the deployed program: `forge push`, `clone`, `pull`, and full `forge verify` including anchored-history inclusion, with Ed25519-prepended transactions and `head_seq` CAS retry (§13, §18).

### Scope
- Solana RPC client + PDA derivation + instruction encoding (IDL/Codama-generated Rust or hand-built).
- Wallet transaction signing; prepend Ed25519 native instruction for commit and branch-update verification.
- `forge push`: upload bundle, `create_commit` per new commit, one `update_branch` with `expected_head_seq` (§13 push detail).
- `forge clone`: read repo/branch, fetch bundle by OID, verify on fetch, checkout HEAD (§13).
- `forge pull`: fetch objects, fast-forward.
- `.forge/onchain-refs` cache and divergence detection (§14.1).
- `forge verify` full: signature + history inclusion via `history_root` recomputation / head walk (§12.4, §19.1).
- Stale-head CAS retry and "pull first" guidance (§7.4).

### Tasks
1. Implement chain client module (RPC via `solana-sdk`/`@solana/kit` Rust bindings), config for cluster + program ID.
2. Implement PDA derivation on the client, matching Phase 3 helpers.
3. Implement transaction builder that prepends the Ed25519 native instruction with `(author, signature, attestation_hash)` and passes the Instructions sysvar (§9.4).
4. Implement `forge push`: compute missing commits since anchored head, upload CAR/bundle to storage (≥2 pins), send `create_commit` per commit, then `update_branch` with `expected_head_seq`; surface tx signatures and new `head_seq` (§13).
5. Implement stale-head handling: on `StaleBranchHead`, instruct `forge pull`, rebase/merge locally, re-sign, retry (§7.4, §13).
6. Implement `forge clone <repo> [dir]`: read repo/default branch, determine bundle location, fetch and hash-verify, checkout (§13).
7. Implement `forge pull`: fetch missing objects, fast-forward the branch.
8. Implement `.forge/onchain-refs` persistence and `forge status` divergence reporting.
9. Extend `forge verify` to include: fetch commit from CAS, recompute OID, verify signature, walk parents / recompute `history_root` to the anchored head, verify tree + all blobs (§12.4, §18.2).
10. Add integration tests against a local validator: init → commit → push → clone → verify; assert `head_seq`, `commit_count`, `history_root` (§24 E2E).
11. Add two-wallet test: wallet B cannot push a commit claiming wallet A; forged commit rejected (§19.1 demo).

### Architecture / Technical Decisions
- Push ordering: **storage upload before tx** so the anchored commit is retrievable (§13).
- Signed messages use domain prefixes; `head_seq` CAS is the only concurrency control (§6.8).
- Read path: RPC accounts + CAS objects; every hash re-verified client-side (§23 data flow).
- MVP verifies only the created/head commit signature onchain; ancestors verified client-side (§9.4).

### Dependencies
Phase 5 (all program instructions), Phase 6 (storage), Phase 7 (CLI local workflow).

### Files / Modules Likely Affected
`cli/src/chain/**`, `cli/src/commands/{push,clone,pull,verify}.rs`, `.forge/onchain-refs`, `tests/e2e/onchain_*`.

### Deliverables
- Working `push`/`clone`/`pull`/`verify` against a local validator and devnet.
- Transaction preparation with Ed25519 introspection.

### Validation
- Full E2E against local validator passes: `init→add→commit→push→clone→log→verify`.
- Onchain `head_seq`, `commit_count`, `history_root` match client recomputation.
- Forged-author and stale-push scenarios fail as expected.
- Corrupted fetched blob makes `forge verify` fail.

### Definition of Done
Two wallets can create/verify history onchain end-to-end; every anchored root is independently reproducible by the client; no trusted indexer is required.

### Risks / Questions
- Transaction size (1,232 B legacy vs. 4,096 B v1) may limit how many operations fit per tx; one push issues 2 txs (commit, branch) — confirm batching strategy (§2.3, §13).
- Hot `RepositoryAccount` write-lock can serialize concurrent pushes (§9.5) — acceptable for demo.
- Which RPC/commitment level (devnet Helius vs. public) and retry policy.
- v1 transaction support/activation may be pending; default to legacy/v0 for MVP.

---

## Phase 9 — Provenance, Tags, Permissions & Authority Hooks

### Goal
Deliver the SHOULD-HAVE differentiators: `create_tag` + Arweave checkpoint, permission allowlist + `update_permissions` + `transfer_repository`, `anchor_program_source` + `forge verify-program`, CLI `tag`/`merge`, the authority-program hook, and the `git-remote-forge` helper.

### Scope
- Program: `create_tag`, `update_permissions`, `transfer_repository`, `anchor_program_source`, optional `set_program_verified`; permission enforcement in commit/branch instructions; `permissions_mode` allowlist/authority-PDA (§7.5, §9.2, §16).
- CLI: `forge tag` (+ Arweave checkpoint), `forge merge`, `forge verify-program`, permission commands.
- `solana-verify` integration for executable hash and optional reproducible rebuild (§12.3, §12.4).
- `git-remote-forge` helper (§13, §14.3, SHOULD).
- Release checkpointing to Arweave (§8.3).

### Tasks
1. Implement `create_tag(name, target_commit, message_hash, signature)`: authorization, target exists, immutable via `init`; emit `TagCreated` (§9.2).
2. Implement `update_permissions(contributor, role, expires_slot)`: admin check, create/update `PermissionAccount` (no `init_if_needed`), emit `PermissionChanged` (§9.2, §11 #20).
3. Enforce allowlist roles (reader/writer/maintainer/admin) in `create_commit`, `create_branch`, `update_branch`, and the Phase 5 instructions when `permissions_mode = 1` (§7.5, §9.2).
4. Implement `transfer_repository(new_owner)`: owner/admin check, update `owner`, emit `RepositoryTransferred` (§9.2, §15).
5. Implement `anchor_program_source(program_id, commit_oid, artifact_hash, build_metadata_hash)`: verify attester authorized, commit in history, program owned by upgradeable loader, artifact hash non-zero; `init` PDA `["prog", program_id]`; `verified = 0` (§9.2, §12.2).
6. Implement optional `set_program_verified(program_id)` with a separate verifier authority (mark as future/stretch) (§9.2).
7. Implement the authority-program hook: document and test `permissions_mode = 2` where `authority` is a program-owned PDA signing via CPI; provide a minimal mock rule program test (§7.5, §16.2, §16.4). Do **not** build a rule VM (§16.4, §19.4).
8. Implement `forge tag` producing an annotated Git tag + onchain `create_tag` + Arweave checkpoint upload (§13, §8.3).
9. Implement `forge merge <branch>`: Git merge locally, build merge commit, push `merge` (§13).
10. Integrate `solana-verify get-executable-hash`; implement `forge verify-program <id>` steps 1–8 with exit codes 0/1/2/3 and `VERIFIED`/`MISMATCH`/`UNVERIFIED_CLAIM` output (§12.4).
11. Implement `git-remote-forge` helper so `git push forge main` / `git fetch forge` map to Forge push/clone (§13, §14.3).
12. Add tests: tag immutability, permission enforcement per role, ownership transfer, provenance claim creation + verification, mismatched artifact hash → `MISMATCH`.

### Architecture / Technical Decisions
- Reuse audited governance programs (Squads/Realms) via the authority-PDA hook; no new rule language/VM (§16.3, §16.4).
- Provenance attestations are **claims** (`verified = 0`) until an independent rebuild marks them verified; `forge verify-program` clearly distinguishes claim vs. verified (§9.2, §12.4).
- Adopt SLSA/in-toto-shaped fields for `build_metadata_hash` offchain (§12.3, §17.3).
- `set_program_verified` is a separate trust level and may ship as stretch.

### Dependencies
Phase 8 (chain client, CLI integration), Phase 6 (Arweave for tags/checkpoints), Phase 5 (branch instructions to enforce permissions).

### Files / Modules Likely Affected
`instructions/{create_tag,update_permissions,transfer_repository,anchor_program_source,set_program_verified}.rs`, `state/{tag,permission,attestation}.rs`, `cli/src/commands/{tag,merge,verify_program,permissions}.rs`, `cli/src/chain/provenance.rs`, `git-remote-forge` binary, program tests.

### Deliverables
- Tagging with permanent checkpoint, permission model, ownership transfer.
- Program→commit provenance creation and `forge verify-program`.
- Authority-program hook documented and minimally tested.
- `git-remote-forge` helper.

### Validation
- `anchor test` covers tag/permission/transfer/provenance paths.
- `forge tag` writes tag onchain and a checkpoint to Arweave (or mock).
- `forge verify-program` on a known devnet program returns the expected status and exit code.
- Allowlist test: unauthorized writer is rejected; authorized writer succeeds.
- Mock rule-program CPI test advances a protected branch.

### Definition of Done
All SHOULD-HAVE protocol features work and are tested; provenance claim vs. verified is distinct; rule hook demonstrated without implementing a VM.

### Risks / Questions
- Scope creep: tags+permissions+provenance+remote-helper is large; may need to split or defer `git-remote-forge` (Open Questions).
- `solana-verify` reproducible rebuild depends on Docker and pinned images; may be flaky in CI — allow claim-only path.
- Squads/Realms integration is a demo stretch, not required to prove the hook.
- Provenance semantics for upgrade authority changes are unspecified.

---

## Phase 10 — TypeScript SDK, Explorer UI, Indexer & End-to-End Demo Hardening

### Goal
Complete the §19/§20 demo surface: Codama-generated TS SDK, minimal Next.js explorer (repo list, commit log, verify badge), optional Helius indexer, cost/CU benchmark, and the full two-wallet end-to-end demo with negative cases in CI.

### Scope
- `@solana/kit` v7+ TS SDK with Codama-generated client + helpers (§21, §23).
- Minimal web UI: repo list, commit log, verify badge, program provenance view (§19.2, §20).
- Optional Helius webhooks/Geyser indexer (not required for MVP with per-commit accounts) (§19.3, §21).
- Cost/scalability benchmark vs. §10 estimates.
- End-to-end CI: full flow, forged commit rejection, stale push rejection, history rewrite visibility (§24).
- Final demo script/narrative per §18.4.

### Tasks
1. Generate the Codama client from the program IDL; commit generated code (§21, §23).
2. Implement SDK helpers mirroring §23: `createRepository`, `getRepository`, `createCommit`, `getCommit`, `getBranch`, `updateBranch`, `verifyCommit`, `getFile`, `verifyProgram`.
3. Ensure every SDK read re-verifies hashed values from CAS client-side (§23 data flow).
4. Implement the minimal Next.js web UI: repo list, commit log (with author + verify status), verify badge, provenance view (§19.2).
5. Implement optional `indexer/` Helius webhook consumer to serve fast queries; clearly mark as non-authoritative cache (§3.2, §9.1).
6. Add `forge gc` behavior and any remaining §13 commands marked ◐ where feasible.
7. Implement the cost/CU benchmark: measure per-account rent and per-push CU/CU-limit against §10 tables; record actual `lamports_per_byte` at run time.
8. Add E2E CI: `init→add→commit→push→clone→log→verify`; two-wallet authorship; forged commit rejected; stale push rejected; `reset_branch` event + preserved `history_root` (§24).
9. Add fuzzing (nice-to-have): Trident instruction sequences and `cargo-fuzz` for the object/CBOR parser (§24 Fuzzing).
10. Write the demo script implementing §18.4 steps 1–8 and the §19.1 demo scenarios.
11. Finalize `docs/protocol.md` and `docs/threat-model.md` mapping implemented mitigations to §11.

### Architecture / Technical Decisions
- **Prefer `@solana/kit` plugins**; do not use `@solana/wallet-adapter-*` or framework-kit (§21).
- Indexer is a UX cache and **never authoritative** (§3.2, §9.1); the verify path must not depend on it.
- Per-commit `CommitAccount` stays for MVP queryability; events + indexer path documented as the scale step (§10.4).
- Explicitly out of scope (documented, not implemented): MMR proofs, ZK-compressed commits, private/encrypted repos, storage incentives, full Git smart protocol, rule VM, onchain file storage (§19.4).

### Dependencies
Phase 9 (provenance/tags/permissions), Phase 8 (chain CLI), Phase 7 (local workflow), Phase 6 (storage). Nearly all prior phases.

### Files / Modules Likely Affected
`sdk/src/{client,repo,commit,verify,storage}.ts`, `web/app/**`, `indexer/src/**`, `tests/e2e/**`, `docs/{protocol,threat-model}.md`, `README.md`, CI workflow.

### Deliverables
- Published/usable TS SDK with Codama client.
- Explorer UI with live verify badges.
- Optional indexer.
- Benchmark report and green E2E CI.

### Validation
- `npm test`/SDK typecheck and unit tests pass.
- Web UI renders repo list + commit log + verify badge against local validator/devnet.
- E2E CI proves all §19.1 demo scenarios including the two negative cases.
- Benchmark numbers recorded with live constants; discrepancies vs. §10 called out.
- `cargo clippy` + `anchor test` + golden vectors still green.

### Definition of Done
The §19.1 MVP demo runs reproducibly from a clean checkout; SDK/web consume the same onchain state; verify is independent of any trusted API; all non-goals are documented as such.

### Risks / Questions
- Codama/@solana/kit v7 API churn may require adjustments; pin versions.
- Web UI + indexer + benchmark in one phase may be too broad; indexer and fuzzing are explicitly optional (§19.3).
- Indexer/RPC rate limits and `getProgramAccounts` cost (§10.4) may affect the UI; must degrade gracefully.
- Live network parameters (rent, v1 tx activation) may differ from the spec's research date; re-verify (§2.3, disclaimer).

---

### 1. Phase Dependency Graph

```
Phase 1 (Foundation)
   │
   ▼
Phase 2 (Object/Hashing Engine)
   │
   ├──────────────► Phase 6 (Storage) ────────────────┐
   │                    ▲                              │
   ▼                    │                              │
Phase 3 (State/Init/Branch)                             │
   │                                                    │
   ▼                                                    │
Phase 4 (create_commit + Ed25519)                       │
   │                                                    │
   ▼                                                    │
Phase 5 (update_branch/merge/reset)                     │
   │                                                    │
   │                                                    ▼
   └──────────────────────────────────────────────► Phase 8 (CLI ↔ chain: push/clone/pull/verify)
                                                        │
Phase 2 ──► Phase 7 (Local CLI, offline) ───────────────┤
                                                        │
                                                        ├──► Phase 9 (Provenance/Tags/Permissions/Hooks)
                                                        │        │
                                                        │        ▼
                                                        └──► Phase 10 (SDK/Web/Indexer/E2E Demo)
                                                                 ▲
                          Phase 7 ───────────────────────────────┘
```

Explicit edges:
- P1 → P2
- P2 → P3 → P4 → P5
- P2 → P6
- P2 → P7
- P5 → P8, P6 → P8, P7 → P8
- P8 → P9
- P8 → P10, P9 → P10, P7 → P10

### 2. Requirement Coverage Matrix

| Requirement (spec §) | Phase | Tasks | Validation |
|---|---|---|---|
| Neutral global anchor; append-only history (§1.2, §5.6) | 3, 4 | P3:3; P4:5 | E2E assert `history_root` monotonic; recompute matches |
| Wallet-native authorship / signed commit (§1.2, §5.5, §6.3, §15) | 2, 4, 7, 8 | P2:7; P4:2,3; P7:4,9; P8:3 | Golden attestation; forged commit rejected; two-wallet test |
| Repositories as onchain objects + transferable ownership (§1.2, §15) | 3, 9 | P3:1,3; P9:4 | Init + transfer tests, `RepositoryTransferred` |
| Programmable rules / authority-program hook (§16) | 9 | P9:7 | Mock rule-program CPI test |
| Program→commit→source provenance (§12) | 9 | P9:5,10 | `forge verify-program` status + exit codes |
| Reuse Git object model (§2.1, §5.1) | 2 | P2:1–6 | `git hash-object` golden vectors |
| Tree ordering determinism (§5.3, §5.7) | 2 | P2:3,10 | `foo` vs `foo/` vs `foo.txt` vectors |
| Path/symlink safety (§5.3, §11 #14) | 2, 7 | P2:4,5; P7:10 | Traversal/NUL/absolute rejection tests |
| SHA-256 native, SHA-1 read interop (§5.1, §5.8) | 2 | P2:1,12 | SHA-1 vector tests; SHA-256 default |
| Canonical attestation (§5.5) | 2 | P2:7,11 | Determinism/map-ordering tests |
| `history_root` genesis/append; `repo_root` (§5.6) | 2, 4 | P2:8,9; P4:5 | §5.7 worked example; on/offchain consistency |
| `RepositoryAccount` (§4.1) | 3 | P3:1,3 | Field/size assertions |
| `CommitAccount` (§4.2) | 3, 4 | P3:1; P4:5 | Deserialization tests |
| `BranchAccount` (§4.4) | 3 | P3:1,4 | Field/size assertions |
| `TagAccount` (§4.5) | 9 | P9:1 | Tag immutability test |
| `PermissionAccount` (§4.6) | 9 | P9:2,3 | Role enforcement tests |
| `ProgramSourceAttestation` (§4.7) | 9 | P9:5 | Claim creation + verify test |
| `initialize_repository` (§9.2) | 3 | P3:3 | Happy + negative tests |
| `create_commit` + parent validation (§9.2, §6.4) | 4 | P4:1,4,8 | All §6.9 failure tests |
| `create_branch` (§9.2, §7.2) | 3 | P3:4 | Duplicate/unknown/unauthorized tests |
| `update_branch` CAS + signed auth (§9.2, §7.4) | 5 | P5:1,2,7 | Stale/replay/race tests |
| `merge` (§9.2, §6.6) | 5, 9 | P5:3; P9:9 | Mismatched-parent rejection; CLI merge |
| Non-FF `reset_branch` logged (§7.3) | 5 | P5:4,7 | `BranchReset` emitted; `history_root` preserved |
| `create_tag` (§9.2) | 9 | P9:1,8 | Immutability + Arweave checkpoint |
| `transfer_repository` (§9.2) | 9 | P9:4 | Ownership change test |
| `update_permissions` (§9.2) | 9 | P9:2,3 | Role enforcement |
| `anchor_program_source` (§9.2, §12.2) | 9 | P9:5 | Provenance PDA creation test |
| `set_program_verified` (§9.2) | 9 (stretch) | P9:6 | Flag transition test or documented deferral |
| Events for indexers (§9.1) | 3, 4, 5, 9 | P3:6; P4:6; P5:5 | Event decode tests; `seq` gap detection |
| Ed25519 introspection pattern (§9.4) | 4, 5 | P4:2,3; P5:6 | Offset/count/non-empty message tests |
| CU measurement (§9.5) | 4, 5, 10 | P4:9; P5; P10:7 | Recorded CU under 200k default |
| Hybrid storage IPFS+Arweave (§8.3) | 6 | P6:2,5 | Round-trip + mock Arweave upload |
| Verify-on-read / corruption detection (§8.3, §24) | 6, 8 | P6:3,7; P8:9 | Corrupted blob fails verify |
| Multi-pin availability (§8.3) | 6 | P6:2,6,7 | Killed-pin simulation |
| CLI `init/add/commit/status/log/branch/checkout` (§13) | 7 | P7:2–8 | Local E2E test |
| CLI `push/pull/clone` (§13) | 8 | P8:4,6,7 | Onchain E2E |
| CLI `verify` (§13, §18.2) | 7, 8 | P7:9; P8:9 | Local + history-inclusion verification |
| CLI `verify-program` (§12.4) | 9 | P9:10 | Status/exit-code tests |
| CLI `tag`/`merge` (§13, §19.2) | 9 | P9:8,9 | Tag/merge E2E |
| `git-remote-forge` (§13, §14.3, SHOULD) | 9 | P9:11 | `git push forge main` test |
| `.forge/` layout (§14.1) | 7 | P7:1 | Layout/config schema test |
| Local repo built on Git (§14.3) | 7 | P7:2,4 | Commit OID == `git rev-parse HEAD` |
| Identity: wallet-native, multisig/rotation (§15) | 7, 9 | P7:4; P9:2,4 | Signed attestation; transfer/rotation events |
| GitHub linking (soft hint, future) (§15) | Non-goal | — | Documented future |
| CI/CD `CIResultAccount` + SLSA (§17) | Non-goal (future) | P9:10 (SLSA-shaped metadata only) | Provenance fields aligned |
| TS SDK / Codama (§21, §23) | 10 | P10:1–3 | SDK tests + typecheck |
| Web UI repo list/commit log/verify badge (§19.2) | 10 | P10:4 | UI renders live state |
| Indexer Helius (§19.3, optional) | 10 | P10:5 | Marked non-authoritative |
| Testing strategy unit/integration/E2E/storage (§24) | all | per-phase tests | CI green each phase |
| Security checklist (§11) | 3, 4, 5, 9 | P3:2–5; P4:2,7; P5:7; P9:3 | Negative tests per attack row |
| Cost/scalability validation (§10) | 10 | P10:7 | Benchmark vs. §10 tables |
| Devnet deployment (§19.1, §21) | 1, 8 | P1:2,7; P8:1 | Program callable on devnet |
| Two-wallet demo + negative cases (§19.1) | 8, 10 | P8:11; P10:8 | Forged + stale push rejected |
| Explicit non-goals (§19.4) | 10 | P10:architecture decision | Documented in `docs/threat-model.md` |

### 3. Technical Risks

| # | Risk (spec §26 + derived) | Impact | Phase to address |
|---|---|---|---|
| 1 | Onchain state cost if per-commit accounts kept at scale (§10, §26.1) | high | Phase 3 (layout), Phase 10 (benchmark + document events/compression path) |
| 2 | Storage availability — anchored history outlives data (§8.2, §26.2) | high | Phase 6 (multi-pin/Arweave), Phase 10 (`gc --verify-availability`) |
| 3 | Ed25519 introspection ergonomics/CU per push (§9.4, §26.3) | high | Phase 4 (helper + CU profiling), Phase 5 (reuse) |
| 4 | Git ↔ SHA-256 interop and tree-ordering determinism bugs (§5.1, §5.3, §26.4) | high | Phase 2 (golden vectors) |
| 5 | Hot `RepositoryAccount` write-lock throughput (§9.5, §26.5) | medium-high | Phase 4 (note), Phase 10 (benchmark + sharding doc) |
| 6 | `gix` SHA-256 support / object-writing gaps | high | Phase 1 (decision), Phase 2 (fallback to hand-rolled hashing) |
| 7 | IPFS gateway/pinning reliability and cost (§2.2) | medium | Phase 6 (provider abstraction), Phase 10 |
| 8 | Arweave bundler limits/cost + endowment risk (§8.2) | medium | Phase 6 (aggregation + mock), Phase 9 (tags) |
| 9 | Transaction size / v1 activation uncertainty (§2.3) | medium | Phase 8 (batching, default legacy) |
| 10 | Account size/discriminator and seed-length limits (§2.3, §4) | medium | Phase 3 (freeze layouts, validate seeds) |
| 11 | Scope creep from SHOULD/FUTURE tiers (§19) | medium | Phase 9 (explicit deferral list), Phase 10 (non-goals) |
| 12 | TS SDK/Codama API churn (§21, §23) | medium | Phase 10 (pin versions) |
| 13 | Realloc 10 KB/instruction limit if history moves to log PDA (§9.5, §10.3) | low-medium | Phase 10 (scalability documentation only) |
| 14 | Live network parameters differ from spec research date (§2.3, disclaimer) | medium | Phase 10 (re-verify at benchmark time) |
| 15 | `emit_cpi!` event support/Anchor version specifics (§9.1) | low-medium | Phase 3 (events module spike) |

### 4. Open Questions

These cannot be determined from the specification and must not be silently assumed:

1. **`gix` vs. `git2` (vs. hand-rolled Git writer):** §21 says `gix`/`git2` but does not resolve SHA-256 support, which Phase 2 depends on. Which library, and what is the fallback if SHA-256 writing is unsupported?
2. **Attestation encoding:** §5.5 offers "canonical CBOR or canonical JSON (JCS)." Which one? It affects golden vectors and all consumers.
3. **`history_root` endianness and attestation map ordering:** §5.6/§5.5 do not specify byte order or field ordering. Must be frozen in Phase 2.
4. **Storage providers:** §8.3 names IPFS and Arweave but not specific pinning services or bundler (Irys vs ArDrive). Which providers, and what are the credentials/config?
5. **Wallet/keypair management:** §15 lists Phantom/Backpack/Solflare or CLI keypair; the CLI needs a concrete convention (file path, env var, hardware). Which?
6. **RPC provider:** §21 says Helius (devnet) with public fallback. Which endpoints/keys, and is an indexer funded?
7. **`PermissionAccount` in MVP:** §4.6 says MVP "may encode a small allowlist inline and skip this account." Inline allowlist or separate accounts? This affects Phase 3 layout.
8. **`reset_branch` scope:** it is a design decision (§7.3) but not in the MVP MUST list (§19.1). Is it in hackathon scope or SHOULD?
9. **`merge` tier:** program merge is in §9.2 but §19.1 MUST omits it and §19.2 lists `forge merge` as SHOULD. Is onchain merge MVP or SHOULD?
10. **`set_program_verified`:** §9.2 marks it future; is it in scope for the hackathon or documented only?
11. **`git-remote-forge`:** SHOULD-HAVE but a significant effort. Is it in scope, and does it target a minimal fetch/push helper or full Git smart protocol (§19.4 defers the full protocol)?
12. **Merge source-branch discovery:** §9.2 merge accounts are ambiguous ("Source head is read offchain / via remaining accounts"). Which mechanism?
13. **Event mechanism:** `emit_cpi!` vs. noop-CPI vs. legacy logs at the pinned Anchor version (§9.1).
14. **Indexer requirement:** §19.3 marks it NICE-TO-HAVE and §10.4 says per-commit accounts remove the need for MVP. Build it or defer?
15. **Web UI authenticity:** the spec says UI is untrusted (§3.2); does the verify badge necessarily run full client-side verification, or display onchain claim status with a separate verify action?
16. **SHA-256 forge digest for SHA-1 repos (§5.8):** full or sampling-based, and in scope at all for MVP?
17. **Devnet program upgrade authority:** who owns the deployed program and how are upgrades coordinated across phases?
18. **CI environment for `solana-verify` rebuilds:** Docker availability and whether the reproducible-rebuild path is required or claim-only is acceptable (§12.4 step 7, §19.2 "basic" verify-program).

### 5. Execution Rules for Future Coding Agents

1. **Complete phases sequentially.** Do not start Phase N+1 until Phase N passes its Definition of Done and Validation. The dependency graph in §1 is authoritative; if a cross-phase dependency is discovered, stop and escalate rather than reordering silently.
2. **Do not skip Definition of Done checks.** Every phase must run its declared validation commands (tests, `anchor test`, `cargo clippy`, golden vectors) and record the results.
3. **Do not modify requirements.** Requirements come from `ONCHAIN_VERSION_CONTROL_ARCHITECTURE.md`. If a spec statement is ambiguous, add it to Open Questions and ask; never invent a requirement or silently drop one.
4. **Keep changes within the current phase's scope.** Do not implement later-phase features opportunistically. If a later feature is needed for a dependency, make the minimal change and note it.
5. **Never trust offchain bytes.** Any hash, blob, CID, storage hint, or generated client value must be re-verified before use (spec §3.2, §8.3, §23, §24).
6. **Freeze interfaces once introduced.** Account layouts (Phase 3), canonical encodings (Phase 2), and signed-message formats (Phase 4/5) must not change without explicit approval and a migration note.
7. **One source of truth for hashing.** All OID/attestation/history computations go through `forge-object`; the program must not independently re-serialize.
8. **Security checklist every instruction.** Follow §11 cross-cutting checklist: validate owner, signer, writable, PDA seeds + canonical bump; no `init_if_needed`; checked math; no arbitrary CPI; validate `remaining_accounts`; secure close; test each error path.
9. **Respect the onchain/offchain boundary.** Only signature- or root-critical data goes onchain; never store files or full messages onchain (§1.3, §4, §8.3, §26).
10. **Preserve the MVP simplifications.** Owner-only auth until Phase 9; head-only onchain signature verification with client-side ancestor verification; per-commit accounts in MVP (§9.4, §10.4, §16.4).
11. **Do not build explicit non-goals** without approval: custom merge engine, general rule VM, onchain file storage, MMRs, ZK-compressed accounts, private repos, storage incentives, full Git smart protocol (§19.4).
12. **Re-verify live constants.** Rent, transaction-size activation, and fee assumptions may have changed since the research date; re-check before benchmarking or deployment (§2.3, disclaimer).
13. **Record decisions as ADRs.** For every open question resolved, write a short decision record (context, options, choice, consequences) in `docs/` before proceeding.
14. **Keep CI green.** Any commit that breaks golden vectors, clippy, or `anchor test` must be fixed before the phase is considered complete.
15. **Traceability.** Reference spec section numbers in code comments and tests so future agents can map implementation back to requirements (e.g. `// §5.3 tree ordering`).
