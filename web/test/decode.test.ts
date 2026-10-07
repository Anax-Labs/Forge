import { expect, test } from "bun:test";

import { base58Decode, base58Encode } from "../lib/base58.ts";
import { decodeRepo } from "../lib/decode.ts";

test("base58 round-trips arbitrary bytes", () => {
  const bytes = new Uint8Array(32);
  for (let i = 0; i < bytes.length; i += 1) bytes[i] = (i * 7 + 1) & 0xff;
  expect(Array.from(base58Decode(base58Encode(bytes)))).toEqual(Array.from(bytes));
});

test("base58 decodes the system program (all zeros)", () => {
  const decoded = base58Decode("11111111111111111111111111111111");
  expect(decoded.length).toBe(32);
  expect(decoded.every((b) => b === 0)).toBe(true);
});

test("decodeRepo reads the frozen RepositoryAccount layout", () => {
  const buf = new Uint8Array(256);
  buf.set(new Uint8Array(32).fill(1), 8); // owner
  buf.set(new Uint8Array(32).fill(2), 40); // repo_id
  buf.set(new TextEncoder().encode("forge"), 72); // name
  buf.set(new TextEncoder().encode("main"), 104); // default_branch
  buf.set(new Uint8Array(32).fill(3), 136); // history_root
  new DataView(buf.buffer).setBigUint64(168, 5n, true); // commit_count

  const repo = decodeRepo("addr", buf);
  expect(repo.name).toBe("forge");
  expect(repo.defaultBranch).toBe("main");
  expect(repo.commitCount).toBe(5);
  expect(repo.historyRoot[0]).toBe(3);
  expect(repo.owner).toBe(base58Encode(new Uint8Array(32).fill(1)));
});
