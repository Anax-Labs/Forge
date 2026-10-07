// Anchor instruction/account discriminators, mirroring the program's
// `global:<name>` / `account:<Name>` derivation.

import { digest } from "./oid.ts";

export function ixDiscriminator(name: string): Uint8Array {
  return digest("sha256", new TextEncoder().encode(`global:${name}`)).slice(0, 8);
}

export function accountDiscriminator(name: string): Uint8Array {
  return digest("sha256", new TextEncoder().encode(`account:${name}`)).slice(0, 8);
}
