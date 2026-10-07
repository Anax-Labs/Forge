import { expect, test } from "bun:test";

import { base58Encode } from "../../web/lib/base58.ts";
import { snapshot } from "../src/index.ts";

function repoAccountBuffer(name: string, commitCount: number): Uint8Array {
  const buf = new Uint8Array(256);
  buf.set(new TextEncoder().encode(name), 72); // name field
  new DataView(buf.buffer).setBigUint64(168, BigInt(commitCount), true); // commit_count
  return buf;
}

function mockRpc(repoBuf: Uint8Array, owner: string) {
  return (async (_url: string, init: { body: string }) => {
    const request = JSON.parse(init.body) as {
      method: string;
      params: [string, { filters: unknown[] }];
    };
    const filters = request.params[1]?.filters ?? [];
    const result =
      request.method === "getProgramAccounts" && filters.length === 1
        ? [
            {
              pubkey: owner,
              account: {
                data: [Buffer.from(repoBuf).toString("base64"), "base64"],
                owner,
                executable: false,
                lamports: 1,
              },
            },
          ]
        : [];
    return new Response(JSON.stringify({ jsonrpc: "2.0", id: 1, result }), {
      status: 200,
    });
  }) as unknown as typeof fetch;
}

test("snapshot summarizes repositories and their related accounts", async () => {
  const owner = base58Encode(new Uint8Array(32).fill(1));
  globalThis.fetch = mockRpc(repoAccountBuffer("forge", 3), owner);

  const snap = await snapshot("http://mock", owner);
  expect(snap.repos.length).toBe(1);
  expect(snap.repos[0].name).toBe("forge");
  expect(snap.repos[0].defaultBranch).toBe("");
  expect(snap.repos[0].commitCount).toBe(3);
  expect(snap.repos[0].commits).toBe(0);
});
