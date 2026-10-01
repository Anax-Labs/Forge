# ADR 0005 — Branch advancement, merge, reset, and delete

- **Status:** Accepted (Phase 5)
- **Spec:** §6.6 (merge), §6.8 (signed update / replay), §7.2 (branch ops),
  §7.3 (non-fast-forward reset), §7.4 (optimistic concurrency), §9.2
  (`update_branch`/`merge`), §11 #4/#9 (history rewrite).
- **Related:** ADR 0003 (guarded init), ADR 0004 (Ed25519 introspection).

## Context

Branches are the only mutable refs in the protocol. Moving a head must be
authorized, race-safe, and observable, while never erasing history. This phase
adds `update_branch`, `merge`, `reset_branch`, and `delete_branch`.

## Decision

### Signed authorization messages

`forge-object::branch` defines two canonical, domain-separated messages:

```text
update = "forge-branch-update\0" || repo || name || new_head32 || expected_head_seq_le
reset  = "forge-branch-reset\0"  || repo || name || new_head32 || expected_head_seq_le
```

The wallet signs the message; the program verifies it with
`verify_ed25519_instruction_preceding` (ADR 0004). Distinct domains prevent a
fast-forward authorization from being replayed as a history rewrite. The
nonce argument sketched in §9.2 is **omitted**: `expected_head_seq` is a
monotonic compare-and-swap token, so a signature is single-use by construction
and a separate nonce adds nothing.

### `update_branch` — fast-forward or merge only

- CAS: requires `branch.head_seq == expected_head_seq` (`StaleBranchHead`),
  then increments (`MathOverflow` guarded).
- Relationship: the new commit must have the current head as its only parent
  (fast-forward) or as one of two parents (merge); otherwise `NonFastForward`.
  An empty branch (all-zero head) accepts any existing commit, matching
  `create_branch from_commit`.
- Auth: owner-only (MVP). The new commit is validated via `refs::load_commit`.
- Emits `BranchUpdated`. The repository `history_root` is untouched.

### `merge` — two-parent topology only

The merge *content* is computed offchain. Onchain, `target` and `source` are
explicit branch accounts; the merge commit must have exactly two parents equal
to `(target.head, source.head)` in either order (`InvalidMerge`). The target is
advanced with the same CAS as `update_branch`, authorized by the target
branch-update message. Emits `BranchUpdated`.

### `reset_branch` — explicit, logged non-fast-forward

A distinct instruction so history rewrites are visible. Same CAS and signature
scheme (reset domain) but **no** fast-forward requirement: the target must only
exist as a real commit of the repository. Emits `BranchReset` and never modifies
`history_root`, so previously seen commits remain anchored (§11 #4/#9).

### `delete_branch`

Owner-only. Forbids deleting the repository's default branch
(`CannotDeleteDefaultBranch`); otherwise closes the branch account and refunds
rent to the authority. Emits `BranchDeleted`.

### Commit-account deserialization bug

`AccountDeserialize::try_deserialize` expects the buffer **including** the 8-byte
discriminator. `create_commit`'s parent validation (and initially this phase's
`refs::load_commit`) passed `&data[8..]`, which would have rejected any real
parent. Fixed to pass the full slice; a parent-chain regression test was added.

## Consequences

- Concurrency is purely compare-and-swap: the loser refetches, rebases, re-signs,
  retries (§7.4).
- History rewriting is possible but always visible (a `BranchReset` event) and
  never removes commits from the anchored log.
- Phase 9 replaces owner-only authorization with allowlists / authority PDAs;
  the `branch.authority` field is already recorded.
