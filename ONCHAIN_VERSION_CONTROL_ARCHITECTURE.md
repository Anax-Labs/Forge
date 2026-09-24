# Onchain Version Control — Architecture & Technical Design

**Working name:** `Forge` (protocol), `forge` (CLI)
**Document status:** Architecture / pre-implementation specification
**Research date:** 2026-09-22
**Target:** Colosseum hackathon, Solana
**Author:** Protocol design working document

> **Reading conventions.** This document distinguishes **Facts** (cited from primary sources) from **Design decisions** (our recommendations) from **Estimates** (arithmetic on cited constants, marked uncertain). Where a number is a live network parameter it may change; always re-check before relying on it. Every externally-sourced claim is linked.

---

## Table of Contents

1. [The Core Idea — and what blockchain actually adds](#1-the-core-idea--and-what-blockchain-actually-adds)
2. [Research: prior art and building blocks](#2-research-prior-art-and-building-blocks)
3. [Architecture overview](#3-architecture-overview)
4. [Data model](#4-data-model)
5. [Hashing & Merkle model](#5-hashing--merkle-model)
6. [Commit model](#6-commit-model)
7. [Branches & merging](#7-branches--merging)
8. [Storage architecture](#8-storage-architecture)
9. [Solana program architecture](#9-solana-program-architecture)
10. [Cost & scalability analysis](#10-cost--scalability-analysis)
11. [Security model](#11-security-model)
12. [Code provenance](#12-code-provenance)
13. [CLI design](#13-cli-design)
14. [Local repository architecture](#14-local-repository-architecture)
15. [Authentication & identity](#15-authentication--identity)
16. [Programmable repository rules](#16-programmable-repository-rules)
17. [CI/CD integration](#17-cicd-integration)
18. [Developer / user experience](#18-developer--user-experience)
19. [Hackathon MVP](#19-hackathon-mvp)
20. [Recommended MVP architecture](#20-recommended-mvp-architecture)
21. [Technology stack](#21-technology-stack)
22. [Repository structure](#22-repository-structure)
23. [API / SDK design](#23-api--sdk-design)
24. [Testing strategy](#24-testing-strategy)
25. [Open questions](#25-open-questions)
26. [Final architecture summary](#26-final-architecture-summary)
- [References](#references)

---

## 1. The Core Idea — and what blockchain actually adds

We are building an **onchain version-control protocol**: Git-like repositories whose history, authorship, branch state, and source-code provenance are cryptographically anchored on Solana, while file contents live in content-addressed offchain storage.

### 1.1 The trap to avoid

The naive framing is "Git on a blockchain." That framing is weak, because **Git is already a content-addressed Merkle DAG and is already distributed**. A blob/tree/commit chain is self-verifying: if you have the objects and the tip hash, you can verify the entire history without trusting a server. Rolling a Git object store into Solana accounts gains little by itself and costs a lot of rent.

So the first protocol-engineer question is: *what does a globally-shared, censorship-resistant, programmable ledger add that Git/GitHub does not already provide?*

### 1.2 What is genuinely new

| Capability | Git | GitHub / centralized forge | **Forge (this design)** |
|---|---|---|---|
| Content-addressed history | ✅ (SHA-1/256 objects) | ✅ (hosts Git) | ✅ (reuses Git objects) |
| Offline/local-first | ✅ | ❌ | ✅ |
| **Globally shared history anchor** | ❌ (each clone independent) | ⚠️ single company's DB | ✅ **Solana as neutral anchor** |
| **Cryptographic author identity** | ⚠️ email string (+ optional GPG) | ⚠️ account login | ✅ **wallet signature over commit ID** |
| **Self-enforcing repository rules** | ❌ | ⚠️ server policy, not portable | ✅ **onchain authority programs** |
| **Canonical code provenance** | ❌ | ⚠️ trusted verifier | ✅ **permissionless program→commit link** |
| **Portable, transferable ownership** | ❌ | ❌ | ✅ **repo as an onchain asset** |
| Censorship resistance | ✅ (mirrors) | ❌ | ⚠️ **history yes, blobs depend on storage** |

**The defensible core value propositions** (Design decision):

1. **A neutral, timestamped, append-only anchor for history and refs.** Anyone can prove *that* a given commit existed at a given time, *that* a branch pointed at a given commit, and *who* authorized it — without trusting the host, because the host is a replicated ledger.
2. **Wallet-based authorship that anyone can verify.** Authorship is not an `author.name` string that anyone can type; it is an Ed25519 signature over the commit object ID by a specific Solana keypair. The signature is verified by the onchain program when the commit enters history.
3. **Programmable repository rules.** A branch's update authority can be a program (Squads multisig, SPL Governance/Realms DAO, a CI-gated policy program) rather than a server. Rules become *self-enforcing and portable* instead of a GitHub setting.
4. **Canonical code provenance for deployed Solana programs.** A permissionless, onchain mapping `program_id → repo → commit → tree → source files → build artifact hash`. This directly answers *"what exact source produced this deployed program?"* — a question that is currently answered by a trusted third party (e.g., OtterSec's verify API) or not at all.
5. **Repositories as first-class onchain objects.** Ownership, permissions, and access can be tokenized, delegated, DAO-controlled, or sold.

### 1.3 Honest limitations (do not over-claim to judges)

- **Git already works offline and is already fast/verifiable.** Forge must not make normal Git operations slower. (Design decision: Git remains the local source of truth; the chain is an *anchor*, not the working store.)
- **Blob availability is not guaranteed by the chain.** Only hashes are anchored. If nobody pins the content-addressed data, history is *provable but not retrievable*. Permanence requires storage incentives (Arweave/Filecoin) or mirroring.
- **Onchain storage is expensive relative to disks.** We must anchor *roots*, not files, and we must batch/compress aggressively.
- **Privacy is fundamentally different.** Everything anchored is public. Private repositories require client-side encryption, which is future work.
- **The chain cannot run Git's diff/merge computation.** Those stay offchain and deterministic; only their *results* (IDs and signatures) are anchored.

---

## 2. Research: prior art and building blocks

### 2.1 Git internals

**Facts.** Git is a content-addressed object store. Every object is serialized as `<type> <byte-length>\0<payload>` and identified by the hash of that serialization. There are four object types ([Git docs, "Git Internals — Git Objects"](https://git-scm.com/book/en/v2/Git-Internals-Git-Objects); [gitdatamodel](https://github.com/git/git/blob/master/Documentation/gitdatamodel.adoc)):

- **blob** — file contents (no filename).
- **tree** — a directory listing. Each entry is `[mode] [name]\0[20-or-32-byte binary object id]`. Modes include `100644` (regular file), `100755` (executable), `120000` (symlink), `040000` (subtree), `160000` (gitlink/submodule) ([Git docs](https://git-scm.com/book/en/v2/Git-Internals-Git-Objects)).
- **commit** — a text object with headers: `tree`, zero or more `parent`, `author`, `committer`, optional `gpgsig`, a blank line, then the message. It references the full directory tree and its parent commit(s) ([gitdatamodel](https://github.com/git/git/blob/master/Documentation/gitdatamodel.adoc)).
- **tag** (annotated) — points at a commit (or other object) with tagger/message; used for releases.

**Tree ordering (critical for determinism).** Tree entries are sorted byte-wise by name, but **subtrees are compared as if the name had a trailing `/`**. Because `/` (0x2F) sorts *after* `\0` (0x00) but before most printable extensions, a file `foo.txt` sorts before a tree `foo/` (`.`, 0x2E < `/`, 0x2F), while a blob named `foo` sorts before a tree named `foo` (`\0` < `/`) ([gitobj SubtreeOrder](https://github.com/git-lfs/gitobj/blob/main/tree.go); [isomorphic-git object models](https://deepwiki.com/isomorphic-git/isomorphic-git/4.2-git-object-models); [GitPython tree.py](https://github.com/gitpython-developers/GitPython/blob/main/git/objects/tree.py)). Getting this wrong changes every tree hash.

**Refs and history.** Branches/tags are just files under `.git/refs` (or packed-refs) containing an object ID. `HEAD` points at a ref. History is a DAG; merges have ≥2 parents. This is exactly a Merkle DAG.

**Git LFS.** Large files are replaced by a small pointer file (`version`, `oid sha256:...`, `size`) and the content is stored on a separate LFS server ([Git LFS spec](https://github.com/git-lfs/git-lfs/blob/main/docs/spec.md)). This is the canonical precedent for our "anchor + offchain content" split — and it proves the pattern is viable in the Git world.

**Why it matters for us.** We should **not reinvent the object model**. Git already gives us deterministic content addressing, Merkle history, and universal tooling. Forge should reuse Git objects and anchor their IDs.

### 2.2 Content-addressed / decentralized storage

**Facts.**

- **IPFS** identifies data by a CID = cryptographic hash of a Merkle DAG node; it is *not* a blockchain and provides *no* permanence by itself — availability depends on nodes pinning/serving the content ([IPFS docs, Merkle DAG](https://docs.ipfs.tech/concepts/merkle-dag); [BitcoinWiki IPFS](https://bitcoinwiki.org/wiki/IPFS)). A 2025-era spec update (IPIP-0499) standardized CID profiles (`unixfs-v1-2025`) so the *same* input produces the *same* CID across implementations ([IPFS Blog, IPIP-0499](https://blog.ipfs.tech/2026-03-reproducible-cids)). Note the ecosystem has had stewardship/funding churn (Interplanetary Shipyard ending sponsored work, Aug 2026) — CIDs remain valid, but hosted gateways are a risk ([BitcoinWiki IPFS](https://bitcoinwiki.org/wiki/IPFS)).
- **Arweave** is a "blockweave" with a **pay-once, store-permanently** endowment model (~200-year projection). Real-time price ~**$36/GB** as of the research date ([ar-fees.arweave.net](https://ar-fees.arweave.net/)). A hard per-transaction payload limit (~10 MB) is reported; bundlers overcome it ([Markets Insider](https://markets.businessinsider.com/news/stocks/most-crypto-is-about-money-arweave-ar-is-about-memory-1036202726)). Risk: permanence depends on the endowment outrunning storage cost and AR value ([Kraken AR report](https://assets-cms.kraken.com/files/51n36hrp/facade/fa29aa9bad855e904fd585717fde45f3bfe61aca.pdf)).
- **Filecoin** is a *rental* market: continuous deals, storage providers can drop data if deals lapse ([Markets Insider](https://markets.businessinsider.com/news/stocks/most-crypto-is-about-money-arweave-ar-is-about-memory-1036202726)).
- **Solana itself** stores account state expensively but has cheap transactional/ledger data via **ZK Compression** (Light Protocol + Helius): compressed PDAs cost roughly **5,000 lamports** vs ~0.0016 SOL for a classic 100-byte PDA ([zkcompression.com](https://www.zkcompression.com/home); [SolanaCompass Light Protocol](https://solanacompass.com/projects/light-protocol)). Trade-offs: validity-proof overhead per tx, specialized RPC (Photon) dependency, and break-even at tens of thousands of accounts ([Soladex](https://www.soladex.io/glossary/zk-compression)).

### 2.3 Solana platform facts (as of research date)

These are the constraints that shape the design. **All from official docs or cited sources.**

| Constraint | Value | Source |
|---|---|---|
| Max account data size | **10 MB** | Solana docs / skill refs |
| Max account growth per instruction | **+10,240 bytes** | [design patterns](https://solana.com/docs) |
| Rent formula | `min_balance = (128 + data_size) × lamports_per_byte` | [Solana "Reduced Rent"](https://solana.com/upgrades/reduced-rent) |
| `lamports_per_byte` (historical) | 6,960 | Solana "Reduced Rent" |
| `lamports_per_byte` (mainnet, Sep 11 2026, epoch 1033) | **5,080** (SIMD-0437 phase 2) | [Gate News](https://www.gate.com/news/detail/solana-account-rent-parameters-drop-to-5080-lamports-in-simd-0437-phase-2-24237691) |
| Target `lamports_per_byte` (Agave 4.4, ~Nov 2026) | **696** (−90%) | [Solana "Reduced Rent"](https://solana.com/upgrades/reduced-rent) |
| Rent is a **refundable deposit** | returned on account close | [Solana "Reduced Rent"](https://solana.com/upgrades/reduced-rent) |
| Legacy/v0 tx size | **1,232 bytes** | [Solana Transactions](https://solana.com/docs/core/transactions) |
| v1 tx size (SIMD-0296/0385) | **4,096 bytes** (opt-in; pending activation around Aug–Sep 2026) | [Solana "Larger Transaction Sizes"](https://solana.com/upgrades/larger-transaction-sizes) |
| v1 requires explicit CU + data limits | defaults are **0** | [Solana "Larger Transaction Sizes"](https://solana.com/upgrades/larger-transaction-sizes) |
| Max accounts per tx | **64** | [Solana Transactions](https://solana.com/docs/core/transactions) |
| Base fee | **5,000 lamports/signature** | [Solana Transactions](https://solana.com/docs/core/transactions) |
| Compute unit limit | 200k/ix default, **1.4M cap** | [Chainstack CU guide](https://docs.chainstack.com/docs/solana-compute-budget) |
| Block CU limit | **100M CU** (SIMD-0286, active Jul 29 2026) | [Solana "100M CU Blocks"](https://solana.com/upgrades/100m-cu-blocks) |
| PDA seeds | ≤16 seeds, ≤32 bytes each, bump 0–255 | [Solana PDA docs](https://solana.com/docs/core/pda) |
| Onchain crypto | Ed25519 verify native program; sha256/keccak256/blake3/poseidon syscalls | Solana skill reference |
| Verified builds | `solana-verify`, pinned Docker images; maps program → Git repo | [solana-verifiable-build](https://github.com/solana-foundation/solana-verifiable-build); [Solana verified builds](https://solana.com/docs/programs/verified-builds) |
| Tooling (2026) | Anchor 1.1.x stable; **Anchor `test`/`localnet` default to Surfpool**; Anchor v2 alpha on Pinocchio; `@solana/kit` v7+; Codama clients | [Anchor 1.0 release notes](https://v2.anchor-lang.com/docs/updates/release-notes/1-0-0); [Anchor v2](https://v2.anchor-lang.com/docs/v1); [Surfpool](https://solana.com/docs/tools/surfpool) |
| Multisig/governance | Squads V4 program `SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf`; SPL Governance/Realms | [SolanaCompass Squads](https://solanacompass.com/projects/squads); [Solana DAOs](https://solana.com/developers/dao) |

**Estimate assumption:** SOL ≈ **$100** for dollar figures (spot was ~$99.90 on 2026-09-13) ([KuCoin](https://www.kucoin.com/news/flash/solana-account-rent-reduction-phase-2-launched-cumulative-drop-of-27)). *Marked uncertain — recompute at current spot.*

### 2.4 Software provenance / verifiable builds

**Facts.** The industry standard for supply-chain provenance is **SLSA**, which defines provenance as verifiable info about *where, when, and how* an artifact was produced, carried inside **in-toto attestations** (signed statements with a subject + predicate) ([SLSA provenance](https://slsa.dev/provenance); [in-toto attestation framework](https://github.com/in-toto/attestation/blob/main/README.md)). **Sigstore/cosign** signs and attests artifacts and uses transparency logs ([Sigstore docs](https://docs.sigstore.dev/cosign/verifying/attestation)). On Solana, **verified builds** rebuild source in pinned Docker images and compare the executable hash to the onchain program, with an API maintained by OtterSec ([Solana verified builds](https://solana.com/docs/programs/verified-builds); [solana-verifiable-build](https://github.com/solana-foundation/solana-verifiable-build)). On EVM chains, **Sourcify** and **Etherscan/Blockscout** provide similar source verification ([Sourcify](https://sourcify.dev/); [Etherscan](https://docs.etherscan.io/contract-verification/get-source-code)).

**Gap we exploit:** existing Solana verification links a program to *a Git repository URL*, verified by a *trusted API*. Forge instead links a program to an **exact commit object ID anchored onchain**, verified **permissionlessly** against the Merkle history. That is a strict improvement in trust-minimization.

### 2.5 Existing decentralized-development projects

#### Radicle
- **What:** Peer-to-peer "sovereign code forge" built on Git; no central server ([Radicle protocol guide](https://radicle.dev/guides/protocol)).
- **Architecture:** Git is the data store; a gossip protocol (**Radicle Link**) replicates repos and social artifacts (issues, patches as **Collaborative Objects / CRDTs**). Identities are Ed25519 **DIDs**; everything (commits, comments, reviews) is cryptographically signed. An **opt-in Ethereum registry** anchors project metadata, and "Radicle Orgs" use Ethereum smart contracts for access control.
- **Onchain:** optional Ethereum registry (identity, project state, orgs).
- **Offchain:** all Git objects + social artifacts, replicated P2P.
- **Consensus/security:** no global consensus for code; authenticity is per-object signatures + gossip; Ethereum for names/governance.
- **Similar because:** content-addressed Git, cryptographic identity, no central host.
- **Should we copy?** **Yes — the local-first, Git-as-database, signed-everything model.** **No — the P2P gossip replication**, which is a large effort and gives no global ordering. Solana gives us cheap global ordering and provenance without a gossip network.

#### Gitopia
- **What:** Cosmos-SDK application chain for decentralized code collaboration ([Gitopia docs](https://docs.gitopia.com/gitopia-architecture/index.html)).
- **Architecture:** app-specific Cosmos chain handles app logic, access control, git **refs**, and metadata; a **decentralized compute infrastructure** unpacks Git packfiles; storage is redundant across **IPFS/Filecoin/Arweave**; a `git-remote-gitopia` helper makes `git push` work ([Gitopia GitHub](https://github.com/gitopia/gitopia)).
- **Onchain:** refs, repo/org/user config, governance.
- **Offchain:** Git objects, packfiles, CI.
- **Similar because:** exactly our domain.
- **Should we copy?** **Yes — the git-remote-helper UX, anchoring refs not files, and the packfile-processing insight** (the chain should never ingest packfiles). **No — its own L1**, which is expensive to bootstrap; Solana gives us an existing security budget and ecosystem.

#### Dgit / git-ssb / git3 (Ethereum) / Forgejo+Forgefed
- **Dgit** (Quorum Control) — decentralized Git *remotes* ([HN](https://news.ycombinator.com/item?id=22684945)).
- **git-ssb** — Git over Secure Scuttlebutt.
- **git3** (Ethereum) — "Git as a service on Ethereum"; anchors repo metadata/refs onchain.
- **Forgejo/Forgefed** — federation for centralized forges; *not* a blockchain approach.
- **Should we copy?** The **remote-helper / mirror** pattern is good UX. The Ethereum git3 approach validates anchoring refs, but Solana's cost/throughput is a better fit for high-frequency ref updates.

#### SourceCred / DAO tooling
- **What:** compute contribution graphs and reputation from repo history; not a version-control layer.
- **Should we copy?** Its **contribution-graph** idea is a compelling future layer on top of our verifiable authorship, but out of MVP scope.

#### Verifiable-build systems (OtterSec API, solana-verify, Sourcify, Sigstore/SLSA)
- **Should we copy?** **Yes:** reuse `solana-verify` to produce the executable hash and adopt in-toto/SLSA-shaped attestation fields. **No:** don't depend on their trusted API as the source of truth — anchor the attestation ourselves.

**Synthesis.** Two patterns recur in the projects that work: (1) **use Git as the object store** and (2) **anchor only roots/refs/metadata onchain**, keeping bulk data offchain. The projects that struggle (P2P gossip, app-specific L1s) pay large infrastructure costs for properties we can get from Solana. Our design follows the two proven patterns and adds the two things nobody has fully delivered on Solana: **onchain signed authorship per commit** and **permissionless program↔commit provenance**.

---

## 3. Architecture overview

### 3.1 High-level diagram

```
┌──────────────────────────────────────────────────────────────────────┐
│                              DEVELOPER                                │
│  forge CLI  (Rust, built on Git via gix/libgit2)                      │
└───────────────┬──────────────────────────────────────────────────────┘
                │
                ▼
┌──────────────────────────────────────────────────────────────────────┐
│                         LOCAL REPOSITORY                              │
│  Git object DB (blobs/trees/commits) + refs                            │
│  .forge/  config, attestations, storage-index, onchain-refs           │
└───────┬───────────────────────────────┬──────────────────────────────┘
        │                               │
        ▼                               ▼
┌───────────────────────┐    ┌──────────────────────────────────────────┐
│  OBJECT / MERKLE      │    │  WALLET SIGNER (Ed25519)                 │
│  TREE ENGINE          │    │  signs commit IDs + tx auth messages     │
│  (Git-native hashing) │    └───────────────┬──────────────────────────┘
└───────────┬───────────┘                    │
            │ object bundle (CAR / git bundle)
            ▼                                │
┌───────────────────────┐                    │
│ CONTENT-ADDRESSED     │                    │
│ STORAGE               │                    │
│  IPFS (hot)           │                    │
│  Arweave (permanent)  │                    │
└───────────┬───────────┘                    │
            │ CID / TXID                     │
            ▼                                ▼
      ┌────────────────────────────────────────────────────────────────┐
      │                     SOLANA PROGRAM (Anchor)                      │
      │  RepositoryAccount · BranchAccount · CommitAccount              │
      │  TagAccount · PermissionAccount · ProgramSourceAttestation      │
      │  Instructions: initialize_repository, create_commit,            │
      │  create_branch, update_branch, merge, create_tag,               │
      │  update_permissions, transfer_repository, anchor_program_source │
      │  Events: CommitCreated, BranchUpdated, TagCreated, ...          │
      └───────────────────────────────┬────────────────────────────────┘
                                      │
                                      ▼
      ┌────────────────────────────────────────────────────────────────┐
      │                    ONCHAIN REPOSITORY STATE                      │
      │  owner · branches · head commits · history root · permissions    │
      │  program→commit provenance attestations                          │
      └────────────────────────────────────────────────────────────────┘
                                      │
                        (optional, read)│
                                      ▼
      ┌────────────────────────────────────────────────────────────────┐
      │  INDEXER (Helius webhooks / Geyser)  →  Web UI / SDK / API      │
      └────────────────────────────────────────────────────────────────┘
```

### 3.2 Component responsibilities

| Component | Responsibility | Trust boundary |
|---|---|---|
| **CLI** | Git-compatible local workflow, build objects, sign, push/pull, verify | Untrusted client (must be verifiable onchain) |
| **Local repo / Git** | Canonical object store and working tree | Local trust |
| **Merkle engine** | Deterministic tree/commit construction; Git-compatible IDs | Deterministic, cross-implementable |
| **Wallet signer** | Ed25519 signatures over commit IDs and auth messages | Holds private key |
| **Content storage** | Availability of object bundles | **Untrusted**; verified by hash on read |
| **Solana program** | Verify signatures, enforce permissions, maintain append-only history + refs | Onchain, trustless |
| **Indexer** | Fast querying of events/accounts; UX only | Untrusted cache, never authoritative |
| **Web / SDK** | Human interfaces | Untrusted client |

### 3.3 Onchain vs offchain boundary (summary)

**Onchain (authoritative, small, immutable or owner-mutated):**
repository identity/owner/config; branch head pointers; **commit IDs, parent IDs, tree roots, author pubkeys, timestamps/slots, signatures**; an **append-only history root**; permissions; governance/rule references; tag refs; **program→commit provenance attestations**.

**Offchain (bulk, content-addressed, verifiable):**
source files and binaries; full Git objects; packfiles; commit messages; build artifacts; documentation; CI attestation payloads; blobs of any size.

**Rule of thumb (Design decision):** *anything a verifier needs to check a signature or reconstruct a root goes onchain; anything a verifier can re-hash goes offchain.*

Full rationale per field is in [§4](#4-data-model) and [§8](#8-storage-architecture).

---

## 4. Data model

Notation: `Pubkey` = 32 bytes, `Oid` = commit/blob/tree object ID (see [§5](#5-hashing--merkle-model); 32 bytes if SHA-256, pad SHA-1 to 32 with a version tag), `Hash` = 32 bytes, `Slot`/`i64` = 8 bytes, `u64` = 8 bytes.

> **Design decision — bind the whole object graph.** Git's commit ID already commits (transitively) to the tree, all blobs, all parent commits, the author/committer strings, and the message. So the onchain record's *authoritative* field is `commit_oid`; the other fields are denormalized for cheap onchain queries and are **re-validated against the commit object** by clients. This is stated explicitly so the program's storage is minimized without weakening guarantees.

### 4.1 Repository

```rust
#[account]
pub struct RepositoryAccount {
    pub owner: Pubkey,           // wallet or program-owned PDA (Squads/DAO)
    pub repo_id: Pubkey,         // == PDA address; stored for indexer convenience
    pub name: [u8; 32],          // owner-scoped name (UTF-8, padded)
    pub default_branch: [u8; 32],
    pub history_root: [u8; 32],  // append-only commitment over all commits (see §5.6)
    pub commit_count: u64,       // monotonic
    pub contributor_count: u32,
    pub storage_backend: u8,     // 0=IPFS, 1=Arweave, 2=hybrid, ...
    pub flags: u16,              // bitfield: require_signed_commits, private, ...
    pub bump: u8,
    pub created_slot: u64,
    pub _reserved: [u8; 64],     // upgrade headroom (Anchor pattern)
}
// ~200 bytes
```

- **PDA seeds:** `[b"repo", owner.as_ref(), name_padded]`. Owner-scoped names avoid global namespace squatting; a global registry can be added later.
- **Lifecycle:** created by `initialize_repository`; `owner` mutable via `transfer_repository`/`update_permissions`.
- **Why onchain:** identity, ownership, config, and `history_root` must be globally agreed and tamper-evident. Full source must not be.

### 4.2 Commit

```rust
#[account]
pub struct CommitAccount {
    pub repo: Pubkey,
    pub commit_oid: [u8; 32],     // Git commit ID (authoritative)
    pub parent_count: u8,         // 0 = root, 1 = normal, 2 = merge
    pub parent_a: [u8; 32],       // == 0 if none
    pub parent_b: [u8; 32],       // merge second parent, else 0
    pub tree_oid: [u8; 32],       // top-level tree root
    pub author: Pubkey,           // wallet that signed commit_oid
    pub authored_at: i64,         // commit timestamp (seconds)
    pub message_hash: [u8; 32],   // hash of message; message itself offchain
    pub attestation_hash: [u8; 32], // hash of the offchain signed attestation
    pub seq: u64,                 // position in repo history (== commit_count at insert)
    pub bump: u8,
}
// ~288 bytes
```

- **PDA seeds:** `[b"commit", repo.as_ref(), commit_oid]`. Deterministic, one account per commit (MVP; scalable alternatives in [§10](#10-cost--scalability-analysis)).
- **Why onchain:** this is the record that makes authorship and provenance *independent of the host*. Message is hashed (not stored) because it is bulk-ish and can be arbitrarily large.

### 4.3 Tree & Blob

Trees and blobs are **not stored onchain**; they are Git objects addressed by OID.

```
Tree  := Git tree object  (mode, name, 20/32-byte entry ID)*   // see §5.3
Blob  := Git blob object  (raw file bytes)
```

- **Why offchain:** a single repo's tree graph easily exceeds 10 MB and would cost enormous rent. Hashes are committed transitively by `commit_oid` and `tree_oid`, so onchain storage would add no integrity.

### 4.4 Branch

```rust
#[account]
pub struct BranchAccount {
    pub repo: Pubkey,
    pub name: [u8; 64],
    pub head_commit: [u8; 32],      // current tip OID (0 for empty branch)
    pub head_seq: u64,              // monotonic update counter (replay/concurrency)
    pub authority: Pubkey,          // wallet OR program-owned PDA (rules, §16)
    pub permissions_mode: u8,       // 0=owner-only, 1=allowlist, 2=authority-PDA
    pub protected: u8,              // bit0 require signed commits, bit1 require CI
    pub bump: u8,
    pub updated_slot: u64,
    pub _reserved: [u8; 32],
}
// ~224 bytes
```

- **PDA seeds:** `[b"branch", repo.as_ref(), name_padded]`.
- **Design decision — head is mutable, history is not.** Branch accounts are the *only* mutable refs; updates are logged via events and via `history_root`. This preserves Git semantics (branches move) while making erasure detectable.

### 4.5 Tag

```rust
#[account]
pub struct TagAccount {
    pub repo: Pubkey,
    pub name: [u8; 64],
    pub target_commit: [u8; 32],
    pub tagger: Pubkey,
    pub message_hash: [u8; 32],
    pub created_slot: u64,
    pub signed: u8,                 // 1 if tagger signature is anchored
    pub bump: u8,
}
// ~168 bytes
```

- **PDA seeds:** `[b"tag", repo.as_ref(), name_padded]`. Immutable after creation (delete + recreate forbidden by the `init` uniqueness). Tags are the natural anchor point for **release artifacts and provenance**.

### 4.6 Permission / Contributor

```rust
#[account]
pub struct PermissionAccount {
    pub repo: Pubkey,
    pub contributor: Pubkey,
    pub role: u8,          // 0=reader, 1=writer, 2=maintainer, 3=admin
    pub granted_slot: u64,
    pub expires_slot: u64, // 0 = never
    pub bump: u8,
}
// ~48 bytes
```

- **PDA seeds:** `[b"perm", repo.as_ref(), contributor.as_ref()]`. Separate account keeps `RepositoryAccount` fixed-size (no realloc) and enables parallel writes per contributor. MVP may encode a small allowlist inline and skip this account; separate accounts are the scale path.

### 4.7 Program-source attestation (provenance)

```rust
#[account]
pub struct ProgramSourceAttestation {
    pub program_id: Pubkey,
    pub repo: Pubkey,
    pub commit_oid: [u8; 32],
    pub artifact_hash: [u8; 32],        // executable hash from solana-verify
    pub build_metadata_hash: [u8; 32],  // toolchain image digest + Cargo.lock etc.
    pub attester: Pubkey,               // who asserted the link (repo owner/verifier)
    pub verified: u8,                   // 0=claimed, 1=independently verified
    pub created_slot: u64,
    pub bump: u8,
}
// ~184 bytes
```

- **PDA seeds:** `[b"prog", program_id.as_ref()]`. See [§12](#12-code-provenance).

### 4.8 Example JSON views (SDK-level)

```jsonc
// Repository
{ "repoId": "7xK...", "owner": "9aB...", "name": "forge",
  "defaultBranch": "main", "historyRoot": "0x…", "commitCount": 142,
  "storageBackend": "hybrid", "createdSlot": 342100000 }

// Commit
{ "commitOid": "f3a9…", "repo": "7xK...", "parents": ["c81b…"],
  "treeRoot": "9d02…", "author": "9aB…", "authoredAt": 1790001234,
  "messageHash": "0x…", "seq": 141, "signature": "4f…", "attestationCid": "bafy…" }

// Branch
{ "name": "main", "headCommit": "f3a9…", "headSeq": 141,
  "authority": "9aB…", "permissionsMode": "owner-only", "protected": true }

// Tag
{ "name": "v1.0.0", "targetCommit": "f3a9…", "tagger": "9aB…", "signed": true }
```

---

## 5. Hashing & Merkle model

**Determinism is a protocol requirement.** Two independent implementations must produce byte-identical IDs. We achieve this by **reusing Git's exact object serialization** for blobs/trees/commits, and defining a small canonical format only for the *attestation* and *history commitment*.

### 5.1 Object ID (reuse Git)

```
oid(object) = H( "<type> " + decimal_byte_length(type,payload) + "\0" + payload )

blob   payload = raw bytes
tree   payload = sorted entries (see §5.3)
commit payload = text headers + "\n" + message
tag    payload = text headers + "\n" + message
```

- `H` = **SHA-256** for Forge-native repos; **SHA-1** for Git-compatibility mode. We tag the algorithm (`sha256:` / `sha1:`) to avoid cross-algorithm confusion. **Design decision:** default new repos to SHA-256 to avoid SHA-1 collision classes; still *read* SHA-1 repos for interoperability. Git supports SHA-256 repositories ([Git data model](https://github.com/git/git/blob/master/Documentation/gitdatamodel.adoc); [singhajit](https://singhajit.com/how-git-stores-data-internally)).
- **Domain separation** is inherent: the type string (`blob`/`tree`/`commit`/`tag`) is part of the hashed header, so an object cannot be reinterpreted as another type.

### 5.2 Blob

```
file bytes  ──H──▶  blob_oid
```

Identical bytes anywhere ⇒ identical blob OID. Deduplication is automatic.

### 5.3 Tree (deterministic directory)

```
for each entry in directory:
    line = mode_ascii + b" " + name_bytes + b"\0" + entry_oid_raw
sort entries:
    key(name, is_tree) = name_bytes + (is_tree ? b"/" : b"")
    sort by key lexicographically (unsigned byte order)
tree_payload = concat(lines)
tree_oid = H("tree " + len(tree_payload) + "\0" + tree_payload)
```

- **Path validation (security):** reject entry names containing `/`, NUL, or equal to `.`/`..`; reject absolute paths; normalize Unicode to NFC before hashing so implementations agree. (Git itself permits unusual names; Forge enforces a safe subset for onchain-anchored trees — *Design decision, documented deviation*.)
- **Symlinks:** represent as Git mode `120000`; the *target string* is the blob content and is **never resolved by the protocol**. Submodules (mode `160000`) are allowed only as commit links, not as URLs, to avoid network side effects.

### 5.4 Commit

```
commit_payload =
  "tree "    + hex(tree_oid)   + "\n" +
  ("parent " + hex(parent_oid) + "\n")* +
  "author "  + author_identity + " " + unix_ts + " " + tz + "\n" +
  "committer " + committer_identity + " " + unix_ts + " " + tz + "\n" +
  "\n" + message_bytes
commit_oid = H("commit " + len(commit_payload) + "\0" + commit_payload)
```

- **Author identity** is a Git identity string for display/interop, but **authorship is defined by the wallet signature over `commit_oid`** ([§6](#6-commit-model)) — not by the string.
- **Canonical timezone/encoding:** ASCII identity, UTF-8 message, LF endings, no trailing whitespace normalization puzzles. Two implementations must emit the same bytes.

### 5.5 Forge attestation (what the wallet signs)

To avoid a chicken-and-egg (a signature inside the commit would change its OID), the signature lives in a **sidecar attestation**:

```jsonc
// canonical CBOR (deterministic map ordering) or canonical JSON (JCS)
{
  "v": 1,
  "repo": "<base58 repo PDA>",
  "commit": "sha256:<hex commit_oid>",
  "parents": ["sha256:<hex>", ...],       // sorted? no: first-parent order preserved
  "tree": "sha256:<hex>",
  "author": "<base58 wallet pubkey>",
  "authoredAt": 1790001234,
  "messageHash": "sha256:<hex>",
  "nonce": "<base58 random 16 bytes>"     // replay/side-channel resistance
}
```

```
attestation_hash = H("forge-attestation\0" + canonical(attestation))
signature        = Ed25519_sign(wallet_sk, attestation_hash)
```

The onchain program verifies `signature` over `attestation_hash` (via the Ed25519 native verifier + Instructions sysvar, [§9.4](#94-ed25519-verification-pattern)).

### 5.6 Repository history root (append-only)

**Design decision.** The repository maintains an append-only commitment so that *previously seen commits can never be erased*, even by a branch force-push:

```
history_root_0   = H("forge-genesis\0" + repo_pda)
history_root_n   = H("forge-append\0" + history_root_{n-1} + commit_oid_n + seq_n)
```

- Stored in `RepositoryAccount.history_root`, updated on every accepted `create_commit` (**not** on branch updates).
- **Inclusion proof (MVP):** the full sequence of commit OIDs; verify by recomputing the chain. O(n), acceptable for a hackathon demo.
- **Inclusion proof (scale):** replace the hash chain with a **Merkle Mountain Range / append-only Merkle log** (à la Certificate Transparency) for O(log n) proofs while keeping O(1) append ([SLSA distribution/provenance log pattern](https://github.com/slsa-framework/slsa/blob/main/spec/distributing-provenance.md)). *Future work; noted in [§25](#25-open-questions).*
- **Repository root hash (for tagging/verification):** `repo_root = H("forge-repo\0" + repo_pda + default_branch_head + history_root + commit_count)` gives a single 32-byte fingerprint of the entire anchored state.

### 5.7 Worked example (small)

```
README.md   content "hello\n"      → blob_oid B1
src/main.rs content "fn main(){}\n" → blob_oid B2

tree src/      = H("tree 33\0" + "100644 main.rs\0"+B2)        = T2
tree root      = H("tree …\0" + "100644 README.md\0"+B1 + "40000 src\0"+T2) = T1
     (note: "README.md" < "src/" byte-wise; and any "src.txt" would sort before "src/")

commit #1 (root): H("commit …\0" + "tree T1\n…") = C1
commit #2:        H("commit …\0" + "tree T1\nparent C1\n…") = C2
history_root_2 = H("forge-append\0" + H("forge-append\0" + genesis + C1 + 0) + C2 + 1)
```

Two implementations that agree on serialization will produce identical `B1,B2,T1,T2,C1,C2,history_root`.

### 5.8 Collision & algorithm considerations

- **SHA-1 is collision-broken for adversarial inputs** (chosen-prefix attacks are practical; Git itself is migrating to SHA-256) — hence Forge-native repos use SHA-256.
- For SHA-1 interop repos, Forge additionally records a **SHA-256 "forge digest"** over the full object graph during `push`, so provenance claims can rely on SHA-256 even for legacy repos. *(Design decision; adds work proportional to repo size and may be sampling-based for large repos in MVP.)*
- **No custom crypto.** Use the runtime's audited `sha256`/`keccak256` syscalls and Ed25519 verifier ([Solana runtime concepts](https://solana.com/docs)).

---

## 6. Commit model

### 6.1 Lifecycle

```
forge add <paths>
   → stage files (Git index)
forge commit -m "msg"
   → build blobs (dedup) → build trees bottom-up → tree_oid
   → read parent = branch head (0/1 parents)
   → assemble Git commit object → commit_oid
   → build canonical Forge attestation
   → attestation_hash = H(attestation)
   → signature = Ed25519_sign(wallet, attestation_hash)
   → store attestation in .forge/attestations/<commit_oid>.cbor
forge push [origin] [branch]
   → upload missing objects (CAR / git bundle) to CAS
   → tx: [ed25519_verify(attestation_hash, sig, author), create_commit(...)]
   → tx: [update_branch(head=commit_oid, expected=head_seq)]
   → onchain validates sig + parent linkage + permissions
   → commit becomes part of anchored history (history_root advances)
```

### 6.2 How commits are created
Deterministic local construction exactly as Git does, plus the sidecar attestation. No network needed to commit.

### 6.3 How commits are signed
Ed25519 over `attestation_hash` with the wallet key. The attestation binds repo, commit, parents, tree, author, time, and a random nonce. Private keys never leave the wallet/CLI; the program never sees them.

### 6.4 How parent commits are validated
On `create_commit`:
- `parent_count == 0` ⇒ `history_root == genesis` (only the first commit may be a root).
- `parent_count >= 1` ⇒ every parent must **already exist** as a `CommitAccount` PDA for this repo (or equal the current branch head being advanced).
- Reject self-parent and cycles: `commit_oid != parent_*`.
- For `update_branch`, the new head's first parent must equal the current head (fast-forward) **or** the update must be a merge where one parent == current head ([§7](#7-branches--merging)).

### 6.5 Forks
A fork is simply a second branch (or a second repo derived from a commit). Creating `alice/feature` from `main` writes a `BranchAccount` whose head is an existing commit. No object copying onchain.

### 6.6 Merge commits
Two parents (`parent_a` = target branch head, `parent_b` = source branch head). The merge commit is created like any commit; the program accepts a 2-parent head update even though it is not a fast-forward, provided both parents are known commits and the merger is authorized. Merge *content* is computed offchain (Git) and committed via `tree_oid`; the chain never performs the merge.

### 6.7 Duplicate commits
`CommitAccount` PDA is keyed by `(repo, commit_oid)`; a second `create_commit` for the same OID fails at `init` (account exists). This is **idempotent by construction**. Clients treat "already exists" as success if fields match. Because content addressing is global, the same change on two branches shares one commit account.

### 6.8 Replay attacks
- **Commit replays:** the attestation includes `repo`, `commit`, `author`, and a 128-bit `nonce`; the program binds verification to `repo` and `commit_oid`. A signature for repo A cannot be replayed into repo B.
- **Branch-update replays:** `update_branch` takes `expected_head_seq`; the program requires `expected_head_seq == branch.head_seq` and increments. Replaying an old signed update fails because `head_seq` has advanced (**optimistic concurrency**). The signed auth message for a branch update includes `(repo, branch_name, new_head, expected_head_seq)`.
- **Cross-chain / tx replays:** Solana's recent-blockhash expiry (150 slots) plus `head_seq` make naive transaction replay inert; the signed messages carry domain-separated prefixes (`forge-...`) to prevent cross-protocol reuse ([Solana Transactions](https://solana.com/docs/core/transactions)).

### 6.9 Malicious commits
- **Spoofed author:** impossible — author signature is verified over the commit ID.
- **Garbage/no-parent head:** rejected (parent must exist, first commit rules).
- **Unapproved author:** permission check (owner/allowlist/branch authority).
- **Path-traversal/symlink payloads:** rejected at tree construction ([§5.3](#53-tree-deterministic-directory)) and flagged at verification.
- **Large/abusive commits:** bounded by client-side size policy; onchain we store only hashes, so the chain cannot be bloated by file size. A future `max_commit_metadata` rule can cap tree depth/entry counts.

---

## 7. Branches & merging

### 7.1 Model

```
main        → C10
alice/feature → C14   (parent C10)
bob/feature   → C17   (parent C10)
merge(C14, C17) → C18
```

Branch state lives in `BranchAccount`. Branch creation requires either repo-owner rights or a permission allowing it.

### 7.2 Operations

| Operation | Onchain effect | Checks |
|---|---|---|
| **Create branch** | `init` `BranchAccount(name, from_commit, authority)` | signer authorized; `from_commit` exists (or empty) |
| **Fast-forward update** | `head = new_head; head_seq += 1` | `new_head.parent_a == head`; authority; sig verified |
| **Non-fast-forward update** | rejected unless `reset_branch` (explicit, logged) | prevents accidental history loss |
| **Merge** | `head = merge_commit; head_seq += 1` | 2 parents; one == head; authority; sig |
| **Delete branch** | `close = owner`; record old head in event | authority; cannot delete default branch |

### 7.3 Fast-forward vs non-fast-forward
- **Fast-forward:** new commit's (first) parent equals the current head. Always allowed for authorized writers.
- **Non-fast-forward:** a rewrite. **Design decision:** require a distinct instruction `reset_branch` that (a) requires maintainer/admin role, (b) emits a `BranchReset { old_head, new_head, actor }` event, and (c) does **not** roll back `history_root`. This makes history rewriting visible and non-erasable while still permitting legitimate rebases.

### 7.4 Concurrent updates, races, optimistic concurrency
Two clients racing to push to `main`:
1. Both read `head_seq = N`.
2. Both sign `update_branch(new_head, expected_seq = N)`.
3. First tx lands: `head_seq → N+1`.
4. Second tx fails (`StaleBranchHead`); client refetches, rebases/merges locally, re-signs, retries.

This is the standard compare-and-swap approach and gives Git-like "non-fast-forward rejected, pull first" behavior, enforced onchain.

### 7.5 Branch ownership & permissions
- `permissions_mode` ∈ {owner-only, allowlist, authority-PDA}.
- `authority` may be a wallet or a **program-owned PDA** (e.g., Squads vault or Realms governance). If a PDA, `update_branch` must be invoked such that the authority PDA signs — which happens via CPI from the governing program. This is the hook for [§16](#16-programmable-repository-rules).

### 7.6 Should branch heads be mutable onchain refs?
**Yes (Design decision).** Branches are inherently mutable; making them immutable would break Git semantics. We make them *controlled and observable*: every update bumps `head_seq`, emits an event, and the append-only `history_root` guarantees that no commit ever leaves the log. Immutability lives in the **history log**, mutability in the **ref** — exactly like Git, but globally anchored.

---

## 8. Storage architecture

### 8.1 The options

| | **A. Files on Solana** | **B. IPFS + Solana** | **C. Arweave + Solana** | **D. Hybrid (recommended)** |
|---|---|---|---|---|
| **Cost** | catastrophic: rent per byte; 10 MB cap; 10 KB/ix growth | cheap/free tiers; pinning subscription | ~**$36/GB**, one-time ([fees](https://ar-fees.arweave.net/)) | IPFS hot + Arweave for tags/checkpoints |
| **Permanence** | high while funded | **none** without pins | high (endowment risk) | good |
| **Availability** | blockchain-available | depends on pinning/gateways | depends on miners + gateways | multi-provider |
| **Retrieval speed** | fast (RPC) | variable (DHT/gateway) | variable | fast local GC + cache |
| **Decentralization** | maximal | medium | medium-high | pragmatic |
| **Impl. complexity** | low | low-medium | medium (bundler, 10 MB limit) | medium |
| **Hackathon feasibility** | ❌ | ✅ | ✅ | ✅ |

### 8.2 Analysis

**Option A (files on Solana)** is a non-starter. Even with the September 2026 rent cut (5,080 lamports/byte), storing a 1 MB file costs `(128 + 1,048,576) × 5080 ≈ 5.33B lamports ≈ 5.33 SOL ≈ $533` (**Estimate**, [rent formula](https://solana.com/upgrades/reduced-rent)); at the target 696 lamports/byte it is still ~$73/MB. A single repo of 100 MB is thousands of dollars, and no single account can exceed 10 MB. **Reject.**

**Option B (IPFS + Solana)** is the natural fit: Git objects are already content-addressed, so their OIDs map cleanly onto CIDs, and we only anchor OIDs onchain. But **IPFS by itself has no permanence** — if nobody pins, the data is gone, and history becomes "provable but hollow." Use it as *hot* storage with multiple pins and treat availability as best-effort. ([IPFS docs](https://docs.ipfs.tech/concepts/merkle-dag); [BitcoinWiki](https://bitcoinwiki.org/wiki/IPFS).)

**Option C (Arweave + Solana)** buys permanence for a one-time fee and is excellent for **release checkpoints, tags, and provenance artifacts**. Two caveats: reported ~10 MB per-transaction payload limit (use a **bundler** such as Irys/ArDrive to aggregate), and endowment-model risk if storage costs outrun AR value ([Kraken AR report](https://assets-cms.kraken.com/files/51n36hrp/facade/fa29aa9bad855e904fd585717fde45f3bfe61aca.pdf); [Markets Insider](https://markets.businessinsider.com/news/stocks/most-crypto-is-about-money-arweave-ar-is-about-memory-1036202726)). **Reject as the sole store; adopt for permanence.**

### 8.3 Recommendation — Option D (hybrid)

**Design decision.**
- **Hot layer: IPFS** (or a plain content-addressed object server) for clone/fetch speed and free-ish pinning. Objects are uploaded as Git object bundles/CARs, content-addressed by their **Git OIDs** (so retrieval is verified on read: recompute `oid(blob)` and compare).
- **Cold/permanent layer: Arweave** for (a) annotated tags/releases, (b) periodic history checkpoints, and (c) provenance artifacts. Uploaded via a bundler to bypass the per-tx size limit.
- **Onchain:** only the `RepositoryAccount.storage_backend` + a per-commit/tag **storage hint** (optional CID/URI hash) so clients know where to look. **Never trust the hint**; always verify by hash.
- **Availability policy:** on `push`, the client uploads to ≥2 independent pinning services (and optionally Arweave for tags). The CLI can `forge gc --verify-availability` to report unbacked objects.

> **Why not just use IPFS?** Because permanence is a core promise of a *provenance* protocol. Arweave's pay-once model makes "the release source will still exist" credible; IPFS alone makes it a hope. Hybrid gets speed now and durability where it matters.

---

## 9. Solana program architecture

Framework: **Anchor 1.1.x** for the MVP (IDL, fast iteration, mature testing); native/Pinocchio is a future performance path ([Anchor release notes](https://v2.anchor-lang.com/docs/updates/release-notes/1-0-0); [Anchor v2](https://v2.anchor-lang.com/docs/v1)).

### 9.1 Accounts (do we need each?)

| Account | Needed? | Why | Approx. size |
|---|---|---|---|
| `RepositoryAccount` | **Yes (core)** | identity, owner, config, history root | ~200 B |
| `BranchAccount` | **Yes (core)** | mutable refs | ~224 B |
| `CommitAccount` | **Yes for MVP** | per-commit authorship record; scalable alternatives in §10 | ~288 B |
| `PermissionAccount` | Should-have | per-contributor roles, scale path | ~48 B |
| `TagAccount` | Nice-to-have | immutable releases | ~168 B |
| `ProgramSourceAttestation` | Should-have (differentiator) | provenance link | ~184 B |
| `RepositoryConfig`/rule accounts | Future | programmable rules | — |

**Events vs account data (Design decision).** Critical, queryable state (heads, authorship, provenance) goes in **account data**. High-frequency, append-only, non-consensus-critical notifications (e.g., "commit created", "branch updated") are **also emitted as events** for indexers. Per the Solana security guidance, **events (preferably `emit_cpi!`/noop-CPI) are preferred over string logs**, and logs must never be parsed as authoritative ([Solana design patterns](https://solana.com/docs)). Use a per-repo `commit_count` as an event sequence number so indexers can detect gaps/ordering ([design patterns](https://solana.com/docs)).

### 9.2 Instructions

Notation: **inputs** = ix args; **accounts** = required accounts; **auth** = authorization; **fails** = failure cases.

#### `initialize_repository(name: [u8;32], default_branch: [u8;32], storage_backend: u8, flags: u16)`
- **Accounts:** `owner` (signer, mut), `repository` (init, PDA `["repo", owner, name]`), `default_branch_account` (init, PDA `["branch", repo, name]`), `system_program`.
- **Validation:** name length/charset; name not empty; bump canonical.
- **State:** create repo + empty default branch; `history_root = genesis`; `commit_count = 0`.
- **Auth:** `owner` signer (pays rent).
- **Fails:** name taken for this owner (PDA exists); invalid name.

#### `create_commit(commit_oid, parent_count, parent_a, parent_b, tree_oid, authored_at, message_hash, attestation_hash)`
- **Accounts:** `author` (signer, mut, pays), `repository` (mut), `commit` (init, PDA `["commit", repo, commit_oid]`), `parent_a_acc` (optional, read), `parent_b_acc` (optional, read), `instructions_sysvar`, `ed25519_program`.
- **Validation:** repo exists; `authorized(repository, author)`; `parent_count ∈ {0,1,2}`; parents exist (or equal a known head) and are not self; if `parent_count==0`, repo must be empty; Ed25519 signature over `attestation_hash` by `author` verified (see §9.4).
- **State:** create `CommitAccount(seq = repo.commit_count)`; `repo.commit_count += 1`; `repo.history_root = append(prev, commit_oid, seq)`.
- **Auth:** `author` signer; signature verified.
- **Fails:** duplicate commit, bad parent, unauthorized author, bad signature, non-root with no parent, root on non-empty repo.

#### `create_branch(name, from_commit, authority)`
- **Accounts:** `authority` (signer), `repository`, `branch` (init, PDA `["branch", repo, name])`, `from_commit_acc` (optional read).
- **Validation:** `authorized` to create branches; `from_commit` exists or zero; name unique.
- **State:** create branch (head = `from_commit`, `head_seq = 0`).
- **Fails:** duplicate name, unknown commit, unauthorized.

#### `update_branch(new_head, expected_head_seq, auth_nonce)`
- **Accounts:** `authority` (signer **or** expected PDA), `repository`, `branch` (mut), `new_commit_acc` (read), `instructions_sysvar`, `ed25519_program`.
- **Validation:** `branch.head_seq == expected_head_seq` (**optimistic concurrency**); `authorized(branch, authority)`; `new_commit` exists in repo; relationship is fast-forward or merge (see §7.2); signature over `(repo, branch, new_head, expected_head_seq)` verified.
- **State:** `branch.head_commit = new_head`; `branch.head_seq += 1`; `updated_slot`.
- **Fails:** stale head, non-fast-forward (unless merge), unauthorized, unknown commit, bad signature.

#### `merge(source_branch, target_branch, merge_commit, expected_target_seq)`
- **Accounts:** `authority` (signer), `repository`, `target` (mut), `merge_commit_acc` (read). (Source head is read offchain / via remaining accounts.)
- **Validation:** `merge_commit.parent_a == target.head`, `parent_b == source.head` (or vice-versa); both parents exist; `expected_target_seq` matches; signature verified; authority.
- **State:** advance `target` to `merge_commit`.
- **Fails:** parents don't match heads, stale, unauthorized, bad signature.

#### `create_tag(name, target_commit, message_hash, signature)`
- **Accounts:** `tagger` (signer), `repository`, `tag` (init).
- **Validation:** authorized; target commit exists; `init` prevents overwrite (immutable).
- **State:** create `TagAccount`; optional Arweave checkpoint upload is a client step (store return TXID hash via a follow-up `set_tag_storage` or include in this ix).

#### `transfer_repository(new_owner)`
- **Accounts:** `owner` (signer), `repository` (mut).
- **Validation:** signer == current owner (or authorized admin).
- **State:** `repo.owner = new_owner`; emit `RepositoryTransferred`.

#### `update_permissions(contributor, role, expires_slot)`
- **Accounts:** `admin` (signer), `repository`, `permission` (init_if_needed **is forbidden**; use explicit create/update paths).
- **Validation:** admin role; valid role enum.
- **State:** create/update `PermissionAccount`; emit `PermissionChanged`.
- **Note:** MVP may restrict to owner-only writes and defer this.

#### `anchor_program_source(program_id, commit_oid, artifact_hash, build_metadata_hash)`
- **Accounts:** `attester` (signer), `repository`, `attestation` (init, PDA `["prog", program_id]`), `program` (read, verify owner == loader).
- **Validation:** attester authorized for repo; `commit_oid` exists in repo history; program account owned by the upgradeable loader; artifact hash non-zero.
- **State:** create `ProgramSourceAttestation{verified:0}`.
- **Fails:** program not found, commit not in repo, unauthorized, attestation already exists.

#### `set_program_verified(program_id)` *(future, separate verifier authority)*
- Marks an attestation `verified=1` after an independent rebuild. Kept separate so claims and verification are distinct trust levels.

### 9.3 Why this account/instruction split
- **Roots onchain, bulk offchain.** Every instruction stores only hashes/IDs.
- **One mutable ref path** (`update_branch`) keeps concurrency reasoning local and uses `head_seq` CAS.
- **Append-only history** is a single cheap field update, not a per-commit chain store — so the program cannot be bribed into forgetting old commits.
- **Provenance is a first-class instruction**, not an afterthought.

### 9.4 Ed25519 verification pattern

A Solana program cannot natively call "verify this signature" from arbitrary bytes; the standard pattern is transaction introspection:
1. The client prepends the **Ed25519 native program** instruction with `(pubkey, signature, message = attestation_hash)`.
2. The Forge instruction is passed the **Instructions sysvar**.
3. Inside, load the previous instruction via the sysvar, assert `program_id == Ed25519SigVerify111111111111111111111111111`, parse its (bincode) data, and assert the pubkey/signature/message match the expected values **and** that the message binds `repo` + `commit_oid`/branch update.
4. Cached/duplicate-signature footguns: assert exactly one ed25519 instruction, correct offsets, and non-empty message.

This is the same family of technique used for onchain signature checks generally and uses the runtime's audited native verifier ([Solana runtime concepts](https://solana.com/docs); [security checklist](https://solana.com/docs)). **MVP simplification:** verify only the **head commit** signature onchain and verify ancestors' signatures client-side during `forge verify`; verifying every historical signature onchain is a scale cost, not a security requirement, because the history root binds them.

### 9.5 Compute budget
`create_commit` does: account init + hash-chain update + optional parent reads + introspection. Expected well under the 200k CU default per instruction; measure with `sol_log_compute_units!` / Surfpool CU profiling and set `SetComputeUnitLimit` from simulation ([Chainstack CU guide](https://docs.chainstack.com/docs/solana-compute-budget); [Surfpool](https://www.surfpool.run/)). One push touches a shared `RepositoryAccount` (`history_root`, `commit_count`) — a **write-lock chokepoint** for a hot repo ([design patterns](https://solana.com/docs)). Mitigation for scale: shard history into per-epoch checkpoint accounts or move history into an append-only log PDA updated less frequently.

---

## 10. Cost & scalability analysis

All figures are **Estimates** using cited constants and the assumption **SOL ≈ $100** (uncertain). Rent is a **refundable deposit**, not a fee ([Solana Reduced Rent](https://solana.com/upgrades/reduced-rent)); it is returned when accounts close. Transaction base fee is **5,000 lamports/signature** ≈ $0.0005 ([Solana Transactions](https://solana.com/docs/core/transactions)); priority fees vary and are excluded.

### 10.1 Per-account rent

`min_balance = (128 + data_size) × lamports_per_byte` ([Reduced Rent](https://solana.com/upgrades/reduced-rent)).

| Account | Size | Rent @6,960 (old) | Rent @5,080 (now) | Rent @696 (target) |
|---|---|---|---|---|
| Repository | 200 B | 0.00228 SOL (~$0.23) | 0.001666 SOL (~$0.17) | 0.000228 SOL (~$0.023) |
| Branch | 224 B | 0.00245 SOL | 0.001788 SOL (~$0.18) | 0.000245 SOL |
| Commit | 288 B | 0.00290 SOL | **0.002113 SOL (~$0.21)** | **0.000290 SOL (~$0.029)** |
| Permission | 48 B | 0.000766 SOL | 0.000458 SOL | 0.000063 SOL |
| Tag | 168 B | 0.00208 SOL | 0.001408 SOL | 0.000190 SOL |
| Provenance | 184 B | 0.00217 SOL | 0.001490 SOL | 0.000201 SOL |

Worked example: 288 B → `(128+288)=416`; `416 × 5,080 = 2,113,280 lamports = 0.00211328 SOL`.

### 10.2 Per-commit account approach (MVP)

| Commits | Rent @5,080 (SOL) | @5,080 USD | Rent @696 (SOL) | @696 USD | Base fees (10k) |
|---:|---:|---:|---:|---:|---:|
| 10 | 0.0211 | **~$2.11** | 0.00290 | ~$0.29 | $0.005 |
| 100 | 0.2113 | **~$21.13** | 0.02896 | ~$2.90 | $0.05 |
| 1,000 | 2.1133 | **~$211.33** | 0.28954 | ~$28.95 | $0.50 |
| 10,000 | 21.133 | **~$2,113** | 2.8954 | **~$289.5** | $5.00 |

Plus per-push tx fees (2 signatures ≈ 10,000 lamports ≈ $0.001 each) — negligible relative to rent. Repository creation ≈ one repo + one branch ≈ **$0.35** now (**recoverable**). This is perfectly fine for a hackathon demo (10–100 commits) but **not** for large repos.

### 10.3 Alternative designs

| Design | 10k-commit state cost | Permanence of every commit | Queryability | Complexity | Verdict |
|---|---:|---|---|---|---|
| **Per-commit account** | ~$2,113 now / ~$290 at target rent | full onchain record | best (`getProgramAccounts`) | low | **MVP** |
| **Events + indexer** | ~$5 (tx fees only; no rent) | depends on indexer/RPC retention | needs indexer | medium | **Scale path** |
| **Append-only log PDA** | one 10 MB account ≈ $5,327 now / ~$730 target; holds ~65k compact records | yes | needs decode | medium; realloc pain (10 KB/ix) | niche |
| **ZK-compressed commit accounts** | ~5,000 lamports each ⇒ 10k ≈ **0.05 SOL ≈ $5** | yes (on ledger) | needs Photon RPC | high (proofs) | **future** |
| **Checkpoint roots only** | ~$0.02 per checkpoint | history provable, data offchain | offchain | low | complement |

**ZK-compression source:** compressed PDAs cost ~**5,000 lamports** and a 100-byte PDA ~0.000015 SOL, a ~99% reduction ([zkcompression.com](https://www.zkcompression.com/home); [SolanaCompass](https://solanacompass.com/projects/light-protocol)); caveats: per-tx validity-proof overhead, specialized RPC (Photon), and break-even at tens of thousands of accounts ([Soladex](https://www.soladex.io/glossary/zk-compression)).

### 10.4 Recommendation (Design decision)
1. **MVP:** per-commit `CommitAccount` + branch heads + append-only `history_root`. Simple, queryable, demonstrable; cost acceptable at demo scale.
2. **Scaling step 1:** stop creating per-commit accounts; **emit `CommitCreated` events** and rely on an **indexer**; keep branch heads + `history_root` onchain. Every commit remains *provable* because the history root binds it and offchain data is hash-verified. Add **periodic checkpoints** (every N commits or on tags) as a single onchain root.
3. **Scaling step 2:** move the commit log to **ZK-compressed accounts** or an append-only Merkle log for O(log n) inclusion proofs and ~$0.0005/commit.
4. **Never** store files onchain.

**Fetch/scale note:** `getProgramAccounts` with `memcmp` on the repository field is the MVP query path; at scale use an indexer because program-account scans are heavy and RPC providers rate-limit them.

---

## 11. Security model

Format: **Attack → Impact → Mitigation → MVP/Future.** Mitigations are only marked MVP if they are genuinely implementable in the hackathon build.

| # | Attack | Impact | Mitigation | Status |
|---|---|---|---|---|
| 1 | **Forged commits** (fake author) | false provenance | Wallet Ed25519 signature over `attestation_hash`, verified via Ed25519 native + Instructions sysvar (§9.4); author derived from signer | **MVP** |
| 2 | **Unauthorized branch update** | repo takeover | `authority` field + signer check; `has_one`/explicit validation; owner-only default | **MVP** |
| 3 | **Replay** of commit/update signatures | duplicate/foreign writes | Domain-separated signed messages binding `repo`,`branch`,`head_seq`,`nonce`; `head_seq` CAS; PDA uniqueness | **MVP** |
| 4 | **Malicious repo owner force-push / history rewrite** | lost history | Append-only `history_root`; rewrites only via logged `reset_branch`; old roots persist | **MVP** (root), **Future** (MMR proofs) |
| 5 | **Compromised wallet** | unauthorized writes | Multisig owner (Squads PDA); key rotation via permissions; timelocks (future) | Future |
| 6 | **Storage disappearance / unavailability** | provable-but-unfetchable history | Multi-pin IPFS + Arweave for tags/checkpoints; `forge gc --verify-availability`; optional mirror incentives | Partial MVP, Future incentives |
| 7 | **Hash collision** (SHA-1) | forged history | SHA-256 Forge-native repos; add SHA-256 forge digest for SHA-1 interop repos; domain separation | **MVP** (SHA-256), Future (legacy digest) |
| 8 | **Malicious merge commit** | injected code shown as merged | Both parents must exist; one must be current head; merge signed by authorized merger; rule programs can require reviews | **MVP** (structure), Future (reviews) |
| 9 | **History rewriting / silent amend** | trust erosion | `history_root` + `BranchReset` events; all commits ever seen remain in log | **MVP** |
| 10 | **Spam repositories / commits** | state bloat, UX abuse | Rent as natural cost; owner-scoped names; optional per-repo fee/rate rule; commit metadata bounded (hashes only) | **MVP** (rent), Future (fees) |
| 11 | **Transaction ordering / race** | lost push, unfair ordering | `expected_head_seq` CAS; clients rebase+retry; no value depends on tx ordering | **MVP** |
| 12 | **Malicious metadata** (names, messages, tag names) | injection, misleading UI | Length/charset limits onchain; treat all strings as untrusted; sanitize in UIs; **never parse logs/events for critical data**; events via `emit_cpi!` | **MVP** |
| 13 | **Malicious file contents** | malware distribution | Protocol never executes content; provenance/CI scanning offchain; content clearly flagged as untrusted | **MVP** (non-execution), Future (scanning) |
| 14 | **Symlink / path traversal** | file escape, deceptive trees | Reject `/`, NUL, `.`/`..`, absolute paths at tree construction; symlinks stored as data, never resolved; submodules as links only | **MVP** |
| 15 | **Dependency attacks** (supply chain) | compromised builds | SBOM + in-toto/SLSA attestation hashes anchored; CI rules on protected branches | Future |
| 16 | **Denial of service** | unavailable repo/program | Program is stateless-per-repo; hot `RepositoryAccount` write-lock sharding (§9.5); account caps; client-side RPC redundancy | Partial MVP, Future sharding |
| 17 | **Storage provider censorship** | missing blobs | Multi-provider pinning; Arweave permanence; onchain storage hints allow alternates; content verifies by hash | Partial MVP, Future |
| 18 | **Malicious/observing RPC** | delayed/censored submission | Use reputable SWQoS RPC; user can submit elsewhere; no protocol reliance on RPC honesty | Future/Operational |
| 19 | **PDA seed collision / squatting** | wrong-account writes | Seeds include owner + repo + name/oid; canonical bump stored; `init` uniqueness; verify owner program | **MVP** |
| 20 | **Reinitialization / revival of closed accounts** | takeover, ghost state | Anchor `init`; avoid `init_if_needed`; proper `close` semantics; never trust point-in-time owner checks | **MVP** |

**Cross-cutting program security checklist** (from Solana security guidance, [security reference](https://solana.com/docs)): validate owner, signer, writable, PDA seeds + canonical bump; no arbitrary CPI; checked math; duplicate-mutable-account guard; secure close; treat `remaining_accounts` as fully untrusted and validate each; test each error path.

**Explicit non-goals for MVP:** confidentiality/private repos, anonymity, storage-provider economic incentives, fully trustless onchain rebuild of every historical commit.

---

## 12. Code provenance

### 12.1 The question

> "What exact source code produced this deployed Solana program?"

### 12.2 The chain of evidence

```
Deployed Program Account (program_id)
        │  executable hash  (solana-verify get-executable-hash)
        ▼
Build Artifact Hash  ──┐
                       │  ProgramSourceAttestation PDA ["prog", program_id]
                       │    { repo, commit_oid, artifact_hash, build_metadata_hash }
                       ▼
             Commit OID  (anchored in RepositoryAccount.history_root)
                       │  Git commit object, signed attestation
                       ▼
              Tree Root  (Git tree OID)
                       │
                       ▼
        Source Files  (Git blobs, content-addressed, hash-verified on fetch)
```

Every arrow is independently verifiable: the executable hash is computed from chain state; the attestation is onchain; the commit is in the anchored history and signed by its author; the tree and blobs are content-addressed and re-hashable. **No trusted API is required.**

### 12.3 Existing verifiable-build approaches (reuse, don't reinvent)

**Facts.** Solana supports verified builds: build in a pinned Docker image with `solana-verify`, then compare the local executable hash to the onchain program ([Solana verified builds](https://solana.com/docs/programs/verified-builds); [solana-verifiable-build](https://github.com/solana-foundation/solana-verifiable-build)). OtterSec maintains a verification API and continuous re-verification ([verify.osec.io](https://verify.osec.io/)). The general provenance format is SLSA/in-toto: a signed statement with a `subject` (artifact digest) and `predicate` (build definition + resolved dependencies) ([SLSA provenance](https://slsa.dev/provenance); [in-toto](https://github.com/in-toto/attestation/blob/main/README.md)).

**Design decision:** reuse `solana-verify` for the executable hash and reproducible rebuild, and shape our onchain attestation fields to match a subset of SLSA `buildDefinition`/`resolvedDependencies` so hashes are composable. Store the full attestation JSON offchain (IPFS/Arweave); store its hash onchain.

### 12.4 `forge verify-program <PROGRAM_ID>`

What it verifies, step by step:
1. **Fetch onchain program** and its `ProgramData`; compute the **executable hash** (reuse `solana-verify get-executable-hash`).
2. **Fetch `ProgramSourceAttestation`** PDA for `program_id`; read `repo`, `commit_oid`, `artifact_hash`, `build_metadata_hash`.
3. **Fetch the commit object** from CAS; recompute `commit_oid`; verify it equals the attestation.
4. **Verify authorship:** fetch the sidecar attestation; check `Ed25519_verify(commit.author, attestation_hash, signature)` and that `attestation_hash` matches the commit fields.
5. **Verify history inclusion:** walk parents to the branch head that is anchored onchain, or recompute the `history_root` chain / Merkle proof; assert the commit is in the anchored log.
6. **Verify tree/files:** fetch tree + blobs; recompute all OIDs; surface the exact files.
7. **Verify artifact:** compare onchain executable hash to `artifact_hash`. Optionally run `solana-verify verify-from-repo` in Docker for a **full reproducible rebuild** and compare again.
8. **Output:** `VERIFIED` / `MISMATCH` / `UNVERIFIED_CLAIM`, plus `repo`, `commit`, `author` (wallet), `authored_at`, `tree_root`, changed files, and storage locations.

Exit codes: `0` verified, `1` mismatch, `2` claim-only, `3` missing data. Provenance records can be tagged `verified` only after step 7's rebuild (MVP may mark claims vs. independently-verified distinctly).

---

## 13. CLI design

`forge` wraps Git. **MVP** commands are marked ✅; **SHOULD** are ◐; **FUTURE** are ○. For each MVP command: inputs, local behavior, chain behavior, storage behavior, output.

| Command | Tier | Inputs | Local | Onchain | Storage | Output |
|---|---|---|---|---|---|---|
| `forge init [dir]` | ✅ | dir | `git init` + write `.forge/config` | — | — | repo initialized |
| `forge clone <repo> [dir]` | ✅ | repo PDA (or URL) | fetch objects; checkout HEAD | read repo/branch | fetch bundle by OID | working tree |
| `forge add <paths>` | ✅ | paths | `git add` | — | — | staged files |
| `forge commit -m <msg>` | ✅ | msg | build objects, sign attestation | — | store attestation | commit OID |
| `forge push [remote] [branch]` | ✅ | branch | upload missing objects | `create_commit` + `update_branch` | upload CAR/bundle | tx sig + head |
| `forge pull [remote]` | ✅ | — | fetch objects, fast-forward | read branch | fetch by OID | updated tree |
| `forge log [branch]` | ✅ | branch | walk commits | read commits/branch | fetch messages | history list |
| `forge status` | ✅ | — | git status + local/chain divergence | — | — | status |
| `forge branch [name]` | ✅ | name | git branch | `create_branch` (on push/upstream) | — | branch list |
| `forge checkout <ref>` | ✅ | ref | git checkout (historical checkout works) | — | fetch objects if needed | working tree |
| `forge remote add/...` | ◐ | name/repo | config mapping | — | — | remote list |
| `forge verify [commit]` | ✅ | commit/ref | recompute OIDs | read commit + history | fetch objects | verified + author |
| `forge verify-program <id>` | ◐ | program id | compute executable hash | read provenance PDA | fetch artifacts | verified/mismatch |
| `forge tag <name>` | ◐ | name | `git tag` | `create_tag` | Arweave checkpoint | tag |
| `forge merge <branch>` | ◐ | branch | git merge, build merge commit | `merge` | upload | merge commit |
| `forge diff <a> <b>` | ○ | refs | git diff | — | fetch objects | diff |
| `forge blame <file>` | ○ | file | git blame | join authors from commits | fetch | annotated lines |
| `forge gc` | ○ | — | prune unreachable | — | `--verify-availability` | report |
| `forge rules ...` | ○ | — | — | rule accounts/CPIs | — | rule state |
| `forge ci ...` | ○ | — | — | CI attestation | artifacts | status |

**`forge push` detail (MVP):**
- **Inputs:** remote (repo PDA), branch (default: current).
- **Local:** determine objects missing onchain = commits since anchored head; build a CAR/git-bundle; compute attestations.
- **Chain:** for each new commit, `create_commit` (with prepended Ed25519 verify ix); then one `update_branch` with `expected_head_seq` for the branch tip.
- **Storage:** upload bundle to IPFS (≥2 pins) before the tx; include storage hint hash.
- **Output:** commit OIDs, tx signatures, new `head_seq`; on stale head, instruct the user to `forge pull` (CAS retry).

---

## 14. Local repository architecture

### 14.1 Layout

```
.forge/
  config                 # repo PDA, remote mapping, storage backends, wallet ref
  attestations/<oid>.cbor# signed commit attestations (sidecars)
  storage-index          # oid → { ipfs CID / arweave TXID / local path }
  onchain-refs           # cached branch heads + head_seq (to detect divergence)
  objects/  refs/  HEAD  # (present only if not using .git directly)
  index
```

**Design decision:** when building on Git, objects/refs live in `.git/` and `.forge/` holds only Forge-specific metadata. A standalone object DB is only needed if we choose not to depend on Git.

### 14.2 Options compared

| Option | Pros | Cons | Hackathon |
|---|---|---|---|
| **1. Build on Git** (`gix`/libgit2) | reuse objects, packfiles, index, refs, merge/diff; any repo works; `git` interop | dependency on Git internals; must add remote helper for native push | **Best** |
| **2. Own object DB** | full control; no Git dependency; can use SHA-256/CBOR natively | reimplement packfiles, diffs, merges, checkout; huge scope | No |
| **3. Git-compatible layer** | clean protocol; no Git binary dependency; can still import/export | still must implement enough of Git to be compatible; high risk | No |

### 14.3 Recommendation
**Build on Git** for the MVP. Git is the world's most battle-tested content-addressed store; reimplementing it is pure risk. Deliver a `forge` CLI wrapping `gix`/`git2`, and (SHOULD) a `git-remote-forge` helper so `git push forge main` works like Gitopia's helper ([Gitopia Git remote helper](https://github.com/gitopia/git-remote-gitopia)). This also makes migration trivial: `forge import <git repo>` can anchor an existing repo's history by walking and submitting commits.

---

## 15. Authentication & identity

**Design decision: wallet-native identity, optional social linking.**

- **Wallet signing:** Solana Ed25519 keypair (Phantom/Backpack/Solflare or CLI keypair) is the identity. The address *is* the author/owner.
- **Commit signing:** sign `attestation_hash` ([§5.5](#55-forge-attestation-what-the-wallet-signs)); verified onchain. This is the root of verifiable authorship.
- **Repository ownership:** `RepositoryAccount.owner` = a wallet **or** a program-owned PDA (Squads vault / DAO). Transferable.
- **Contributor identity:** `PermissionAccount` maps wallet → role. Onchain roles: reader/writer/maintainer/admin.
- **Wallet rotation:** `update_permissions` + `transfer_repository`; emit rotation events to preserve the audit trail; future: link old→new keys in a signed rotation record.
- **Multisig ownership:** set `owner` to a **Squads V4** vault PDA (`SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf`, [SolanaCompass](https://solanacompass.com/projects/squads)) or an SPL Governance/Realms authority. Because those programs can sign arbitrary CPIs, a 2-of-3 or DAO vote can authorize `update_branch`/`transfer_repository` — no protocol change required.
- **Delegated permissions:** `permissions_mode` per branch; a maintainer can be granted writer access to specific branches.
- **Optional GitHub linking:** a wallet signs a statement `{platform:"github", username, oauth_attestation_hash, nonce}`; the statement is anchored (or stored as a Solana Attestation Service record). This proves *linkage* but trusts the OAuth provider — clearly labeled as a **soft identity hint**, never as cryptographic authorship. *(Future.)*

**Threat note:** wallet theft = identity theft. Mitigations: hardware wallets, Squads multisig ownership, and (future) onchain timelocks for ownership transfers.

---

## 16. Programmable repository rules

### 16.1 The idea
GitHub branch protection, required reviews, and CI gates are **centralized server config** — not portable, not verifiable, and not self-enforcing. On Solana, a branch's `authority` can be **another program**, so rules become real code enforced by the chain.

### 16.2 Design: authority-program hook (elegant, minimal protocol change)

```
BranchAccount.authority = <wallet>  OR  <rule-program-owned PDA>
permissions_mode = 2 (authority-PDA)
```

To advance a protected branch, a **rule program** must CPI into `forge_repository::update_branch` such that the authority PDA signs. The rule program decides whether to approve:

```
main branch:
  authority = approval_policy PDA
  approval_policy.update_branch(...) requires:
    2 approvals from maintainers  (proposal + approve accounts)
release branch:
  authority = Realms DAO governance PDA (token vote / multisig)
commit:
  policy requires a CIResult attestation for commit_oid (see §17)
deployment:
  policy requires artifact_hash(program) ← commit in repo history
repository:
  owner = Squads 2-of-3
```

### 16.3 Why this is clean
- **No new rule language / VM.** Reuse existing, audited governance: Squads, SPL Governance, Realms ([Solana DAOs](https://solana.com/developers/dao)).
- **Optional and composable.** A repo with `permissions_mode = 0` never pays for it. Rules are added per branch.
- **Portable and transparent.** The rule is a program ID onchain, auditable by anyone — unlike a GitHub checkbox.

### 16.4 MVP treatment
Ship only `permissions_mode ∈ {owner-only, allowlist}` and the `authority` field (so the hook exists). Document the authority-program pattern as the extension. Do **not** build a rule VM or approval UI for the hackathon.

---

## 17. CI/CD integration

### 17.1 Flow

```
commit (onchain event)
   ↓  indexer / webhook
CI runner (offchain, in a pinned reproducible container)
   ↓  tests, build
artifact hash + in-toto/SLSA attestation
   ↓  user-signed tx (CI authority)
onchain CIResult / ProgramSourceAttestation
   ↓
protected branch / release tag requires it
```

### 17.2 Onchain representation

```rust
#[account]
pub struct CIResultAccount {
    pub repo: Pubkey,
    pub commit_oid: [u8; 32],
    pub status: u8,                 // 0=fail,1=pass,2=error
    pub artifact_hash: [u8; 32],
    pub attestation_hash: [u8; 32], // in-toto/SLSA envelope hash
    pub runner: Pubkey,             // CI authority (multisig in prod)
    pub created_slot: u64,
    pub bump: u8,
}
// PDA ["ci", repo, commit_oid, runner]
```

- **MVP (future tier):** store only `attestation_hash` + status; full logs/SBOM offchain (IPFS/Arweave).
- **Trust:** the result is only as trustworthy as the runner's key. Use a multisig/DAO runner authority, and mark results as attestations, not proofs. `forge verify` must present them as claims.
- **Branch rule:** `protected` bit `require_ci` makes `update_branch` check for a passing `CIResultAccount` (or CPI to the rule program).

### 17.3 Reuse standards
Adopt **in-toto Statement / SLSA Provenance** fields (`buildType`, `externalParameters`, `resolvedDependencies`, `byproducts`) and Sigstore-style signing where practical ([SLSA provenance](https://slsa.dev/provenance); [Sigstore](https://docs.sigstore.dev/cosign/verifying/attestation)). This lets Forge provenance interoperate with the broader supply-chain ecosystem instead of being an island.

---

## 18. Developer / user experience

### 18.1 Developer

```bash
$ forge init
$ forge add .
$ forge commit -m "Initial protocol"
$ forge push
  ✓ uploaded 6 objects to IPFS (2 pins)
  ✓ commit f3a9… signed by 9aB…
  ✓ create_commit  tx 4Tz…
  ✓ update_branch main (seq 0 → 1)  tx 5Qm…
```

### 18.2 Reviewer

```bash
$ forge log
  f3a9…  9aB…  Initial protocol
$ forge diff C10 C17
$ forge verify f3a9…
  ✓ commit OID recomputed
  ✓ author signature valid (9aB…)
  ✓ included in anchored history (seq 1, root 0x…)
  ✓ tree + 6 blobs hash-verified
```

### 18.3 User (provenance)

```
Program 7xK…
   ↓
verified source: repo forge, commit f3a9…
   ↓
contributors: 9aB… (wallet), backed by signature chain
   ↓
build: solana-verify, image digest sha256:…
```

### 18.4 Complete flow (narrative)
1. Alice initializes a repo → creates onchain repo + `main` branch.
2. Alice commits locally (Git objects + signed attestation), pushes → IPFS bundle + `create_commit` + `update_branch`.
3. Bob clones from the repo PDA → fetches bundle, verifies every OID, checks Alice's signature, checks the head matches the anchored ref.
4. Bob commits on `bob/feature` and pushes → new branch account.
5. Alice merges via `forge merge` → 2-parent merge commit anchored.
6. Alice tags `v1.0.0`; the tag and a checkpoint are pushed to Arweave.
7. Alice builds and deploys a Solana program reproducibly; `forge anchor-program` links `program_id → commit`.
8. A user runs `forge verify-program 7xK…` and independently confirms the deployed bytecode came from that exact commit.

---

## 19. Hackathon MVP

**Goal:** the smallest demo that proves the core blockchain value — *verifiable authorship + anchored history + program→commit provenance*.

### 19.1 MUST HAVE
- **Program:** `initialize_repository`, `create_commit` (Ed25519 verify), `create_branch`, `update_branch` (CAS via `head_seq`), read-only getters. Append-only `history_root`.
- **CLI (Rust, on Git):** `init`, `add`, `commit`, `push`, `clone`, `log`, `branch`, `checkout`, `verify`, `status`.
- **Storage:** upload object bundles to IPFS with ≥2 pins; verify by hash on fetch.
- **Identity:** wallet-signed commit attestations.
- **Network:** Solana **devnet**.
- **Demo:** two wallets; create repo, push commits, second wallet clones + verifies authorship; show a forged commit rejected; show a stale push rejected.
- **Verification story:** `forge verify` proves commit signature + history inclusion by recomputing OIDs.

### 19.2 SHOULD HAVE
- `forge tag` + Arweave checkpoint.
- `forge merge` (2-parent).
- `forge verify-program` (basic: executable hash vs. anchored attestation).
- Permission allowlist (owner + writers).
- `git-remote-forge` helper so `git push forge main` works.
- Minimal web UI: repo list, commit log, verify badge.

### 19.3 NICE TO HAVE
- Indexer (Helius webhooks) for fast queries + web UI.
- CI attestation (`CIResultAccount`).
- Squads multisig as repo owner (demo governance).
- ZK-compressed commit accounts.
- `forge blame` / `forge diff`.

### 19.4 FUTURE
- Full Git smart-protocol remote (fetch/push negotiation).
- MMR inclusion proofs; SHA-256 forge digest for SHA-1 repos.
- Private/encrypted repos.
- Storage economics/mirror incentives.
- Generic onchain rule programs and DAO governance.
- Cross-chain provenance.
- Reputation/contribution graphs (SourceCred-style) on verified authorship.

> **Do not build:** a custom merge engine, a general rule VM, or file storage onchain.

---

## 20. Recommended MVP architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        forge CLI (Rust)                          │
│  ┌──────────────┐ ┌────────────────┐ ┌───────────┐ ┌──────────┐  │
│  │ Local Git    │ │ Merkle/Object  │ │ Wallet    │ │ RPC      │  │
│  │ Object Store │ │ Engine (gix)   │ │ Signer    │ │ Client   │  │
│  │ (.git)       │ │                │ │ (Ed25519) │ │ (@solana │  │
│  │ .forge/ meta │ │                │ │           │ │  /kit)   │  │
│  └──────┬───────┘ └───────┬────────┘ └─────┬─────┘ └────┬─────┘  │
└─────────┼─────────────────┼────────────────┼────────────┼────────┘
          │                 │                │            │
          │ object bundle   │                │            ▼
          ▼                 │                │   ┌────────────────────┐
   ┌──────────────┐         │                │   │  Solana (devnet)   │
   │ IPFS (2 pins)│         │                │   │  Anchor program    │
   │ + Arweave    │         │                │   │  RepositoryAccount │
   │  (tags/ckpt) │         │                │   │  BranchAccount     │
   └──────────────┘         │                │   │  CommitAccount     │
                            │                │   │  ProgramSourceAtt.  │
                            │                │   └────────────────────┘
                            │                │
                    (hash-addressed)   (signed attestations)

Read path:  web / SDK → indexer (optional) → Solana RPC + IPFS → verify locally
```

**MVP decisions, stated plainly:**
- **Git is the object store**; Solana anchors OIDs.
- **Per-commit `CommitAccount`** for demo queryability; events emitted for the scale path.
- **`history_root` hash chain** for append-only history; MMR later.
- **IPFS primary + Arweave for tags/checkpoints.**
- **Anchor 1.1.x**, tested with Surfpool/LiteSVM.
- **Provenance attestation** is a first-class PDA, the differentiator.

---

## 21. Technology stack

| Layer | Choice | Why |
|---|---|---|
| **Blockchain** | **Solana devnet** | required by hackathon; cheap txs, fast finality; native Ed25519 + 100M CU blocks |
| **Program** | **Anchor 1.1.x** | IDL + codegen + constraints; fastest path; Anchor `test` now uses Surfpool by default ([release notes](https://v2.anchor-lang.com/docs/updates/release-notes/1-0-0)) |
| **Program (later)** | Pinocchio / Anchor v2 alpha | smaller binaries, fewer CU ([Anchor v2](https://v2.anchor-lang.com/docs/v1); [Pinocchio](https://github.com/pinocchioSolana/pinocchioSolana)) |
| **RPC** | Helius (devnet), fallback public RPC | reliability + webhooks/indexing; ZK-compression support if adopted |
| **CLI** | **Rust** (`gix`/`git2`, `clap`, `solana-sdk`, `reqwest`) | performance, type safety, direct `gix` object access; one binary |
| **CLI alt** | TypeScript (`@solana/kit`) | faster iteration if team prefers TS; weaker Git object access |
| **Storage** | **IPFS (Kubo/pinning) + Arweave (Irys/ArDrive)** | content-addressed hot + permanent cold; verify on read |
| **SDK** | **TypeScript `@solana/kit` v7+ with Codama-generated client** | official direction; typed instructions/accounts ([skill refs](https://solana.com/docs/tools)) |
| **Frontend** | **Next.js + `@solana/react` + `@solana/kit-plugin-wallet`** | official React bindings; Wallet Standard ([frontend refs](https://solana.com/docs)) |
| **Indexer** | Helius webhooks / Geyser (optional) | fast UX; not required for MVP with per-commit accounts |
| **Testing** | Surfpool (integration), LiteSVM/Mollusk (unit), Anchor tests, Trident (fuzz) | current recommended Solana testing stack ([Surfpool](https://solana.com/docs/tools/surfpool); [testing refs](https://solana.com/docs)) |

**Do not use** for new work: `@solana/wallet-adapter-*` or framework-kit; prefer `@solana/kit` plugins ([skill refs](https://solana.com/docs)).

---

## 22. Repository structure

```
onchain-forge/
├── programs/
│   └── forge_repository/
│       ├── src/
│       │   ├── lib.rs
│       │   ├── instructions/
│       │   │   ├── initialize_repository.rs
│       │   │   ├── create_commit.rs
│       │   │   ├── create_branch.rs
│       │   │   ├── update_branch.rs
│       │   │   ├── merge.rs
│       │   │   ├── create_tag.rs
│       │   │   ├── update_permissions.rs
│       │   │   └── anchor_program_source.rs
│       │   ├── state/{repository,commit,branch,tag,permission,attestation}.rs
│       │   ├── errors.rs
│       │   └── events.rs
│       ├── Cargo.toml
│       └── tests/
├── cli/                      # Rust `forge` CLI (gix)
│   ├── src/{main,commands,storage,chain,git,attest}.rs
│   └── Cargo.toml
├── sdk/                      # Codama-generated TS client + helpers
│   └── src/{client,repo,commit,verify,storage}.ts
├── web/                      # Next.js explorer / verify UI
│   └── app/
├── indexer/                  # Helius webhook consumer (optional)
│   └── src/
├── tests/
│   ├── e2e/                  # init→commit→push→clone→verify
│   └── vectors/              # golden Git OID vectors
├── docs/
│   ├── ONCHAIN_VERSION_CONTROL_ARCHITECTURE.md   # this file
│   ├── protocol.md
│   └── threat-model.md
├── Anchor.toml
├── package.json
└── README.md
```

---

## 23. API / SDK design

TypeScript SDK built on `@solana/kit` + a Codama-generated program client.

```typescript
import { createClient, signerFromFile, solanaDevnetRpc } from '@solana/kit';
import { forgeRepository } from '@onchain-forge/sdk';

const client = createClient()
  .use(signerFromFile(process.env.FORGE_KEYPAIR!))
  .use(solanaDevnetRpc());

// --- Repository ---
const repo = await forgeRepository.createRepository(client, {
  name: 'forge',
  defaultBranch: 'main',
  storageBackend: 'hybrid',
  flags: { requireSignedCommits: true },
});                                  // → { repo: Address, txSignature }

const meta = await forgeRepository.getRepository(client, repo);
// → { owner, name, defaultBranch, historyRoot, commitCount, ... }

// --- Commit ---
const commit = await forgeRepository.createCommit(client, {
  repo,
  commitOid: 'f3a9…',
  parents: ['c81b…'],
  treeOid: '9d02…',
  authoredAt: 1790001234,
  messageHash: '0x…',
  attestation,                      // canonical CBOR + signature
});                                  // → { commit: Address, txSignature }

const c = await forgeRepository.getCommit(client, repo, 'f3a9…');
// → { commitOid, parents, treeRoot, author, authoredAt, seq, ... }

// --- Branch ---
const branch = await forgeRepository.getBranch(client, repo, 'main');
// → { name, headCommit, headSeq, authority, permissionsMode }

const upd = await forgeRepository.updateBranch(client, {
  repo, branch: 'main',
  newHead: 'f3a9…',
  expectedHeadSeq: 0n,              // optimistic concurrency
});

// --- Verify ---
const result = await forgeRepository.verifyCommit(client, repo, 'f3a9…', {
  fetchObject: (oid) => ipfs.get(oid),   // untrusted; verified internally
});
// → { valid, signatureValid, author, included: true, seq, treeRoot }

// --- Content ---
const file = await forgeRepository.getFile(client, repo, 'f3a9…', 'src/lib.rs');
// → Buffer (hash-verified against tree)

// --- Provenance ---
const provenance = await forgeRepository.verifyProgram(client, PROGRAM_ID);
// → { status: 'VERIFIED'|'MISMATCH'|'CLAIM', repo, commit, author, artifactHash }
```

**Data flow:** SDK builds instructions → serializes args → wallet signs tx → RPC → program validates → account mutated + events emitted. Reads go to RPC (accounts) + CAS (objects); **every hashed value is re-verified client-side**, never trusted from storage.

---

## 24. Testing strategy

### Unit
- **Hashing determinism:** golden vectors — construct blobs/trees/commits and compare against `git hash-object` / `git cat-file` output for the same inputs; include the tree-ordering edge cases (`foo` vs `foo/` vs `foo.txt`).
- **Canonical serialization:** CBOR attestation determinism; map ordering; nonce handling.
- **Path safety:** reject `/`, `..`, NUL, absolute paths; NFC normalization.
- **Signature verification:** valid/invalid/malleable Ed25519 cases.

### Integration (program)
- **LiteSVM/Mollusk:** each instruction in isolation; every error path (duplicate commit, unauthorized, stale head, bad parent, bad signature) maps to a specific error code.
- **Surfpool:** fork devnet, run the full instruction set, use cheatcodes/time-travel; profile CU.
- **Anchor tests:** account constraints, PDA derivation, canonical bumps.

### End-to-end
- `forge init → add → commit → push → clone → log → verify` against a local validator, asserting onchain `head_seq`, `commit_count`, and `history_root`.
- Two-wallet authorship: wallet B cannot push a commit claiming wallet A.
- Replay: re-sending a signed `update_branch` fails after `head_seq` advances.
- History rewrite: `reset_branch` emits event and preserves `history_root`.

### Storage consistency
- CID/OID round-trip: upload then fetch, recompute OID, assert equality.
- Corruption test: mutate a stored blob, assert `forge verify` fails.
- Availability: `forge gc --verify-availability` reports unpinned objects.

### On/offchain consistency
- Anchor `history_root` matches a locally recomputed chain after N pushes.
- `forge verify-program` on a known deployed program (devnet) yields the expected status.

### Fuzzing (nice-to-have)
- **Trident** (Anchor) for instruction sequences; **cargo-fuzz** for the object parser/CBOR decoder. Never trust fetched bytes.

### CI
- Run `anchor test` (Surfpool backend), Rust unit tests, `cargo clippy -- -W clippy::all -W clippy::pedantic`, and golden-vector tests on every PR.

---

## 25. Open questions

| # | Question | Why it matters | Options | Current rec. | Experiment to resolve |
|---|---|---|---|---|---|
| 1 | Commit ID algorithm: Git SHA-1 vs. new SHA-256 digest? | interop vs. collision safety | SHA-1 compat; SHA-256 native; dual | **SHA-256 native, read SHA-1** | Golden-vector + migration test on a real repo |
| 2 | Per-commit accounts vs. events+indexer vs. ZK-compressed? | 100× cost difference at 10k commits | all three | **per-commit MVP → events → compression** | Cost benchmark on devnet at 1k/10k commits |
| 3 | How to guarantee blob availability long-term? | "provable but hollow" history | multi-pin; Arweave; incentives | **multi-pin + Arweave for tags** | Availability simulation with a killed pin |
| 4 | Rewrites vs. immutability: how strict? | legitimate rebase vs. tamper evidence | hard reject; logged reset; policy | **logged `reset_branch`** | UX test with a rebase workflow |
| 5 | Onchain verification of *every* historical signature? | trust vs. cost | head-only; all; batched | **head-only onchain, ancestors client-side** | CU measurement for all-signature batch |
| 6 | Private repos? | adoption | client-side encryption; trusted enclave | **future: encrypt blobs, anchor ciphertext hash** | Prototype encrypted-tree push |
| 7 | Global name registry / anti-squatting? | discoverability | owner-scoped; paid global; ENS-like | **owner-scoped for MVP** | Namespace design review |
| 8 | GitHub identity linking? | onboarding | SAS attestation; OAuth-signed | **future, soft hint only** | Prototype `forge link github` |
| 9 | How to prove very large repos cheaply? | verification cost | full fetch; sampling; Merkle proofs | **MMR + checkpoint proofs** | Benchmark MMR proof verify CU |
| 10 | Merge/review rules onchain? | GitHub parity | authority-program (reuse Squads/Realms) | **authority-program hook** | Demo Squads-gated push |
| 11 | Storage backend interface standardization? | portability | CID vs. OID vs. URI | **content-addressed by OID; backend-agnostic hints** | Implement two backends |
| 12 | Interop with real Git servers? | migration | `git-remote-forge`; import tooling | **remote helper + import** | Push/pull against a real repo |

---

## 26. Final architecture summary

### Architecture in one diagram

```
Developer (forge CLI, Rust, built on Git)
        │  objects + signed attestations
        ▼
Local Git object store (.git) + .forge metadata
        │  CAR / git-bundle (hash-addressed)
        ▼
Content-addressed storage: IPFS (hot, 2+ pins) + Arweave (permanent tags/checkpoints)
        │  OIDs, CIDs, signatures
        ▼
Solana program (Anchor): RepositoryAccount · BranchAccount · CommitAccount · TagAccount ·
                         PermissionAccount · ProgramSourceAttestation
        │
        ▼
Onchain repository state: owner · branches/heads · append-only history_root ·
                          permissions · program→commit provenance
```

### Core data model
`RepositoryAccount{owner,name,default_branch,history_root,commit_count,...}` · `BranchAccount{name,head_commit,head_seq,authority,permissions_mode,protected}` · `CommitAccount{commit_oid,parents,tree_oid,author,authored_at,message_hash,attestation_hash,seq}` · `TagAccount` · `PermissionAccount` · `ProgramSourceAttestation{program_id,repo,commit_oid,artifact_hash,build_metadata_hash,verified}`.

### Onchain / offchain boundary
- **Onchain:** identity, ownership, config, refs, commit IDs/parents/tree roots, author pubkeys, signatures, timestamps, history root, permissions, tags, provenance attestations.
- **Offchain:** all file bytes, binaries, Git objects/packfiles, messages, build artifacts, CI logs/SBOMs.
- **Rule:** *signature- or root-critical → onchain; re-hashable → offchain.*

### Transaction flow (push)
`forge commit` (local, sign) → upload bundle to IPFS/Arweave → tx1 `create_commit` (Ed25519 verify + append to history) → tx2 `update_branch` (CAS on `head_seq`) → event `BranchUpdated` → clients refetch.

### Security model (one line)
Wallet-signed commit IDs verified onchain + append-only `history_root` + per-branch authority with optimistic concurrency + hash-verified offchain storage + safe path/symlink handling; everything else (privacy, storage incentives, full historical re-verification) is explicitly future work.

### MVP scope
Program: `initialize_repository`, `create_commit`, `create_branch`, `update_branch` (+ getters). CLI: `init/add/commit/push/clone/log/branch/checkout/verify/status`. Storage: IPFS + Arweave-for-tags. Identity: wallet-signed attestations. Network: devnet. Optional: `forge tag`, `forge merge`, `forge verify-program`.

### Future roadmap
MMR proofs → commit events + indexer → ZK-compressed commit log → authority-program rules (Squads/Realms) → CI/SLSA attestations → encrypted private repos → storage incentives → `git-remote-forge` compatibility → contribution/reputation graphs.

### Biggest technical risks
1. **Onchain state cost** if per-commit accounts are kept at scale (mitigate via events/compression).
2. **Storage availability** — anchored history can outlive its data.
3. **Ed25519 verification ergonomics/ CU** per push.
4. **Git-vs-SHA-256 interop** and tree-ordering determinism bugs.
5. **Hot `RepositoryAccount` write-lock** throughput for popular repos.

### Biggest differentiator from Git/GitHub
Git is already a verifiable Merkle DAG; **Forge adds a neutral global anchor, wallet-native cryptographic authorship, self-enforcing programmable repository rules, and a permissionless *deployed-program → exact-commit → source* provenance link** — properties a centralized forge can only promise, not prove.

---

## References

**Git / version control**
- [Git Internals — Git Objects](https://git-scm.com/book/en/v2/Git-Internals-Git-Objects)
- [Git data model (gitdatamodel.adoc)](https://github.com/git/git/blob/master/Documentation/gitdatamodel.adoc)
- [gitobj tree ordering (SubtreeOrder)](https://github.com/git-lfs/gitobj/blob/main/tree.go)
- [isomorphic-git object models](https://deepwiki.com/isomorphic-git/isomorphic-git/4.2-git-object-models)
- [GitPython tree.py](https://github.com/gitpython-developers/GitPython/blob/main/git/objects/tree.py)
- [How Git stores data internally](https://singhajit.com/how-git-stores-data-internally)
- [Git LFS specification](https://github.com/git-lfs/git-lfs/blob/main/docs/spec.md)
- [Git LFS site](https://git-lfs.com/)

**Decentralized storage**
- [IPFS Merkle DAG docs](https://docs.ipfs.tech/concepts/merkle-dag)
- [IPFS — Content addressed, versioned, P2P (Benet 2014)](https://arxiv.org/abs/1407.3561)
- [IPFS IPIP-0499 reproducible CIDs](https://blog.ipfs.tech/2026-03-reproducible-cids)
- [BitcoinWiki: IPFS](https://bitcoinwiki.org/wiki/IPFS)
- [Arweave fee calculator](https://ar-fees.arweave.net/)
- [Kraken: Arweave (AR) report](https://assets-cms.kraken.com/files/51n36hrp/facade/fa29aa9bad855e904fd585717fde45f3bfe61aca.pdf)
- [Markets Insider: Arweave permanence](https://markets.businessinsider.com/news/stocks/most-crypto-is-about-money-arweave-ar-is-about-memory-1036202726)

**Solana**
- [Solana docs: Transactions](https://solana.com/docs/core/transactions)
- [Solana "Reduced Rent" (SIMD-0437)](https://solana.com/upgrades/reduced-rent)
- [Gate News: rent at 5,080 lamports/byte](https://www.gate.com/news/detail/solana-account-rent-parameters-drop-to-5080-lamports-in-simd-0437-phase-2-24237691)
- [Solana "Larger Transaction Sizes" (SIMD-0296/0385)](https://solana.com/upgrades/larger-transaction-sizes)
- [Solana "100M CU Blocks" (SIMD-0286)](https://solana.com/upgrades/100m-cu-blocks)
- [Solana PDA docs](https://solana.com/docs/core/pda)
- [Solana Program Deployment / verified builds](https://solana.com/docs/core/programs/program-deployment)
- [Solana Verified Builds](https://solana.com/docs/programs/verified-builds)
- [solana-verifiable-build CLI](https://github.com/solana-foundation/solana-verifiable-build)
- [OtterSec verified builds API](https://verify.osec.io/)
- [Chainstack: compute budget](https://docs.chainstack.com/docs/solana-compute-budget)
- [ZK Compression](https://www.zkcompression.com/home)
- [SolanaCompass: Light Protocol](https://solanacompass.com/projects/light-protocol)
- [Soladex: ZK Compression](https://www.soladex.io/glossary/zk-compression)
- [Surfpool](https://solana.com/docs/tools/surfpool)
- [Anchor 1.0/1.1 release notes](https://v2.anchor-lang.com/docs/updates/release-notes/1-0-0)
- [Anchor v2 (Pinocchio-based)](https://v2.anchor-lang.com/docs/v1)
- [Pinocchio](https://github.com/pinocchioSolana/pinocchioSolana)
- [SolanaCompass: Squads](https://solanacompass.com/projects/squads)
- [Solana DAOs & Governance](https://solana.com/developers/dao)

**Existing decentralized-development projects**
- [Radicle protocol guide](https://radicle.dev/guides/protocol)
- [Gitopia architecture](https://docs.gitopia.com/gitopia-architecture/index.html)
- [Gitopia whitepaper](https://gitopia.com/whitepaper.pdf)
- [Gitopia GitHub](https://github.com/gitopia/gitopia)
- [Dgit (HN discussion)](https://news.ycombinator.com/item?id=22684945)

**Provenance / supply chain**
- [SLSA provenance](https://slsa.dev/provenance)
- [SLSA v1.0 attestation model](https://slsa.dev/spec/v1.0/attestation-model)
- [in-toto Attestation Framework](https://github.com/in-toto/attestation/blob/main/README.md)
- [SLSA: distributing provenance](https://github.com/slsa-framework/slsa/blob/main/spec/distributing-provenance.md)
- [Sigstore: in-toto attestations](https://docs.sigstore.dev/cosign/verifying/attestation)
- [Sourcify](https://sourcify.dev/)
- [Etherscan contract verification](https://docs.etherscan.io/contract-verification/get-source-code)

---

*This document is a design specification, not an implementation. Network parameters, prices, and feature activations are subject to change; re-verify all cited constants before deployment. Estimates are arithmetic on cited values at the research date and are not guarantees.*
