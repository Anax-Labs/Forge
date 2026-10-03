//! Multi-pin wrapper: upload to ≥2 independent backends; fetch from any (§8.3).

use crate::backend::{Locator, StorageBackend};
use crate::error::StorageError;

/// A set of independent pinning providers.
pub struct MultiPin<'a> {
    backends: Vec<&'a dyn StorageBackend>,
}

impl<'a> MultiPin<'a> {
    /// Requires at least two backends (availability policy).
    ///
    /// # Errors
    /// Returns [`StorageError::TooFewPins`] when `backends.len() < 2`.
    pub fn new(backends: Vec<&'a dyn StorageBackend>) -> Result<Self, StorageError> {
        if backends.len() < 2 {
            return Err(StorageError::TooFewPins);
        }
        Ok(Self { backends })
    }

    /// Uploads `bytes` to every backend.
    ///
    /// # Errors
    /// Fails if any pin fails — a push must not proceed with a single pin.
    pub fn put(&self, bytes: &[u8]) -> Result<Vec<Locator>, StorageError> {
        let mut locators = Vec::with_capacity(self.backends.len());
        for backend in &self.backends {
            locators.push(backend.put(bytes)?);
        }
        Ok(locators)
    }

    /// Fetches from the first locator whose provider still has the bytes.
    ///
    /// # Errors
    /// Returns [`StorageError::NotFound`] when every pin is gone.
    pub fn get(&self, locators: &[Locator]) -> Result<Vec<u8>, StorageError> {
        let mut last_not_found = StorageError::NotFound("no locators".into());
        for locator in locators {
            let Some(backend) = self
                .backends
                .iter()
                .find(|b| b.provider_id() == locator.provider)
            else {
                continue;
            };
            match backend.get(&locator.id) {
                Ok(bytes) => return Ok(bytes),
                Err(err @ StorageError::NotFound(_)) => last_not_found = err,
                Err(err) => return Err(err),
            }
        }
        Err(last_not_found)
    }

    /// Backends in configuration order.
    pub fn backends(&self) -> &[&'a dyn StorageBackend] {
        &self.backends
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemoryBackend;

    #[test]
    fn rejects_single_backend() {
        let a = MemoryBackend::new("a");
        assert!(matches!(
            MultiPin::new(vec![&a]),
            Err(StorageError::TooFewPins)
        ));
    }

    #[test]
    fn fetch_survives_killed_pin() {
        let a = MemoryBackend::new("a");
        let b = MemoryBackend::new("b");
        let multi = MultiPin::new(vec![&a, &b]).unwrap();
        let locs = multi.put(b"payload").unwrap();
        a.kill();
        assert_eq!(multi.get(&locs).unwrap(), b"payload");
        b.kill();
        assert!(matches!(multi.get(&locs), Err(StorageError::NotFound(_))));
    }
}
