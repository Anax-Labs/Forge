//! Wallet keypair loading for local attestation signing (§15).
//!
//! Convention (ADR 0007): a Solana CLI JSON keypair file. `FORGE_WALLET`
//! overrides `.forge/config.wallet`. `forge init` writes `.forge/id.json`.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use solana_keypair::Keypair;

/// Environment variable that overrides the configured wallet path.
pub const WALLET_ENV: &str = "FORGE_WALLET";

/// Default wallet path relative to the repository root.
pub const DEFAULT_WALLET: &str = ".forge/id.json";

/// Loads a Solana JSON keypair from `path`.
///
/// # Errors
/// I/O or JSON/decode errors.
pub fn load_keypair(path: &Path) -> Result<Keypair> {
    let bytes = fs::read(path).with_context(|| format!("read wallet {}", path.display()))?;
    let parsed: Vec<u8> = serde_json::from_slice(&bytes)
        .with_context(|| format!("parse wallet JSON {}", path.display()))?;
    Keypair::try_from(parsed.as_slice()).map_err(|err| anyhow::anyhow!("invalid keypair: {err}"))
}

/// Writes `keypair` as Solana CLI JSON.
///
/// # Errors
/// I/O errors.
pub fn save_keypair(path: &Path, keypair: &Keypair) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_vec(&keypair.to_bytes().as_ref())?;
    fs::write(path, json)?;
    Ok(())
}

/// Resolves the wallet path: `FORGE_WALLET`, else `config.wallet` under `root`.
pub fn resolve_wallet_path(root: &Path, configured: &str) -> PathBuf {
    if let Ok(from_env) = std::env::var(WALLET_ENV) {
        if !from_env.is_empty() {
            let p = PathBuf::from(from_env);
            if p.is_absolute() {
                return p;
            }
            return root.join(p);
        }
    }
    let p = Path::new(configured);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        root.join(p)
    }
}

/// Loads the keypair used to sign attestations.
///
/// # Errors
/// Missing or invalid keypair file.
pub fn load_repo_wallet(root: &Path, configured: &str) -> Result<Keypair> {
    load_keypair(&resolve_wallet_path(root, configured))
}
