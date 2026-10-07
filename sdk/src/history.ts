// Append-only repository history root and repo-root fingerprint, mirroring
// crates/forge-object/src/history.rs.

import { type Algorithm, type Oid, concat, digest, leU64, oid, toBytes32 } from "./oid.ts";

const GENESIS_DOMAIN = new TextEncoder().encode("forge-genesis\0");
const APPEND_DOMAIN = new TextEncoder().encode("forge-append\0");
const REPO_ROOT_DOMAIN = new TextEncoder().encode("forge-repo\0");

export function genesisHistoryRoot(repoPda: Uint8Array, algorithm: Algorithm): Oid {
  return oid(algorithm, digest(algorithm, concat(GENESIS_DOMAIN, repoPda)));
}

export function appendHistoryRoot(
  algorithm: Algorithm,
  previous: Oid,
  commitOid: Oid,
  seq: number,
): Oid {
  const preimage = concat(
    APPEND_DOMAIN,
    toBytes32(previous),
    toBytes32(commitOid),
    leU64(seq),
  );
  return oid(algorithm, digest(algorithm, preimage));
}

export function historyRootChain(
  algorithm: Algorithm,
  repoPda: Uint8Array,
  commits: Array<[Oid, number]>,
): Oid {
  let root = genesisHistoryRoot(repoPda, algorithm);
  for (const [commit, seq] of commits) {
    root = appendHistoryRoot(algorithm, root, commit, seq);
  }
  return root;
}

export function repoRoot(
  algorithm: Algorithm,
  repoPda: Uint8Array,
  defaultBranchHead: Oid,
  historyRoot: Oid,
  commitCount: number,
): Oid {
  const preimage = concat(
    REPO_ROOT_DOMAIN,
    repoPda,
    toBytes32(defaultBranchHead),
    toBytes32(historyRoot),
    leU64(commitCount),
  );
  return oid(algorithm, digest(algorithm, preimage));
}
