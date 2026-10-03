//! Minimal HTTP transport so IPFS/Arweave clients can be mocked in tests.

use crate::error::StorageError;

/// HTTP method used by pinning/bundler APIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    /// GET.
    Get,
    /// POST.
    Post,
}

/// Request body.
#[derive(Debug, Clone)]
pub enum HttpBody<'a> {
    /// Empty.
    Empty,
    /// Raw bytes.
    Raw(&'a [u8]),
    /// Multipart file field (Kubo `/api/v0/add`).
    Multipart {
        /// Form field name.
        field: &'static str,
        /// Filename sent to the server.
        filename: &'static str,
        /// File bytes.
        bytes: &'a [u8],
    },
}

/// Outgoing request.
#[derive(Debug, Clone)]
pub struct HttpRequest<'a> {
    /// GET or POST.
    pub method: HttpMethod,
    /// Absolute URL.
    pub url: String,
    /// Extra headers.
    pub headers: Vec<(String, String)>,
    /// Body.
    pub body: HttpBody<'a>,
}

/// HTTP response.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    /// Status code.
    pub status: u16,
    /// Response body.
    pub body: Vec<u8>,
}

/// Blocking HTTP client.
pub trait HttpTransport: Send + Sync {
    /// Perform one request.
    ///
    /// # Errors
    /// Returns [`StorageError::Http`] on transport failure.
    fn execute(&self, request: &HttpRequest<'_>) -> Result<HttpResponse, StorageError>;
}

/// `reqwest` blocking implementation.
#[derive(Debug, Clone)]
pub struct ReqwestTransport {
    client: reqwest::blocking::Client,
}

impl Default for ReqwestTransport {
    fn default() -> Self {
        Self {
            client: reqwest::blocking::Client::new(),
        }
    }
}

impl HttpTransport for ReqwestTransport {
    fn execute(&self, request: &HttpRequest<'_>) -> Result<HttpResponse, StorageError> {
        let mut builder = match request.method {
            HttpMethod::Get => self.client.get(&request.url),
            HttpMethod::Post => self.client.post(&request.url),
        };
        for (k, v) in &request.headers {
            builder = builder.header(k, v);
        }
        builder = match request.body {
            HttpBody::Empty => builder,
            HttpBody::Raw(bytes) => builder.body(bytes.to_vec()),
            HttpBody::Multipart {
                field,
                filename,
                bytes,
            } => {
                let part =
                    reqwest::blocking::multipart::Part::bytes(bytes.to_vec()).file_name(filename);
                let form = reqwest::blocking::multipart::Form::new().part(field, part);
                builder.multipart(form)
            }
        };
        let response = builder
            .send()
            .map_err(|err| StorageError::Http(err.to_string()))?;
        let status = response.status().as_u16();
        let body = response
            .bytes()
            .map_err(|err| StorageError::Http(err.to_string()))?
            .to_vec();
        if !(200..300).contains(&status) {
            return Err(StorageError::Http(format!(
                "status {status}: {}",
                String::from_utf8_lossy(&body)
            )));
        }
        Ok(HttpResponse { status, body })
    }
}

/// Scripted transport for tests.
pub struct MockTransport {
    handler: Box<dyn Fn(&HttpRequest<'_>) -> Result<HttpResponse, StorageError> + Send + Sync>,
}

impl MockTransport {
    /// Wraps a request handler.
    pub fn new<F>(handler: F) -> Self
    where
        F: Fn(&HttpRequest<'_>) -> Result<HttpResponse, StorageError> + Send + Sync + 'static,
    {
        Self {
            handler: Box::new(handler),
        }
    }
}

impl HttpTransport for MockTransport {
    fn execute(&self, request: &HttpRequest<'_>) -> Result<HttpResponse, StorageError> {
        (self.handler)(request)
    }
}
