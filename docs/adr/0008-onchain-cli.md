# ADR 0008 — Onchain CLI (push / clone / pull / verify)

- **Status:** Accepted (Phase 8)
- **Spec:** §9.4 (Ed25519 prepend), §12.4 (verify), §13 (push/clone/pull),
  §7.4 (`head_seq` CAS).
- **Related:** ADR 0004, ADR 0005, ADR 0006, ADR 0007.

## Context

Phase 8 must connect the local Git-backed CLI to `forge_repository` without an
indexer. Instruction layouts are frozen. Storage locators remain untrusted.

## Decision

### Client encoding

The CLI hand-builds Anchor instruction data (`sha256("global:<name>")[..8]`)
and account metas matching the Phase 4/5 LiteSVM tests, including `emit_cpi!`
trailing `event_authority` + program accounts. PDA seeds reuse
`repo` / `branch` / `commit`. No Codama/IDL client in MVP.

### Transports

| Backend | Use |
|---|---|
| LiteSVM | In-process tests (`cli/tests/onchain_workflow.rs`) |
| JSON-RPC (`FORGE_RPC`, default `http://127.0.0.1:8899`) | local validator / devnet |

Transactions are **legacy/v0**. One push is N `create_commit` txs plus one
`update_branch` tx; each signed instruction is prepended with the Ed25519
native verify ix (exactly one per tx, ADR 0004).

### Push ordering

1. `initialize_repository` if the PDA is missing (owner + `config.name`).
2. Upload a CARv1 of reachable objects to ≥2 pins **before** any tx (ADR 0006).
3. Re-sign attestations with `repo = <pda>` (commits made offline used `"local"`).
4. `create_commit` oldest-first for missing commit PDAs.
5. One `update_branch` with `expected_head_seq`. On `StaleBranchHead` (6015)
   tell the user to `forge pull`, rebase/re-sign, retry.

### Clone / pull

Read repo + branch accounts. Fetch the CAR via `.forge/storage-index` (or
`FORGE_STORAGE` shared root), re-hash every object, checkout HEAD. Pull
fast-forwards only when local HEAD is an ancestor of the onchain head.

### Verify

Always: Git OIDs + sidecar Ed25519 (`LOCAL_VERIFIED` if no PDA). With a PDA:
load `CommitAccount`, walk parent PDAs, sort by `seq`, recompute
`history_root_chain`, require `commit_count` and `history_root` match
(`VERIFIED`). Exit codes 0/1/3.

## Consequences

- Clone of a foreign machine still needs locators (IPFS APIs or a shared
  `FORGE_STORAGE`). The protocol address is the Git OID, not the CID.
- Hardware wallets and v1 (4096-byte) transactions remain out of scope.
