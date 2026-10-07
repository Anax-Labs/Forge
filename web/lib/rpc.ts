// Minimal JSON-RPC client (fetch only). The explorer is read-only and never
// trusts the RPC: decoded values are re-derived where a verifier needs them.

export interface RpcAccount {
  pubkey: string;
  account: {
    data: [string, "base64"];
    owner: string;
    executable: boolean;
    lamports: number;
  };
}

export interface MemcmpFilter {
  memcmp: { offset: number; bytes: string; encoding?: "base58" | "base64" };
}

export async function rpc<T>(
  url: string,
  method: string,
  params: unknown[],
): Promise<T> {
  const response = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
  });
  const json = (await response.json()) as { result?: T; error?: { message: string } };
  if (json.error) throw new Error(json.error.message);
  return json.result as T;
}

export async function getProgramAccounts(
  url: string,
  programId: string,
  filters: MemcmpFilter[] = [],
): Promise<RpcAccount[]> {
  return rpc<RpcAccount[]>(url, "getProgramAccounts", [
    programId,
    { encoding: "base64", filters },
  ]);
}

export async function getAccountInfo(
  url: string,
  pubkey: string,
): Promise<RpcAccount["account"] | null> {
  const result = await rpc<{ value: RpcAccount["account"] | null }>(
    url,
    "getAccountInfo",
    [pubkey, { encoding: "base64" }],
  );
  return result.value;
}

export function b64ToBytes(b64: string): Uint8Array {
  if (typeof Buffer !== "undefined") {
    return Uint8Array.from(Buffer.from(b64, "base64"));
  }
  const binary = atob(b64);
  const out = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) out[i] = binary.charCodeAt(i);
  return out;
}
