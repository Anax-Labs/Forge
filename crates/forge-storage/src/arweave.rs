//! Arweave upload via an Irys-compatible bundler (§8.2, §8.3).
//!
//! Live Arweave transactions are reported to cap around 10 MiB. Payloads
//! larger than [`DEFAULT_CHUNK_BYTES`] are split, uploaded as separate
//! items, then referenced by a JSON manifest. Tests inject a mock HTTP
//! transport and may lower the chunk size.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::backend::{Locator, LocatorBackend, StorageBackend};
use crate::error::StorageError;
use crate::http::{HttpBody, HttpMethod, HttpRequest, HttpTransport};

/// Documented Arweave per-transaction payload ceiling (§8.2).
pub const ARWEAVE_TX_LIMIT_BYTES: usize = 10 * 1024 * 1024;
/// Conservative chunk size so bundler metadata still fits under the limit.
pub const DEFAULT_CHUNK_BYTES: usize = 9_500_000;

/// JSON manifest for a chunked upload. The SHA-256 is of the original bytes
/// and is checked on read — the TXID is never trusted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChunkManifest {
    /// Manifest version.
    pub v: u8,
    /// Bundler transaction ids in order.
    pub chunks: Vec<String>,
    /// Hex SHA-256 of the concatenated payload.
    pub sha256: String,
}

/// Irys-compatible bundler client.
pub struct ArweaveBundler<T> {
    provider: String,
    url: String,
    max_item_bytes: usize,
    transport: T,
}

impl<T: HttpTransport> ArweaveBundler<T> {
    /// `url` is the bundler origin, e.g. `https://node2.irys.xyz` or a mock.
    pub fn new(provider: impl Into<String>, url: impl Into<String>, transport: T) -> Self {
        Self {
            provider: provider.into(),
            url: url.into().trim_end_matches('/').to_string(),
            max_item_bytes: DEFAULT_CHUNK_BYTES,
            transport,
        }
    }

    /// Override the aggregation threshold (tests use a small value).
    #[must_use]
    pub fn with_max_item_bytes(mut self, max_item_bytes: usize) -> Self {
        self.max_item_bytes = max_item_bytes.min(ARWEAVE_TX_LIMIT_BYTES);
        self
    }

    fn post_item(&self, bytes: &[u8]) -> Result<String, StorageError> {
        if bytes.len() > ARWEAVE_TX_LIMIT_BYTES {
            return Err(StorageError::Config(format!(
                "Arweave item {} exceeds {} byte limit",
                bytes.len(),
                ARWEAVE_TX_LIMIT_BYTES
            )));
        }
        let response = self.transport.execute(&HttpRequest {
            method: HttpMethod::Post,
            url: format!("{}/tx", self.url),
            headers: vec![("content-type".into(), "application/octet-stream".into())],
            body: HttpBody::Raw(bytes),
        })?;
        let value: serde_json::Value = serde_json::from_slice(&response.body)?;
        value
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned)
            .ok_or_else(|| StorageError::Http("bundler response missing id".into()))
    }

    fn get_item(&self, id: &str) -> Result<Vec<u8>, StorageError> {
        let response = self.transport.execute(&HttpRequest {
            method: HttpMethod::Get,
            url: format!("{}/{id}", self.url),
            headers: Vec::new(),
            body: HttpBody::Empty,
        })?;
        Ok(response.body)
    }

    fn digest_hex(bytes: &[u8]) -> String {
        hex::encode(Sha256::digest(bytes))
    }

    /// Uploads `bytes`, aggregating over the per-tx limit when needed.
    ///
    /// # Errors
    /// Returns HTTP or configuration errors.
    pub fn upload(&self, bytes: &[u8]) -> Result<Locator, StorageError> {
        self.put(bytes)
    }
}

impl<T: HttpTransport> StorageBackend for ArweaveBundler<T> {
    fn provider_id(&self) -> &str {
        &self.provider
    }

    fn locator_backend(&self) -> LocatorBackend {
        LocatorBackend::Arweave
    }

    fn put(&self, bytes: &[u8]) -> Result<Locator, StorageError> {
        let id = if bytes.len() <= self.max_item_bytes {
            self.post_item(bytes)?
        } else {
            let mut chunks = Vec::new();
            for part in bytes.chunks(self.max_item_bytes) {
                chunks.push(self.post_item(part)?);
            }
            let manifest = ChunkManifest {
                v: 1,
                chunks,
                sha256: Self::digest_hex(bytes),
            };
            let encoded = serde_json::to_vec(&manifest)?;
            self.post_item(&encoded)?
        };
        Ok(Locator {
            backend: LocatorBackend::Arweave,
            provider: self.provider.clone(),
            id,
        })
    }

    fn get(&self, locator_id: &str) -> Result<Vec<u8>, StorageError> {
        let body = self.get_item(locator_id)?;
        if let Ok(manifest) = serde_json::from_slice::<ChunkManifest>(&body) {
            if manifest.v != 1 {
                return Err(StorageError::InvalidIndex(format!(
                    "unsupported arweave manifest v={}",
                    manifest.v
                )));
            }
            let mut payload = Vec::new();
            for chunk in &manifest.chunks {
                payload.extend_from_slice(&self.get_item(chunk)?);
            }
            let actual = Self::digest_hex(&payload);
            if actual != manifest.sha256 {
                return Err(StorageError::CidMismatch {
                    expected: manifest.sha256,
                    actual,
                });
            }
            return Ok(payload);
        }
        Ok(body)
    }

    fn exists(&self, locator_id: &str) -> Result<bool, StorageError> {
        match self.get_item(locator_id) {
            Ok(_) => Ok(true),
            Err(StorageError::Http(msg)) if msg.contains("status 404") => Ok(false),
            Err(err) => Err(err),
        }
    }

    fn delete(&self, _locator_id: &str) -> Result<(), StorageError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{HttpResponse, MockTransport};
    use std::collections::HashMap;
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    };

    fn bundler_transport() -> MockTransport {
        let store = Mutex::new(HashMap::<String, Vec<u8>>::new());
        let seq = AtomicU64::new(1);
        MockTransport::new(move |req| {
            if req.url.ends_with("/tx") && matches!(req.method, HttpMethod::Post) {
                let bytes = match req.body {
                    HttpBody::Raw(b) => b.to_vec(),
                    _ => Vec::new(),
                };
                let id = format!("tx{}", seq.fetch_add(1, Ordering::SeqCst));
                store.lock().expect("store").insert(id.clone(), bytes);
                let body = serde_json::json!({ "id": id }).to_string().into_bytes();
                return Ok(HttpResponse { status: 200, body });
            }
            if matches!(req.method, HttpMethod::Get) {
                let id = req.url.rsplit('/').next().unwrap_or_default().to_string();
                let store = store.lock().expect("store");
                if let Some(bytes) = store.get(&id) {
                    return Ok(HttpResponse {
                        status: 200,
                        body: bytes.clone(),
                    });
                }
                return Err(StorageError::Http("status 404".into()));
            }
            Err(StorageError::Http(format!("unexpected {}", req.url)))
        })
    }

    #[test]
    fn small_upload_roundtrip() {
        let client = ArweaveBundler::new("irys", "http://bundler.test", bundler_transport());
        let loc = client.put(b"checkpoint").unwrap();
        assert_eq!(client.get(&loc.id).unwrap(), b"checkpoint");
    }

    #[test]
    fn large_payload_is_chunked_and_reassembled() {
        let client = ArweaveBundler::new("irys", "http://bundler.test", bundler_transport())
            .with_max_item_bytes(8);
        let payload = vec![7u8; 20];
        let loc = client.put(&payload).unwrap();
        assert_eq!(client.get(&loc.id).unwrap(), payload);
    }
}
