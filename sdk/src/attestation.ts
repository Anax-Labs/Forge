// Canonical Forge attestation + attestation_hash, mirroring
// crates/forge-object/src/attestation.rs and docs/protocol.md.

import * as cbor from "./cbor.ts";
import { type Oid, concat, digest, oid, tagged } from "./oid.ts";

const ATTESTATION_DOMAIN = new TextEncoder().encode("forge-attestation\0");

export const ATTESTATION_VERSION = 1;

export interface Attestation {
  version: number;
  repo: string;
  commit: Oid;
  parents: Oid[];
  tree: Oid;
  author: string;
  authoredAt: number;
  messageHash: Oid;
  nonce: string;
}

export function toCanonicalCbor(attestation: Attestation): Uint8Array {
  const parents = [
    ...cbor.encodeArrayHead(attestation.parents.length),
    ...attestation.parents.flatMap((parent) => cbor.encodeText(tagged(parent))),
  ];
  const entries: Array<[number[], number[]]> = [
    [cbor.encodeText("v"), cbor.encodeUint(attestation.version)],
    [cbor.encodeText("repo"), cbor.encodeText(attestation.repo)],
    [cbor.encodeText("commit"), cbor.encodeText(tagged(attestation.commit))],
    [cbor.encodeText("parents"), parents],
    [cbor.encodeText("tree"), cbor.encodeText(tagged(attestation.tree))],
    [cbor.encodeText("author"), cbor.encodeText(attestation.author)],
    [cbor.encodeText("authoredAt"), cbor.encodeInt(attestation.authoredAt)],
    [cbor.encodeText("messageHash"), cbor.encodeText(tagged(attestation.messageHash))],
    [cbor.encodeText("nonce"), cbor.encodeText(attestation.nonce)],
  ];
  return cbor.toBytes(cbor.encodeMap(entries));
}

export function attestationHash(attestation: Attestation): Oid {
  const algorithm = attestation.commit.algorithm;
  const preimage = concat(ATTESTATION_DOMAIN, toCanonicalCbor(attestation));
  return oid(algorithm, digest(algorithm, preimage));
}
