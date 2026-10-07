// Anchor account discriminators (`account:<Name>`), computed with WebCrypto so
// the explorer can filter `getProgramAccounts` precisely.

import { base58Encode } from "./base58.ts";

async function sha256(data: Uint8Array): Promise<Uint8Array> {
  // Copy into a fresh ArrayBuffer so the value satisfies WebCrypto's BufferSource
  // (TS 5.9 types Uint8Array over ArrayBufferLike).
  const copy = new Uint8Array(data.length);
  copy.set(data);
  return new Uint8Array(await crypto.subtle.digest("SHA-256", copy.buffer));
}

export async function accountDiscriminator(name: string): Promise<Uint8Array> {
  const digest = await sha256(new TextEncoder().encode(`account:${name}`));
  return digest.slice(0, 8);
}

export async function accountDiscriminatorBase58(name: string): Promise<string> {
  return base58Encode(await accountDiscriminator(name));
}
