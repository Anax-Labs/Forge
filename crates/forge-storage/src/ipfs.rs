//! Kubo HTTP RPC client (IPFS hot layer, §8.3).
//!
//! Upload uses `/api/v0/add` (CIDv1). Fetch uses `/api/v0/cat`. The returned
//! CID is a locator only; Git OIDs are recomputed by the caller.

use crate::backend::{Locator, LocatorBackend, StorageBackend};
use crate::error::StorageError;
use crate::http::{HttpBody, HttpMethod, HttpRequest, HttpTransport};

/// Kubo (or pinning-service-compatible) HTTP API.
pub struct IpfsClient<T> {
    provider: String,
    api_url: String,
    transport: T,
}

impl<T: HttpTransport> IpfsClient<T> {
    /// `api_url` is the Kubo API origin, e.g. `http://127.0.0.1:5001`.
    pub fn new(provider: impl Into<String>, api_url: impl Into<String>, transport: T) -> Self {
        Self {
            provider: provider.into(),
            api_url: api_url.into().trim_end_matches('/').to_string(),
            transport,
        }
    }

    fn add(&self, bytes: &[u8]) -> Result<String, StorageError> {
        let url = format!(
            "{}/api/v0/add?pin=true&cid-version=1&raw-leaves=true",
            self.api_url
        );
        let response = self.transport.execute(&HttpRequest {
            method: HttpMethod::Post,
            url,
            headers: Vec::new(),
            body: HttpBody::Multipart {
                field: "file",
                filename: "forge.car",
                bytes,
            },
        })?;
        let value: serde_json::Value = serde_json::from_slice(&response.body)?;
        value
            .get("Hash")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned)
            .ok_or_else(|| StorageError::Http("Kubo add response missing Hash".into()))
    }

    fn cat(&self, cid: &str) -> Result<Vec<u8>, StorageError> {
        let url = format!("{}/api/v0/cat?arg={cid}", self.api_url);
        let response = self.transport.execute(&HttpRequest {
            method: HttpMethod::Post,
            url,
            headers: Vec::new(),
            body: HttpBody::Empty,
        })?;
        Ok(response.body)
    }
}

impl<T: HttpTransport> StorageBackend for IpfsClient<T> {
    fn provider_id(&self) -> &str {
        &self.provider
    }

    fn locator_backend(&self) -> LocatorBackend {
        LocatorBackend::Ipfs
    }

    fn put(&self, bytes: &[u8]) -> Result<Locator, StorageError> {
        let id = self.add(bytes)?;
        Ok(Locator {
            backend: LocatorBackend::Ipfs,
            provider: self.provider.clone(),
            id,
        })
    }

    fn get(&self, locator_id: &str) -> Result<Vec<u8>, StorageError> {
        self.cat(locator_id)
    }

    fn exists(&self, locator_id: &str) -> Result<bool, StorageError> {
        match self.cat(locator_id) {
            Ok(_) => Ok(true),
            Err(StorageError::Http(msg)) if msg.contains("status 404") => Ok(false),
            Err(StorageError::NotFound(_)) => Ok(false),
            Err(err) => Err(err),
        }
    }

    fn delete(&self, locator_id: &str) -> Result<(), StorageError> {
        let url = format!("{}/api/v0/pin/rm?arg={locator_id}", self.api_url);
        match self.transport.execute(&HttpRequest {
            method: HttpMethod::Post,
            url,
            headers: Vec::new(),
            body: HttpBody::Empty,
        }) {
            Ok(_) | Err(StorageError::Http(_)) => Ok(()),
            Err(err) => Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{HttpResponse, MockTransport};
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[test]
    fn add_and_cat_roundtrip() {
        let store = Mutex::new(HashMap::<String, Vec<u8>>::new());
        let transport = MockTransport::new(move |req| {
            if req.url.contains("/api/v0/add") {
                let bytes = match req.body {
                    HttpBody::Multipart { bytes, .. } | HttpBody::Raw(bytes) => bytes.to_vec(),
                    HttpBody::Empty => Vec::new(),
                };
                store
                    .lock()
                    .expect("store")
                    .insert("bafkrei-test".into(), bytes);
                return Ok(HttpResponse {
                    status: 200,
                    body: br#"{"Hash":"bafkrei-test","Size":"1"}"#.to_vec(),
                });
            }
            if req.url.contains("/api/v0/cat") {
                let body = store
                    .lock()
                    .expect("store")
                    .get("bafkrei-test")
                    .cloned()
                    .unwrap_or_default();
                return Ok(HttpResponse { status: 200, body });
            }
            Err(StorageError::Http(format!("unexpected {}", req.url)))
        });
        let client = IpfsClient::new("ipfs-0", "http://127.0.0.1:5001", transport);
        let loc = client.put(b"hello-ipfs").unwrap();
        assert_eq!(loc.id, "bafkrei-test");
        assert_eq!(client.get(&loc.id).unwrap(), b"hello-ipfs");
    }
}
