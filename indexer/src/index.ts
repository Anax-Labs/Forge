// Optional indexer (Phase 10b): a non-authoritative cache over the onchain
// accounts. It reuses the explorer's decoders. Verification paths must never
// depend on this service (§3.2, §9.1).
//
// Run: `FORGE_RPC=... FORGE_PROGRAM_ID=... bun run src/index.ts`

import {
  decodeBranch,
  decodeCommit,
  decodeProvenance,
  decodeRepo,
  decodeTag,
} from "../../web/lib/decode.ts";
import { accountDiscriminatorBase58 } from "../../web/lib/discriminator.ts";
import { b64ToBytes, getProgramAccounts, type MemcmpFilter } from "../../web/lib/rpc.ts";

export interface RepoSnapshot {
  address: string;
  owner: string;
  name: string;
  defaultBranch: string;
  commitCount: number;
  branches: number;
  commits: number;
  tags: number;
  provenance: number;
}

export interface Snapshot {
  fetchedAt: string;
  programId: string;
  repos: RepoSnapshot[];
}

async function discriminatorFilters(
  name: string,
  offset: number,
  value: string,
): Promise<MemcmpFilter[]> {
  return [
    { memcmp: { offset: 0, bytes: await accountDiscriminatorBase58(name) } },
    { memcmp: { offset, bytes: value } },
  ];
}

/** Fetches and summarizes all repositories (and their related accounts). */
export async function snapshot(rpcUrl: string, programId: string): Promise<Snapshot> {
  const repoDisc = await accountDiscriminatorBase58("RepositoryAccount");
  const repoAccounts = await getProgramAccounts(rpcUrl, programId, [
    { memcmp: { offset: 0, bytes: repoDisc } },
  ]);

  const repos: RepoSnapshot[] = [];
  for (const account of repoAccounts) {
    const repo = decodeRepo(account.pubkey, b64ToBytes(account.account.data[0]));
    const [branches, commits, tags, provenance] = await Promise.all([
      getProgramAccounts(
        rpcUrl,
        programId,
        await discriminatorFilters("BranchAccount", 8, account.pubkey),
      ),
      getProgramAccounts(
        rpcUrl,
        programId,
        await discriminatorFilters("CommitAccount", 8, account.pubkey),
      ),
      getProgramAccounts(
        rpcUrl,
        programId,
        await discriminatorFilters("TagAccount", 8, account.pubkey),
      ),
      getProgramAccounts(
        rpcUrl,
        programId,
        await discriminatorFilters("ProgramSourceAttestation", 40, account.pubkey),
      ),
    ]);
    // Decode a couple of accounts to catch layout drift early.
    branches.forEach((a) => decodeBranch(a.pubkey, b64ToBytes(a.account.data[0])));
    commits.forEach((a) => decodeCommit(a.pubkey, b64ToBytes(a.account.data[0])));
    tags.forEach((a) => decodeTag(a.pubkey, b64ToBytes(a.account.data[0])));
    provenance.forEach((a) => decodeProvenance(a.pubkey, b64ToBytes(a.account.data[0])));

    repos.push({
      address: account.pubkey,
      owner: repo.owner,
      name: repo.name,
      defaultBranch: repo.defaultBranch,
      commitCount: repo.commitCount,
      branches: branches.length,
      commits: commits.length,
      tags: tags.length,
      provenance: provenance.length,
    });
  }

  return { fetchedAt: new Date().toISOString(), programId, repos };
}

export function cachePath(dir = process.cwd()): string {
  return `${dir}/.forge-index.json`;
}

if (import.meta.main) {
  const rpcUrl = process.env.FORGE_RPC ?? "https://api.devnet.solana.com";
  const programId =
    process.env.FORGE_PROGRAM_ID ?? "4smCAEoycSXSvVsyic8ircQmHmENPCHn17Fma83SYVbf";
  const snap = await snapshot(rpcUrl, programId);
  await Bun.write(cachePath(), JSON.stringify(snap, null, 2));
  console.log(`indexed ${snap.repos.length} repositories -> ${cachePath()}`);
}
