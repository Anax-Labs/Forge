// Runtime configuration for the explorer.

export const PROGRAM_ID =
  process.env.NEXT_PUBLIC_FORGE_PROGRAM_ID ??
  "4smCAEoycSXSvVsyic8ircQmHmENPCHn17Fma83SYVbf";

export const RPC_URL =
  process.env.NEXT_PUBLIC_FORGE_RPC ?? "https://api.devnet.solana.com";
