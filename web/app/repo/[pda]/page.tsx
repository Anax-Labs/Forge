"use client";

import Link from "next/link";
import { useParams } from "next/navigation";
import { useEffect, useState } from "react";

import { base58Decode, shortAddress } from "@/lib/base58";
import { PROGRAM_ID, RPC_URL } from "@/lib/config";
import {
  type BranchAccount,
  type CommitAccount,
  type ProvenanceAccount,
  type RepoAccount,
  type TagAccount,
  decodeBranch,
  decodeCommit,
  decodeProvenance,
  decodeRepo,
  decodeTag,
} from "@/lib/decode";
import { accountDiscriminatorBase58 } from "@/lib/discriminator";
import {
  type MemcmpFilter,
  b64ToBytes,
  getAccountInfo,
  getProgramAccounts,
} from "@/lib/rpc";
import { bytesEqual, historyRootChain, isZero, shortHex, toHex } from "@/lib/verify";

interface Loaded {
  repo: RepoAccount;
  branches: BranchAccount[];
  commits: CommitAccount[];
  tags: TagAccount[];
  provenance: ProvenanceAccount[];
  historyOk: boolean | null;
}

function Badge({ ok, label }: { ok: boolean; label: string }) {
  return (
    <span
      style={{
        display: "inline-block",
        padding: "0.1rem 0.5rem",
        borderRadius: "999px",
        fontSize: "0.8rem",
        background: ok ? "#123a24" : "#3a1717",
        color: ok ? "#5ee08a" : "#ff7a7a",
      }}
    >
      {label}
    </span>
  );
}

export default function RepoPage() {
  const params = useParams<{ pda: string }>();
  const pda = params?.pda;
  const [data, setData] = useState<Loaded | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!pda) return;
    (async () => {
      try {
        const info = await getAccountInfo(RPC_URL, pda);
        if (!info) {
          setError("repository account not found");
          return;
        }
        const repo = decodeRepo(pda, b64ToBytes(info.data[0]));
        const repoFilter: MemcmpFilter = { memcmp: { offset: 8, bytes: pda } };

        const [branchDisc, commitDisc, tagDisc, provDisc] = await Promise.all([
          accountDiscriminatorBase58("BranchAccount"),
          accountDiscriminatorBase58("CommitAccount"),
          accountDiscriminatorBase58("TagAccount"),
          accountDiscriminatorBase58("ProgramSourceAttestation"),
        ]);
        const withRepo = (disc: string): MemcmpFilter[] => [
          { memcmp: { offset: 0, bytes: disc } },
          repoFilter,
        ];
        const [branchAccts, commitAccts, tagAccts, provAccts] = await Promise.all([
          getProgramAccounts(RPC_URL, PROGRAM_ID, withRepo(branchDisc)),
          getProgramAccounts(RPC_URL, PROGRAM_ID, withRepo(commitDisc)),
          getProgramAccounts(RPC_URL, PROGRAM_ID, withRepo(tagDisc)),
          getProgramAccounts(RPC_URL, PROGRAM_ID, [
            { memcmp: { offset: 0, bytes: provDisc } },
            { memcmp: { offset: 40, bytes: pda } },
          ]),
        ]);

        const branches = branchAccts.map((a) =>
          decodeBranch(a.pubkey, b64ToBytes(a.account.data[0])),
        );
        const commits = commitAccts
          .map((a) => decodeCommit(a.pubkey, b64ToBytes(a.account.data[0])))
          .sort((a, b) => a.seq - b.seq);
        const tags = tagAccts.map((a) => decodeTag(a.pubkey, b64ToBytes(a.account.data[0])));
        const provenance = provAccts.map((a) =>
          decodeProvenance(a.pubkey, b64ToBytes(a.account.data[0])),
        );

        let historyOk: boolean | null = null;
        if (BigInt(commits.length) === BigInt(repo.commitCount)) {
          const root = await historyRootChain(
            base58Decode(pda),
            commits.map((c) => ({ oid: c.commitOid, seq: c.seq })),
          );
          historyOk = bytesEqual(root, repo.historyRoot);
        }

        setData({ repo, branches, commits, tags, provenance, historyOk });
      } catch (err) {
        setError(String(err));
      }
    })();
  }, [pda]);

  if (error) return <p style={{ color: "#ff6b6b" }}>Error: {error}</p>;
  if (!data) return <p style={{ color: "#8b98a9" }}>Loading repository…</p>;

  const { repo, branches, commits, tags, provenance, historyOk } = data;

  return (
    <div>
      <p>
        <Link href="/" style={{ color: "#7ab7ff" }}>
          ← all repositories
        </Link>
      </p>
      <h2 style={{ marginBottom: "0.2rem" }}>{repo.name}</h2>
      <p style={{ color: "#8b98a9", marginTop: 0 }} title={repo.owner}>
        owner {shortAddress(repo.owner)} · default branch {repo.defaultBranch} ·{" "}
        {repo.commitCount} commits
      </p>

      <h3>
        Anchored history{" "}
        {historyOk === null ? (
          <span style={{ color: "#8b98a9", fontSize: "0.85rem" }}>
            (incomplete commit set)
          </span>
        ) : (
          <Badge ok={historyOk} label={historyOk ? "history_root ✓" : "history_root ✗"} />
        )}
      </h3>
      <p style={{ color: "#8b98a9", fontFamily: "monospace", fontSize: "0.8rem" }}>
        {toHex(repo.historyRoot)}
      </p>

      <h3>Branches</h3>
      <ul>
        {branches.map((branch) => (
          <li key={branch.address}>
            <strong>{branch.name}</strong> · head_seq {branch.headSeq} ·{" "}
            {isZero(branch.headCommit) ? (
              <em>empty</em>
            ) : (
              <code>{shortHex(branch.headCommit)}</code>
            )}
          </li>
        ))}
        {branches.length === 0 && <li style={{ color: "#8b98a9" }}>none</li>}
      </ul>

      <h3>Commits</h3>
      <table style={{ width: "100%", borderCollapse: "collapse" }}>
        <thead>
          <tr style={{ textAlign: "left", color: "#8b98a9" }}>
            <th style={{ padding: "0.3rem 0.6rem" }}>seq</th>
            <th style={{ padding: "0.3rem 0.6rem" }}>commit</th>
            <th style={{ padding: "0.3rem 0.6rem" }}>author</th>
            <th style={{ padding: "0.3rem 0.6rem" }}>parents</th>
            <th style={{ padding: "0.3rem 0.6rem" }}>attested</th>
          </tr>
        </thead>
        <tbody>
          {commits.map((commit) => (
            <tr key={commit.address} style={{ borderTop: "1px solid #1c2431" }}>
              <td style={{ padding: "0.3rem 0.6rem" }}>{commit.seq}</td>
              <td style={{ padding: "0.3rem 0.6rem" }} title={toHex(commit.commitOid)}>
                <code>{shortHex(commit.commitOid)}</code>
              </td>
              <td style={{ padding: "0.3rem 0.6rem" }} title={commit.author}>
                {shortAddress(commit.author)}
              </td>
              <td style={{ padding: "0.3rem 0.6rem" }}>{commit.parentCount}</td>
              <td style={{ padding: "0.3rem 0.6rem" }}>
                {isZero(commit.attestationHash) ? (
                  <Badge ok={false} label="unsigned" />
                ) : (
                  <Badge ok label="signed ✓" />
                )}
              </td>
            </tr>
          ))}
          {commits.length === 0 && (
            <tr>
              <td colSpan={5} style={{ padding: "0.3rem 0.6rem", color: "#8b98a9" }}>
                no commits
              </td>
            </tr>
          )}
        </tbody>
      </table>

      <h3>Tags</h3>
      <ul>
        {tags.map((tag) => (
          <li key={tag.address}>
            <strong>{tag.name}</strong> → <code>{shortHex(tag.targetCommit)}</code>{" "}
            {tag.signed === 1 ? <Badge ok label="signed" /> : <Badge ok={false} label="unsigned" />}
          </li>
        ))}
        {tags.length === 0 && <li style={{ color: "#8b98a9" }}>none</li>}
      </ul>

      <h3>Program provenance</h3>
      <ul>
        {provenance.map((claim) => (
          <li key={claim.address}>
            program <code>{shortAddress(claim.programId)}</code> → commit{" "}
            <code>{shortHex(claim.commitOid)}</code>{" "}
            {claim.verified === 1 ? (
              <Badge ok label="verified" />
            ) : (
              <Badge ok={false} label="claim" />
            )}
          </li>
        ))}
        {provenance.length === 0 && <li style={{ color: "#8b98a9" }}>none</li>}
      </ul>
    </div>
  );
}
