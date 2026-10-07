// Forge TypeScript SDK (dependency-free core).
//
// The canonical encoding/verification primitives are pure TypeScript with no
// runtime dependencies, so they run under `node --test` and can be embedded in
// browsers. Onchain RPC/transaction submission is layered on `@solana/kit` by
// the consuming app (Phase 10b).

export * as cbor from "./cbor.ts";
export * from "./oid.ts";
export * from "./object.ts";
export * from "./history.ts";
export * from "./attestation.ts";
export * from "./discriminator.ts";
