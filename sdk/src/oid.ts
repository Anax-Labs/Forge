// Algorithm-tagged digests and the canonical 32-byte form, mirroring
// crates/forge-object/src/hash.rs.

import { createHash } from "node:crypto";

export type Algorithm = "sha1" | "sha256";

export interface Oid {
  algorithm: Algorithm;
  bytes: Uint8Array;
}

export function digest(algorithm: Algorithm, data: Uint8Array): Uint8Array {
  const hash = createHash(algorithm === "sha1" ? "sha1" : "sha256");
  hash.update(data);
  return new Uint8Array(hash.digest());
}

export function oid(algorithm: Algorithm, bytes: Uint8Array): Oid {
  const expected = algorithm === "sha1" ? 20 : 32;
  if (bytes.length !== expected) {
    throw new Error(`invalid oid length ${bytes.length} for ${algorithm}`);
  }
  return { algorithm, bytes };
}

export function fromHex(algorithm: Algorithm, hex: string): Oid {
  return oid(algorithm, hexToBytes(hex));
}

export function toHex(bytes: Uint8Array): string {
  return Buffer.from(bytes).toString("hex");
}

export function tagged(value: Oid): string {
  return `${value.algorithm}:${toHex(value.bytes)}`;
}

export function toBytes32(value: Oid): Uint8Array {
  if (value.algorithm === "sha256") return value.bytes;
  const out = new Uint8Array(32);
  out[0] = 0x01; // SHA-1 version tag
  out.set(value.bytes, 1);
  return out;
}

export function hexToBytes(hex: string): Uint8Array {
  return Uint8Array.from(Buffer.from(hex, "hex"));
}

export function concat(...parts: Uint8Array[]): Uint8Array {
  const total = parts.reduce((n, p) => n + p.length, 0);
  const out = new Uint8Array(total);
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

export function leU64(value: number): Uint8Array {
  const out = new Uint8Array(8);
  let v = BigInt(value);
  for (let i = 0; i < 8; i++) {
    out[i] = Number(v & 0xffn);
    v >>= 8n;
  }
  return out;
}
