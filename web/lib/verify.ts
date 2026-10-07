// Client-side re-verification using WebCrypto (browser + Bun/Node).
//
// Recomputes the append-only history root from the anchored commit set and
// compares it to `RepositoryAccount.history_root`. The explorer never trusts the
// RPC for this; the verifier re-derives it.

const GENESIS_DOMAIN = new TextEncoder().encode("forge-genesis\0");
const APPEND_DOMAIN = new TextEncoder().encode("forge-append\0");

function concat(...parts: Uint8Array[]): Uint8Array {
  const total = parts.reduce((n, p) => n + p.length, 0);
  const out = new Uint8Array(total);
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

function leU64(value: number): Uint8Array {
  const out = new Uint8Array(8);
  let v = BigInt(value);
  for (let i = 0; i < 8; i += 1) {
    out[i] = Number(v & 0xffn);
    v >>= 8n;
  }
  return out;
}

async function sha256(data: Uint8Array): Promise<Uint8Array> {
  const copy = new Uint8Array(data.length);
  copy.set(data);
  return new Uint8Array(await crypto.subtle.digest("SHA-256", copy.buffer));
}

export async function historyRootChain(
  repoPda: Uint8Array,
  commits: Array<{ oid: Uint8Array; seq: number }>,
): Promise<Uint8Array> {
  let root = await sha256(concat(GENESIS_DOMAIN, repoPda));
  const sorted = [...commits].sort((a, b) => a.seq - b.seq);
  for (const commit of sorted) {
    root = await sha256(concat(APPEND_DOMAIN, root, commit.oid, leU64(commit.seq)));
  }
  return root;
}

export function bytesEqual(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i += 1) if (a[i] !== b[i]) return false;
  return true;
}

export function toHex(bytes: Uint8Array): string {
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

export function shortHex(bytes: Uint8Array, keep = 8): string {
  const hex = toHex(bytes);
  return `${hex.slice(0, keep)}…${hex.slice(-keep)}`;
}

export function isZero(bytes: Uint8Array): boolean {
  return bytes.every((b) => b === 0);
}
