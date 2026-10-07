// Git object framing and object-id computation, mirroring
// crates/forge-object/src/object.rs.

import { type Algorithm, type Oid, digest, oid } from "./oid.ts";

export type ObjectType = "blob" | "tree" | "commit" | "tag";

export function serialize(type: ObjectType, payload: Uint8Array): Uint8Array {
  const header = new TextEncoder().encode(`${type} ${payload.length}\0`);
  const out = new Uint8Array(header.length + payload.length);
  out.set(header, 0);
  out.set(payload, header.length);
  return out;
}

export function objectOid(
  type: ObjectType,
  payload: Uint8Array,
  algorithm: Algorithm,
): Oid {
  return oid(algorithm, digest(algorithm, serialize(type, payload)));
}

export function blobOid(content: Uint8Array, algorithm: Algorithm): Oid {
  return objectOid("blob", content, algorithm);
}
