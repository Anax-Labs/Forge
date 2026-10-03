//! Local directory CAS (`oid → local path` in `.forge/storage-index`, §14.1).

use std::fs;
use std::path::{Path, PathBuf};

use crate::backend::{Locator, LocatorBackend, StorageBackend};
use crate::cid::Cid;
use crate::error::StorageError;

/// Files named by CIDv1 multibase under `root`.
#[derive(Debug, Clone)]
pub struct FsBackend {
    provider: String,
    root: PathBuf,
}

impl FsBackend {
    /// Uses `root` as the CAS directory, creating it if needed.
    ///
    /// # Errors
    /// Returns I/O errors from `create_dir_all`.
    pub fn open(provider: impl Into<String>, root: impl AsRef<Path>) -> Result<Self, StorageError> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root)?;
        Ok(Self {
            provider: provider.into(),
            root,
        })
    }

    fn path_for(&self, locator_id: &str) -> Result<PathBuf, StorageError> {
        if locator_id.is_empty()
            || locator_id.contains(['/', '\\'])
            || locator_id.contains("..")
            || !locator_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        {
            return Err(StorageError::InvalidCid(format!(
                "unsafe locator id {locator_id}"
            )));
        }
        Ok(self.root.join(locator_id))
    }
}

impl StorageBackend for FsBackend {
    fn provider_id(&self) -> &str {
        &self.provider
    }

    fn locator_backend(&self) -> LocatorBackend {
        LocatorBackend::Fs
    }

    fn put(&self, bytes: &[u8]) -> Result<Locator, StorageError> {
        let id = Cid::of_raw_sha256(bytes).to_multibase();
        fs::write(self.path_for(&id)?, bytes)?;
        Ok(Locator {
            backend: LocatorBackend::Fs,
            provider: self.provider.clone(),
            id,
        })
    }

    fn get(&self, locator_id: &str) -> Result<Vec<u8>, StorageError> {
        match fs::read(self.path_for(locator_id)?) {
            Ok(bytes) => Ok(bytes),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                Err(StorageError::NotFound(locator_id.to_string()))
            }
            Err(err) => Err(err.into()),
        }
    }

    fn exists(&self, locator_id: &str) -> Result<bool, StorageError> {
        Ok(self.path_for(locator_id)?.is_file())
    }

    fn delete(&self, locator_id: &str) -> Result<(), StorageError> {
        match fs::remove_file(self.path_for(locator_id)?) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err.into()),
        }
    }
}
