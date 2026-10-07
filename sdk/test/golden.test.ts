// Cross-language determinism: the TypeScript SDK must reproduce the exact
// values in tests/vectors/golden.json, which the Rust `forge-object` crate also
// produces (and which are cross-checked against real `git`).

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { attestationHash, toCanonicalCbor } from "../src/attestation.ts";
import { historyRootChain, genesisHistoryRoot, repoRoot } from "../src/history.ts";
import { blobOid } from "../src/object.ts";
import {
  type Algorithm,
  type Oid,
  digest,
  fromHex,
  hexToBytes,
  toHex,
} from "../src/oid.ts";

const vectors = JSON.parse(
  readFileSync(new URL("../../tests/vectors/golden.json", import.meta.url), "utf8"),
);

const REPO_PDA = hexToBytes(vectors.repo_pda);
const REPO_STRING = "7xKrepoPdaPlaceholder111111111111111111111";
const AUTHOR = "9aBauthorPlaceholder1111111111111111111111";
const NONCE = "nonceNonceNonceNonceNonceNonce22";
const AUTHORED_AT = 1_700_000_000;
const COMMIT_MSG = new TextEncoder().encode("Initial commit\n");

function oidOf(algorithm: Algorithm, hex: string): Oid {
  return fromHex(algorithm, hex);
}

for (const algorithm of ["sha1", "sha256"] as Algorithm[]) {
  const expected = vectors.algorithms[algorithm];

  test(`${algorithm}: blob oids match golden vectors`, () => {
    for (const file of vectors.files) {
      const content = hexToBytes(file.content_hex);
      const computed = toHex(blobOid(content, algorithm).bytes);
      assert.equal(computed, expected.blobs[file.path], `blob ${file.path}`);
    }
  });

  test(`${algorithm}: canonical attestation CBOR matches`, () => {
    const commit = oidOf(algorithm, expected.commit);
    const tree = oidOf(algorithm, expected.root_tree);
    const messageHash = fromHex(algorithm, toHex(digest(algorithm, COMMIT_MSG)));
    const cborHex = toHex(
      toCanonicalCbor({
        version: 1,
        repo: REPO_STRING,
        commit,
        parents: [],
        tree,
        author: AUTHOR,
        authoredAt: AUTHORED_AT,
        messageHash,
        nonce: NONCE,
      }),
    );
    assert.equal(cborHex, expected.attestation_cbor_hex);
  });

  test(`${algorithm}: attestation_hash matches`, () => {
    const commit = oidOf(algorithm, expected.commit);
    const tree = oidOf(algorithm, expected.root_tree);
    const messageHash = fromHex(algorithm, toHex(digest(algorithm, COMMIT_MSG)));
    const hash = attestationHash({
      version: 1,
      repo: REPO_STRING,
      commit,
      parents: [],
      tree,
      author: AUTHOR,
      authoredAt: AUTHORED_AT,
      messageHash,
      nonce: NONCE,
    });
    assert.equal(toHex(hash.bytes), expected.attestation_hash);
  });

  test(`${algorithm}: history_root and repo_root match`, () => {
    const commit = oidOf(algorithm, expected.commit);
    assert.equal(
      toHex(genesisHistoryRoot(REPO_PDA, algorithm).bytes),
      expected.genesis_history_root,
    );
    const root = historyRootChain(algorithm, REPO_PDA, [[commit, 0]]);
    assert.equal(toHex(root.bytes), expected.history_root);
    assert.equal(
      toHex(repoRoot(algorithm, REPO_PDA, commit, root, 1).bytes),
      expected.repo_root,
    );
  });
}
