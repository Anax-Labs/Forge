//! In-memory CAS used by tests and as a drop-in `StorageBackend`.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::backend::{Locator, LocatorBackend, StorageBackend};
use crate::cid::Cid;
use crate::error::StorageError;

/// Thread-safe map from CID multibase → bytes.
#[derive(Debug)]
pub struct MemoryBackend {
    provider: String,
    store: Mutex<HashMap<String, Vec<u8>>>,
}

impl MemoryBackend {
    /// Creates an empty store named `provider`.
    pub fn new(provider: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            store: Mutex::new(HashMap::new()),
        }
    }

    /// Drops every object (killed-pin simulation).
    pub fn kill(&self) {
        self.store.lock().expect("memory backend mutex").clear();
    }

    /// Overwrites `id` without recomputing a CID (corruption / attack tests).
    pub fn insert_unchecked(&self, id: &str, bytes: Vec<u8>) {
        self.store
            .lock()
            .expect("memory backend mutex")
            .insert(id.to_string(), bytes);
    }
}

impl Default for MemoryBackend {
    fn default() -> Self {
        Self::new("memory")
    }
}

impl StorageBackend for MemoryBackend {
    fn provider_id(&self) -> &str {
        &self.provider
    }

    fn locator_backend(&self) -> LocatorBackend {
        LocatorBackend::Memory
    }

    fn put(&self, bytes: &[u8]) -> Result<Locator, StorageError> {
        let id = Cid::of_raw_sha256(bytes).to_multibase();
        self.store
            .lock()
            .expect("memory backend mutex")
            .insert(id.clone(), bytes.to_vec());
        Ok(Locator {
            backend: LocatorBackend::Memory,
            provider: self.provider.clone(),
            id,
        })
    }

    fn get(&self, locator_id: &str) -> Result<Vec<u8>, StorageError> {
        self.store
            .lock()
            .expect("memory backend mutex")
            .get(locator_id)
            .cloned()
            .ok_or_else(|| StorageError::NotFound(locator_id.to_string()))
    }

    fn exists(&self, locator_id: &str) -> Result<bool, StorageError> {
        Ok(self
            .store
            .lock()
            .expect("memory backend mutex")
            .contains_key(locator_id))
    }

    fn delete(&self, locator_id: &str) -> Result<(), StorageError> {
        self.store
            .lock()
            .expect("memory backend mutex")
            .remove(locator_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_get_roundtrip() {
        let backend = MemoryBackend::new("m0");
        let loc = backend.put(b"abc").unwrap();
        assert_eq!(backend.get(&loc.id).unwrap(), b"abc");
        backend.delete(&loc.id).unwrap();
        assert!(backend.get(&loc.id).is_err());
    }
}
