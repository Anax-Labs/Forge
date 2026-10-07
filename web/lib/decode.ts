// Account decoders matching the frozen layouts (ADR 0003). Each account starts
// with an 8-byte Anchor discriminator, which is skipped. Field order is frozen,
// so offsets are stable.

import { base58Encode } from "./base58.ts";

function dataView(bytes: Uint8Array): DataView {
  return new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
}

function u64(bytes: Uint8Array, offset: number): number {
  return Number(dataView(bytes).getBigUint64(offset, true));
}

function i64(bytes: Uint8Array, offset: number): number {
  return Number(dataView(bytes).getBigInt64(offset, true));
}

function pubkey(bytes: Uint8Array, offset: number): string {
  return base58Encode(bytes.subarray(offset, offset + 32));
}

function bytes32(bytes: Uint8Array, offset: number): Uint8Array {
  return bytes.slice(offset, offset + 32);
}

function name(bytes: Uint8Array, offset: number): string {
  const slice = bytes.subarray(offset, offset + 32);
  const end = slice.indexOf(0);
  return new TextDecoder().decode(end === -1 ? slice : slice.subarray(0, end));
}

export interface RepoAccount {
  address: string;
  owner: string;
  repoId: string;
  name: string;
  defaultBranch: string;
  historyRoot: Uint8Array;
  commitCount: number;
}

export interface BranchAccount {
  address: string;
  repo: string;
  name: string;
  headCommit: Uint8Array;
  headSeq: number;
  authority: string;
}

export interface CommitAccount {
  address: string;
  repo: string;
  commitOid: Uint8Array;
  parentCount: number;
  parentA: Uint8Array;
  parentB: Uint8Array;
  treeOid: Uint8Array;
  author: string;
  authoredAt: number;
  attestationHash: Uint8Array;
  seq: number;
}

export interface TagAccount {
  address: string;
  repo: string;
  name: string;
  targetCommit: Uint8Array;
  tagger: string;
  signed: number;
}

export interface ProvenanceAccount {
  address: string;
  programId: string;
  repo: string;
  commitOid: Uint8Array;
  artifactHash: Uint8Array;
  attester: string;
  verified: number;
}

export function decodeRepo(address: string, bytes: Uint8Array): RepoAccount {
  return {
    address,
    owner: pubkey(bytes, 8),
    repoId: pubkey(bytes, 40),
    name: name(bytes, 72),
    defaultBranch: name(bytes, 104),
    historyRoot: bytes32(bytes, 136),
    commitCount: u64(bytes, 168),
  };
}

export function decodeBranch(address: string, bytes: Uint8Array): BranchAccount {
  return {
    address,
    repo: pubkey(bytes, 8),
    name: name(bytes, 40),
    headCommit: bytes32(bytes, 72),
    headSeq: u64(bytes, 104),
    authority: pubkey(bytes, 112),
  };
}

export function decodeCommit(address: string, bytes: Uint8Array): CommitAccount {
  return {
    address,
    repo: pubkey(bytes, 8),
    commitOid: bytes32(bytes, 40),
    parentCount: bytes[72],
    parentA: bytes32(bytes, 73),
    parentB: bytes32(bytes, 105),
    treeOid: bytes32(bytes, 137),
    author: pubkey(bytes, 169),
    authoredAt: i64(bytes, 201),
    attestationHash: bytes32(bytes, 241),
    seq: u64(bytes, 273),
  };
}

export function decodeTag(address: string, bytes: Uint8Array): TagAccount {
  return {
    address,
    repo: pubkey(bytes, 8),
    name: name(bytes, 40),
    targetCommit: bytes32(bytes, 72),
    tagger: pubkey(bytes, 104),
    signed: bytes[176],
  };
}

export function decodeProvenance(address: string, bytes: Uint8Array): ProvenanceAccount {
  return {
    address,
    programId: pubkey(bytes, 8),
    repo: pubkey(bytes, 40),
    commitOid: bytes32(bytes, 72),
    artifactHash: bytes32(bytes, 104),
    attester: pubkey(bytes, 168),
    verified: bytes[200],
  };
}
