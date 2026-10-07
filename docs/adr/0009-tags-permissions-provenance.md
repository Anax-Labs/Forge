# ADR 0009 — Tags, permissions, ownership transfer, and provenance

- **Status:** Accepted (Phase 9, program side; CLI in Phase 9b)
- **Spec:** §4.5 (tags), §4.6 (permissions), §4.7/§12.2/§12.4 (provenance),
  §7.5/§16 (roles/authority hook), §9.2, §15 (ownership).
- **Related:** ADR 0002 (names/seeds), ADR 0004 (Ed25519), ADR 0005 (branch refs).

## Context

Phase 9 delivers the SHOULD-HAVE differentiators: signed release tags,
contributor roles, ownership transfer, and the permissionless program→commit
provenance link. The roadmap flags this phase for possible splitting; this ADR
records what shipped and what is deferred.

## Decision

### Tags (`create_tag`)

An immutable release pointer. The tagger signs
`forge_object::tag::tag_message` (`"forge-tag\0" || repo || name || target_commit
|| message_hash`); the program verifies it via Ed25519 introspection (ADR 0004)
and stores `signed = 1`. `TagAccount` is Anchor `init`-only, so a tag name can
never be overwritten. Authorization is writer-role (owner or allowlisted writer).

### Permissions (`update_permissions`)

Roles (`reader`/`writer`/`maintainer`/`admin`) live in a per-contributor
`PermissionAccount`. Because `init_if_needed` is forbidden (§11 #20), the handler
has explicit create and update paths: an empty PDA is created with the guarded
helper, an existing one is rewritten in place. Administration is owner-only in
the MVP; `role > admin` is `InvalidRole`.

Enforcement is centralised in `refs::require_min_role`: the owner is always
authorized; otherwise the signer's `PermissionAccount` must be supplied as a
remaining account, be unexpired, and have `role >= min_role`. It is wired into
`create_commit` (writer) and `update_branch` (maintainer); the remaining branch
operations stay owner-only until Phase 9b widens them. Missing permission →
`Unauthorized`; insufficient role → `InsufficientRole`.

### Ownership transfer (`transfer_repository`)

Updates `RepositoryAccount.owner`. **Caveat:** the repository PDA seeds include
the original owner (`["repo", owner, name]`), so the PDA is stable across a
transfer and is *not* re-derivable from the new owner. Clients must persist and
use `RepositoryAccount.repo_id`. This is a documented consequence of the frozen
Phase 3 seed layout (ADR 0002), not a new address scheme.

### Provenance (`anchor_program_source`)

Creates a `ProgramSourceAttestation` PDA keyed by `program_id`, mapping
`program_id → repo → commit → artifact_hash` (§12.2). Validation: attester has
writer permission; the commit is already in the repository's history; the
referenced program account is owned by the upgradeable loader and executable;
`artifact_hash != 0`. The record is a **claim** (`verified = 0`); `forge
verify-program` (Phase 9b) distinguishes a claim from an independently verified
build. One claim per program (the PDA is keyed by `program_id`).

## Deferred (tracked)

- **CLI (Phase 9b):** `forge tag`, `forge merge`, `forge verify-program`,
  permission commands; Arweave checkpoint upload for tags.
- **`set_program_verified`:** requires a designated verifier authority, which is
  not yet defined; kept as a distinct future trust level (§9.2).
- **Authority-PDA rule hook (§16.2):** the `authority`/`permissions_mode` fields
  exist and `require_min_role` is the enforcement seam; a mock CPI rule program
  is not built (roadmap allows deferring; no rule VM).
- **`git-remote-forge`:** SHOULD-HAVE; deferred per the roadmap's scope note.
- **`PermissionAccount` is absent from the generated IDL** because
  `update_permissions` uses an `UncheckedAccount` for its create-or-update path.
  The struct is public and exported, but Codama clients cannot decode it until
  Phase 9b exposes it (e.g. a read-only getter or a split create/update).

## Consequences

- Tags are cryptographically attributable and immutable.
- Roles are stored and enforced on the highest-value write paths; widening to all
  branch ops is a small, mechanical follow-up via `require_min_role`.
- Provenance is a first-class, trust-minimised claim; independent verification is
  a separate step and clearly labelled.
