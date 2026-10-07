"use client";

import Link from "next/link";
import { useEffect, useState } from "react";

import { shortAddress } from "@/lib/base58";
import { PROGRAM_ID, RPC_URL } from "@/lib/config";
import { decodeRepo, type RepoAccount } from "@/lib/decode";
import { b64ToBytes, getProgramAccounts } from "@/lib/rpc";

export default function HomePage() {
  const [repos, setRepos] = useState<RepoAccount[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getProgramAccounts(RPC_URL, PROGRAM_ID)
      .then((accounts) =>
        setRepos(
          accounts.map((account) =>
            decodeRepo(account.pubkey, b64ToBytes(account.account.data[0])),
          ),
        ),
      )
      .catch((err) => setError(String(err)));
  }, []);

  if (error) return <p style={{ color: "#ff6b6b" }}>RPC error: {error}</p>;
  if (!repos) return <p style={{ color: "#8b98a9" }}>Loading repositories…</p>;
  if (repos.length === 0) {
    return <p style={{ color: "#8b98a9" }}>No repositories found on {PROGRAM_ID}.</p>;
  }

  return (
    <table style={{ width: "100%", borderCollapse: "collapse" }}>
      <thead>
        <tr style={{ textAlign: "left", color: "#8b98a9" }}>
          <th style={{ padding: "0.4rem 0.6rem" }}>Repository</th>
          <th style={{ padding: "0.4rem 0.6rem" }}>Owner</th>
          <th style={{ padding: "0.4rem 0.6rem" }}>Default branch</th>
          <th style={{ padding: "0.4rem 0.6rem" }}>Commits</th>
        </tr>
      </thead>
      <tbody>
        {repos.map((repo) => (
          <tr key={repo.address} style={{ borderTop: "1px solid #1c2431" }}>
            <td style={{ padding: "0.4rem 0.6rem" }}>
              <Link href={`/repo/${repo.address}`} style={{ color: "#7ab7ff" }}>
                {repo.name}
              </Link>
            </td>
            <td style={{ padding: "0.4rem 0.6rem" }} title={repo.owner}>
              {shortAddress(repo.owner)}
            </td>
            <td style={{ padding: "0.4rem 0.6rem" }}>{repo.defaultBranch}</td>
            <td style={{ padding: "0.4rem 0.6rem" }}>{repo.commitCount}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
