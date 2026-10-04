//! `.forge/config` — local Forge metadata (§14.1).
//!
//! Objects and refs live in `.git/`. This file only records Forge-specific
//! settings: wallet path, remotes, and (after Phase 8) the repository PDA.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

/// On-disk config version.
pub const CONFIG_VERSION: u32 = 1;

/// A named remote mapping to an onchain repository (PDA as base58).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Remote {
    /// Repository PDA (base58) or a local placeholder until `initialize_repository`.
    pub repo: String,
}

/// `.forge/config` document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForgeConfig {
    /// Format version (currently 1).
    pub version: u32,
    /// Object-hash algorithm (`sha256` for Forge-native repos).
    pub algorithm: String,
    /// Onchain repository name (NUL-padded PDA seed). Defaults to `forge`.
    #[serde(default = "default_name")]
    pub name: String,
    /// Default branch name (`main`).
    pub default_branch: String,
    /// Wallet keypair path, relative to the repository root unless absolute.
    pub wallet: String,
    /// Onchain repository PDA once known (Phase 8). Empty means local-only.
    #[serde(default)]
    pub repo_pda: String,
    /// Storage backend tag (`hybrid` / `ipfs` / `arweave`).
    #[serde(default = "default_storage")]
    pub storage_backend: String,
    /// Named remotes.
    #[serde(default)]
    pub remotes: BTreeMap<String, Remote>,
}

fn default_storage() -> String {
    "hybrid".into()
}

fn default_name() -> String {
    "forge".into()
}

impl ForgeConfig {
    /// Fresh config for `forge init`.
    pub fn new_local(wallet: impl Into<String>, default_branch: impl Into<String>) -> Self {
        Self {
            version: CONFIG_VERSION,
            algorithm: "sha256".into(),
            name: default_name(),
            default_branch: default_branch.into(),
            wallet: wallet.into(),
            repo_pda: String::new(),
            storage_backend: default_storage(),
            remotes: BTreeMap::new(),
        }
    }

    /// Repo string used in attestations: the PDA if set, otherwise `"local"`.
    pub fn attestation_repo(&self) -> &str {
        if self.repo_pda.is_empty() {
            "local"
        } else {
            &self.repo_pda
        }
    }

    /// Loads `{root}/.forge/config`.
    ///
    /// # Errors
    /// Missing file or invalid JSON / version.
    pub fn load(root: &Path) -> Result<Self> {
        let path = config_path(root);
        let bytes = fs::read(&path)
            .with_context(|| format!("missing Forge config at {}", path.display()))?;
        let config: Self = serde_json::from_slice(&bytes).context("invalid .forge/config")?;
        if config.version != CONFIG_VERSION {
            anyhow::bail!("unsupported .forge/config version {}", config.version);
        }
        Ok(config)
    }

    /// Writes pretty JSON to `{root}/.forge/config`.
    ///
    /// # Errors
    /// I/O errors.
    pub fn save(&self, root: &Path) -> Result<()> {
        let dir = forge_dir(root);
        fs::create_dir_all(&dir)?;
        fs::create_dir_all(dir.join("attestations"))?;
        let bytes = serde_json::to_vec_pretty(self)?;
        fs::write(config_path(root), bytes)?;
        Ok(())
    }
}

/// `{root}/.forge`.
pub fn forge_dir(root: &Path) -> PathBuf {
    root.join(".forge")
}

/// `{root}/.forge/config`.
pub fn config_path(root: &Path) -> PathBuf {
    forge_dir(root).join("config")
}

/// `{root}/.forge/onchain-refs`.
pub fn onchain_refs_path(root: &Path) -> PathBuf {
    forge_dir(root).join("onchain-refs")
}

/// `{root}/.forge/attestations`.
pub fn attestations_dir(root: &Path) -> PathBuf {
    forge_dir(root).join("attestations")
}

/// Ensures `path` is inside `root` after normalization (no `..` escape).
///
/// # Errors
/// Returns an error when the path would leave the work tree.
pub fn ensure_inside(root: &Path, path: &Path) -> Result<PathBuf> {
    let root = root
        .canonicalize()
        .context("canonicalize repository root")?;
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let parent = joined.parent().unwrap_or(&root);
    if !parent.exists() {
        return Err(anyhow!(
            "path {} is not inside the repository",
            path.display()
        ));
    }
    let canon_parent = parent.canonicalize()?;
    if !canon_parent.starts_with(&root) {
        return Err(anyhow!(
            "refusing path {} which escapes the repository",
            path.display()
        ));
    }
    Ok(joined)
}

/// Empty onchain-refs placeholder (filled in Phase 8).
pub fn write_empty_onchain_refs(root: &Path) -> Result<()> {
    let doc = serde_json::json!({
        "version": 1,
        "branches": {}
    });
    fs::write(onchain_refs_path(root), serde_json::to_vec_pretty(&doc)?)?;
    Ok(())
}

/// Cached onchain heads for `forge status`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OnchainRefs {
    /// Format version.
    pub version: u32,
    /// Repository PDA.
    #[serde(default)]
    pub repository: String,
    /// Anchored commit count.
    #[serde(default)]
    pub commit_count: u64,
    /// History root tagged string.
    #[serde(default)]
    pub history_root: String,
    /// Branch name → head.
    #[serde(default)]
    pub branches: BTreeMap<String, OnchainBranchRef>,
}

/// One cached branch head.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OnchainBranchRef {
    /// Tagged commit oid.
    pub head: String,
    /// `head_seq`.
    pub head_seq: u64,
}

impl OnchainRefs {
    /// Load or empty.
    pub fn load(root: &Path) -> Result<Self> {
        let path = onchain_refs_path(root);
        if !path.exists() {
            return Ok(Self {
                version: 1,
                ..Self::default()
            });
        }
        let bytes = fs::read(path)?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Persist pretty JSON.
    pub fn save(&self, root: &Path) -> Result<()> {
        fs::write(onchain_refs_path(root), serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
}
