//! Content-addressed storage façade (Phase 6 + 8 push/clone).
//!
//! Default pins are two directories (`cas` + `cas-2`) so MultiPin's ≥2 rule
//! holds offline. `FORGE_STORAGE` selects a shared root so clone can fetch
//! the same CAR locators. `FORGE_IPFS_API` / `FORGE_IPFS_API_2` switch to Kubo.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use forge_object::hash::Oid;
use forge_storage::backend::StorageBackend;
use forge_storage::fs::FsBackend;
use forge_storage::graph::{ObjectGraph, ObjectSource};
use forge_storage::http::ReqwestTransport;
use forge_storage::ipfs::IpfsClient;
use forge_storage::multi::MultiPin;
use forge_storage::object::GitObject;
use forge_storage::{fetch_object, storage_index_path, StorageIndex};

pub use forge_storage::report_for_forge_dir;

/// Dual-pin store plus the on-disk index.
pub struct PinSet {
    /// First backend.
    pub a: Box<dyn StorageBackend>,
    /// Second backend.
    pub b: Box<dyn StorageBackend>,
    /// Locator cache.
    pub index: StorageIndex,
    index_path: PathBuf,
    /// Shared attestation dump (clone reads this).
    pub sidecar_root: PathBuf,
}

/// Resolves the storage root: test override, `FORGE_STORAGE`, else `{repo}/.forge`.
#[must_use]
pub fn storage_root(repo: &Path) -> PathBuf {
    if let Some(path) = TEST_STORAGE.with(|slot| slot.borrow().clone()) {
        return path;
    }
    std::env::var("FORGE_STORAGE").map_or_else(|_| repo.join(".forge"), PathBuf::from)
}

thread_local! {
    static TEST_STORAGE: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// Shared CAS root for in-process clone tests.
pub fn set_test_storage(path: Option<PathBuf>) {
    TEST_STORAGE.with(|slot| *slot.borrow_mut() = path);
}

/// Opens ≥2 backends and loads/creates `storage-index`.
///
/// # Errors
/// I/O errors.
pub fn open_pins(repo: &Path) -> Result<PinSet> {
    let root = storage_root(repo);
    std::fs::create_dir_all(&root)?;
    let sidecar_root = root.join("attestations");
    std::fs::create_dir_all(&sidecar_root)?;
    let index_path = if root.ends_with(".forge") {
        storage_index_path(repo)
    } else {
        root.join("storage-index")
    };
    let index = if index_path.exists() {
        StorageIndex::load(&index_path)?
    } else {
        StorageIndex::new("sha256")
    };

    if let Ok(url) = std::env::var("FORGE_IPFS_API") {
        let a = IpfsClient::new("ipfs-0", url, ReqwestTransport::default());
        let b = if let Ok(url2) = std::env::var("FORGE_IPFS_API_2") {
            Box::new(IpfsClient::new("ipfs-1", url2, ReqwestTransport::default()))
                as Box<dyn StorageBackend>
        } else {
            Box::new(FsBackend::open("cas", cas_dir(&root))?) as Box<dyn StorageBackend>
        };
        return Ok(PinSet {
            a: Box::new(a),
            b,
            index,
            index_path,
            sidecar_root,
        });
    }

    let a = FsBackend::open("cas", cas_dir(&root))?;
    let b = FsBackend::open("cas-2", root.join("cas-2"))?;
    Ok(PinSet {
        a: Box::new(a),
        b: Box::new(b),
        index,
        index_path,
        sidecar_root,
    })
}

fn cas_dir(root: &Path) -> PathBuf {
    root.join("cas")
}

impl PinSet {
    /// Upload a CAR of `graph` to both pins **before** any chain tx (§13).
    ///
    /// # Errors
    /// Pin or index errors.
    pub fn upload(
        &mut self,
        graph: &ObjectGraph,
        root: &Oid,
    ) -> Result<Vec<forge_storage::Locator>> {
        let backends = [&*self.a, &*self.b];
        let pins = MultiPin::new(vec![backends[0], backends[1]])?;
        let locators = forge_storage::upload_bundle(&pins, graph, root, &mut self.index)?;
        self.index.save(&self.index_path)?;
        Ok(locators)
    }

    /// Fetch one object by oid (re-hashed).
    pub fn fetch(&self, oid: &Oid) -> Result<GitObject, forge_storage::StorageError> {
        let backends = [&*self.a, &*self.b];
        let pins = MultiPin::new(vec![backends[0], backends[1]])?;
        fetch_object(&pins, &self.index, oid)
    }

    /// Persist the index.
    pub fn save_index(&self) -> Result<()> {
        self.index.save(&self.index_path)?;
        Ok(())
    }
}

/// `ObjectSource` over a [`PinSet`].
pub struct PinSource<'a> {
    pins: &'a PinSet,
}

impl<'a> PinSource<'a> {
    /// Borrow pins.
    #[must_use]
    pub fn new(pins: &'a PinSet) -> Self {
        Self { pins }
    }
}

impl ObjectSource for PinSource<'_> {
    fn get(&self, oid: &Oid) -> Result<GitObject, forge_storage::StorageError> {
        self.pins.fetch(oid)
    }
}

/// Copy attestation sidecars into the shared storage root so clone can load them.
pub fn publish_sidecars(repo: &Path, pins: &PinSet, oid: &Oid) -> Result<()> {
    let hex = oid.to_hex();
    let src = crate::config::attestations_dir(repo);
    for ext in ["cbor", "sig"] {
        let from = src.join(format!("{hex}.{ext}"));
        if from.exists() {
            std::fs::copy(&from, pins.sidecar_root.join(format!("{hex}.{ext}")))
                .with_context(|| format!("publish sidecar {}", from.display()))?;
        }
    }
    Ok(())
}

/// Load sidecars from shared storage into a repository `.forge/attestations`.
pub fn import_sidecars(repo: &Path, pins: &PinSet, oid: &Oid) -> Result<()> {
    let hex = oid.to_hex();
    let dest = crate::config::attestations_dir(repo);
    std::fs::create_dir_all(&dest)?;
    for ext in ["cbor", "sig"] {
        let from = pins.sidecar_root.join(format!("{hex}.{ext}"));
        if from.exists() {
            std::fs::copy(&from, dest.join(format!("{hex}.{ext}")))?;
        }
    }
    Ok(())
}
