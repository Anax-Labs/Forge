//! `.forge/storage-index` — `oid → CID / TXID / local path` (§14.1).
//!
//! Locators in this file are hints. Readers must recompute Git OIDs.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use forge_object::hash::Oid;
use serde::{Deserialize, Serialize};

use crate::backend::Locator;
use crate::error::StorageError;
use crate::object::GitObject;

/// On-disk index version.
pub const INDEX_VERSION: u32 = 1;

/// A stored object's locators.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexEntry {
    /// `blob` / `tree` / `commit` / `tag`.
    pub object_type: String,
    /// Independent pins / checkpoints. Never trusted.
    pub locators: Vec<Locator>,
}

/// The `.forge/storage-index` document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageIndex {
    /// Format version (currently 1).
    pub version: u32,
    /// Hash algorithm tag (`sha256` / `sha1`).
    pub algorithm: String,
    /// Map of tagged oid → locators.
    pub objects: BTreeMap<String, IndexEntry>,
}

impl StorageIndex {
    /// Empty index for `algorithm`.
    pub fn new(algorithm: impl Into<String>) -> Self {
        Self {
            version: INDEX_VERSION,
            algorithm: algorithm.into(),
            objects: BTreeMap::new(),
        }
    }

    /// Loads JSON from `path`.
    ///
    /// # Errors
    /// Returns I/O or parse errors, or [`StorageError::InvalidIndex`].
    pub fn load(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let bytes = fs::read(path.as_ref())?;
        let index: Self = serde_json::from_slice(&bytes)?;
        if index.version != INDEX_VERSION {
            return Err(StorageError::InvalidIndex(format!(
                "unsupported storage-index version {}",
                index.version
            )));
        }
        Ok(index)
    }

    /// Writes canonical pretty JSON.
    ///
    /// # Errors
    /// Returns I/O errors.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), StorageError> {
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec_pretty(self)?;
        fs::write(path, bytes)?;
        Ok(())
    }

    /// Records locators for `obj` (oid recomputed via [`GitObject::verify`]).
    ///
    /// # Errors
    /// Returns [`StorageError::OidMismatch`] if verification fails.
    pub fn record(&mut self, obj: &GitObject, locators: Vec<Locator>) -> Result<(), StorageError> {
        obj.verify()?;
        self.objects.insert(
            obj.oid.to_tagged_string(),
            IndexEntry {
                object_type: obj.object_type.as_str().to_string(),
                locators,
            },
        );
        Ok(())
    }

    /// Locators for `oid`, if any.
    pub fn locators_for(&self, oid: &Oid) -> Option<&[Locator]> {
        self.objects
            .get(&oid.to_tagged_string())
            .map(|entry| entry.locators.as_slice())
    }
}

/// Conventional path of the index under a Forge directory.
pub fn storage_index_path(forge_dir: impl AsRef<Path>) -> std::path::PathBuf {
    forge_dir.as_ref().join(".forge").join("storage-index")
}

/// Conventional local CAS directory.
pub fn cas_path(forge_dir: impl AsRef<Path>) -> std::path::PathBuf {
    forge_dir.as_ref().join(".forge").join("cas")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::LocatorBackend;
    use crate::object::GitObject;
    use forge_object::hash::HashAlgorithm;
    use forge_object::object::ObjectType;

    #[test]
    fn roundtrip_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("storage-index");
        let mut index = StorageIndex::new("sha256");
        let obj = GitObject::from_payload(ObjectType::Blob, b"z".to_vec(), HashAlgorithm::Sha256);
        index
            .record(
                &obj,
                vec![Locator {
                    backend: LocatorBackend::Fs,
                    provider: "cas".into(),
                    id: "bafkrei-z".into(),
                }],
            )
            .unwrap();
        index.save(&path).unwrap();
        let loaded = StorageIndex::load(&path).unwrap();
        assert_eq!(loaded, index);
    }
}
