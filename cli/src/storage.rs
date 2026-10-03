//! Content-addressed storage backends (IPFS hot / Arweave cold).
//!
//! The implementation lives in `forge-storage` (Phase 6). This module is the
//! CLI façade: `forge gc --verify-availability` and, in Phase 8, `push`/`clone`.

pub use forge_storage::report_for_forge_dir;
