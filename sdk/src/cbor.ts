// Canonical CBOR (RFC 8949 §4.2.1 core deterministic encoding), mirroring
// crates/forge-object/src/cbor.rs. Shortest forms, definite lengths, map keys
// sorted bytewise by their encoded bytes.

function encodeHead(major: number, value: number): number[] {
  const tag = major << 5;
  if (value < 24) return [tag | value];
  if (value <= 0xff) return [tag | 24, value & 0xff];
  if (value <= 0xffff) return [tag | 25, (value >>> 8) & 0xff, value & 0xff];
  if (value <= 0xffffffff) {
    return [tag | 26, (value >>> 24) & 0xff, (value >>> 16) & 0xff, (value >>> 8) & 0xff, value & 0xff];
  }
  // 64-bit
  const out = [tag | 27];
  for (let shift = 56; shift >= 0; shift -= 8) {
    out.push(Math.floor(value / 2 ** shift) & 0xff);
  }
  return out;
}

export function encodeUint(value: number): number[] {
  return encodeHead(0, value);
}

export function encodeInt(value: number): number[] {
  return value >= 0 ? encodeHead(0, value) : encodeHead(1, -1 - value);
}

export function encodeText(value: string): number[] {
  const bytes = Array.from(new TextEncoder().encode(value));
  return [...encodeHead(3, bytes.length), ...bytes];
}

export function encodeArrayHead(len: number): number[] {
  return encodeHead(4, len);
}

export function encodeMapHead(len: number): number[] {
  return encodeHead(5, len);
}

/** Sorts `[encodedKey, encodedValue]` pairs by encoded key and emits the map. */
export function encodeMap(entries: Array<[number[], number[]]>): number[] {
  const sorted = [...entries].sort((a, b) => compareBytes(a[0], b[0]));
  const out = encodeMapHead(sorted.length);
  for (const [key, value] of sorted) out.push(...key, ...value);
  return out;
}

function compareBytes(a: number[], b: number[]): number {
  const n = Math.min(a.length, b.length);
  for (let i = 0; i < n; i++) {
    if (a[i] !== b[i]) return a[i] - b[i];
  }
  return a.length - b.length;
}

export function toBytes(values: number[]): Uint8Array {
  return Uint8Array.from(values);
}
