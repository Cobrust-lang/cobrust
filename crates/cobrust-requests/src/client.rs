// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: requests 2.31.0
// oracle: cpython 3.11 (module: requests)
// functions translated: 13 (6 free verbs + Session::new + 6 Session methods)
// see PROVENANCE.toml for the full manifest.

//! Translated requests body — `Session`, `Response`, free verb
//! functions, `HttpError`. Per-function provenance lines follow.

#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

/// HTTP method enum — closed (constitution §2.2 forbids open enums).
/// Mirrors the six verbs `requests` exposes as top-level functions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
}

impl HttpMethod {
    fn as_reqwest(self) -> reqwest::Method {
        match self {
            HttpMethod::Get => reqwest::Method::GET,
            HttpMethod::Post => reqwest::Method::POST,
            HttpMethod::Put => reqwest::Method::PUT,
            HttpMethod::Patch => reqwest::Method::PATCH,
            HttpMethod::Delete => reqwest::Method::DELETE,
            HttpMethod::Head => reqwest::Method::HEAD,
        }
    }
}

/// Single error type for HTTP failures. Mirrors the union of
/// `requests.exceptions.{ConnectionError, Timeout, HTTPError, JSONDecodeError}`
/// from the Python form — they're collapsed into one Rust enum because
/// `Result<T, E>` is the default error path (constitution §2.2).
#[derive(Clone, Debug)]
pub struct HttpError {
    pub kind: HttpErrorKind,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpErrorKind {
    /// URL did not parse / scheme unsupported.
    InvalidUrl,
    /// Network-level failure (DNS, TCP, TLS).
    Network,
    /// Transport timed out.
    Timeout,
    /// Response body decoding failed (`Response::json` / `text`).
    DecodeBody,
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self.kind {
            HttpErrorKind::InvalidUrl => "invalid url",
            HttpErrorKind::Network => "network",
            HttpErrorKind::Timeout => "timeout",
            HttpErrorKind::DecodeBody => "decode body",
        };
        write!(f, "http {kind} error: {}", self.message)
    }
}

impl std::error::Error for HttpError {}

impl HttpError {
    pub(crate) fn invalid_url(message: impl Into<String>) -> Self {
        Self {
            kind: HttpErrorKind::InvalidUrl,
            message: message.into(),
        }
    }

    pub(crate) fn network(message: impl Into<String>) -> Self {
        Self {
            kind: HttpErrorKind::Network,
            message: message.into(),
        }
    }

    pub(crate) fn timeout(message: impl Into<String>) -> Self {
        Self {
            kind: HttpErrorKind::Timeout,
            message: message.into(),
        }
    }

    pub(crate) fn decode_body(message: impl Into<String>) -> Self {
        Self {
            kind: HttpErrorKind::DecodeBody,
            message: message.into(),
        }
    }

    /// Lift a `reqwest::Error` into our taxonomy.
    pub(crate) fn from_reqwest(err: &reqwest::Error) -> Self {
        if err.is_timeout() {
            return Self::timeout(err.to_string());
        }
        if err.is_request() || err.is_connect() {
            return Self::network(err.to_string());
        }
        if err.is_decode() {
            return Self::decode_body(err.to_string());
        }
        if err.is_builder() {
            return Self::invalid_url(err.to_string());
        }
        Self::network(err.to_string())
    }
}

/// HTTP response — constitution §5.1: ≤ 7 public fields per struct
/// (we expose 0 public fields; observers project the inner state).
///
/// Mirrors `requests.Response` minus the cookie-jar / auth surfaces
/// which are out of M-batch scope (M9+).
#[derive(Debug)]
pub struct Response {
    status: u16,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

impl Response {
    pub(crate) fn from_reqwest(mut resp: reqwest::blocking::Response) -> Result<Self, HttpError> {
        let status = resp.status().as_u16();
        let mut headers: HashMap<String, String> = HashMap::with_capacity(resp.headers().len());
        for (name, value) in resp.headers() {
            let key = name.as_str().to_owned();
            // Header values may be non-utf8; lift to lossy string as
            // `requests` does (it returns str via response.headers).
            let val = value.to_str().unwrap_or("").to_owned();
            headers.insert(key, val);
        }
        let mut body: Vec<u8> = Vec::new();
        if let Err(e) = std::io::Read::read_to_end(&mut resp, &mut body) {
            return Err(HttpError::network(format!("read body: {e}")));
        }
        Ok(Self {
            status,
            headers,
            body,
        })
    }

    /// Construct a `Response` directly from primitive parts (used by
    /// the in-process wiremock harness in `tests/`).
    pub fn from_parts(status: u16, headers: HashMap<String, String>, body: Vec<u8>) -> Self {
        Self {
            status,
            headers,
            body,
        }
    }

    /// HTTP status code, per `requests.Response.status_code`.
    pub fn status_code(&self) -> u16 {
        self.status
    }

    /// True for any 2xx status, per the truthy semantics of
    /// `requests.Response.ok` (boolean version).
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// Response headers as a borrowed map. Mirrors
    /// `requests.Response.headers` (case-insensitive in upstream;
    /// case-preserving here — header keys are returned by reqwest in
    /// lowercase canonical form).
    pub fn headers(&self) -> &HashMap<String, String> {
        &self.headers
    }

    /// Decoded body as utf-8. Mirrors `requests.Response.text`.
    ///
    /// # Errors
    /// Returns [`HttpError::decode_body`] for non-utf8 bodies.
    pub fn text(self) -> Result<String, HttpError> {
        String::from_utf8(self.body).map_err(|e| HttpError::decode_body(e.to_string()))
    }

    /// Parse the response body as JSON. Mirrors
    /// `requests.Response.json()`.
    ///
    /// # Errors
    /// Returns [`HttpError::decode_body`] for non-utf8 bodies or
    /// malformed JSON.
    pub fn json(self) -> Result<serde_json::Value, HttpError> {
        serde_json::from_slice::<serde_json::Value>(&self.body)
            .map_err(|e| HttpError::decode_body(e.to_string()))
    }

    /// Raw bytes of the response body (no decoding).
    pub fn bytes(self) -> Vec<u8> {
        self.body
    }
}

// fn:Session::new provider=synthetic model=requests-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate

/// Stateful client with a persistent connection pool. Mirrors
/// `requests.Session` — pool/keep-alive lives inside the `reqwest`
/// client, which is `Send + Sync`.
pub struct Session {
    inner: reqwest::blocking::Client,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    /// Create a new session with cobrust defaults: 30 s timeout,
    /// rustls TLS, no proxy auto-detection.
    pub fn new() -> Self {
        let inner = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("reqwest blocking client builder is infallible with cobrust defaults");
        Self { inner }
    }

    fn dispatch(
        &self,
        method: HttpMethod,
        url: &str,
        body: Option<&[u8]>,
    ) -> Result<Response, HttpError> {
        let parsed = reqwest::Url::parse(url).map_err(|e| HttpError::invalid_url(e.to_string()))?;
        let mut req = self.inner.request(method.as_reqwest(), parsed);
        if let Some(body_bytes) = body {
            req = req.body(body_bytes.to_owned());
        }
        let resp = req.send().map_err(|e| HttpError::from_reqwest(&e))?;
        Response::from_reqwest(resp)
    }

    // fn:Session::get provider=synthetic model=requests-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
    /// HTTP GET. Mirrors `Session.get(url)`.
    pub fn get(&self, url: &str) -> Result<Response, HttpError> {
        self.dispatch(HttpMethod::Get, url, None)
    }

    // fn:Session::post provider=synthetic model=requests-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
    /// HTTP POST. Mirrors `Session.post(url, data)`.
    pub fn post(&self, url: &str, body: &[u8]) -> Result<Response, HttpError> {
        self.dispatch(HttpMethod::Post, url, Some(body))
    }

    // fn:Session::put provider=synthetic model=requests-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
    /// HTTP PUT. Mirrors `Session.put(url, data)`.
    pub fn put(&self, url: &str, body: &[u8]) -> Result<Response, HttpError> {
        self.dispatch(HttpMethod::Put, url, Some(body))
    }

    // fn:Session::patch provider=synthetic model=requests-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
    /// HTTP PATCH. Mirrors `Session.patch(url, data)`.
    pub fn patch(&self, url: &str, body: &[u8]) -> Result<Response, HttpError> {
        self.dispatch(HttpMethod::Patch, url, Some(body))
    }

    // fn:Session::delete provider=synthetic model=requests-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
    /// HTTP DELETE. Mirrors `Session.delete(url)`.
    pub fn delete(&self, url: &str) -> Result<Response, HttpError> {
        self.dispatch(HttpMethod::Delete, url, None)
    }

    // fn:Session::head provider=synthetic model=requests-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
    /// HTTP HEAD. Mirrors `Session.head(url)`.
    pub fn head(&self, url: &str) -> Result<Response, HttpError> {
        self.dispatch(HttpMethod::Head, url, None)
    }
}

/// Process-wide default session, used by the free-function verb
/// shorthands. Lazily constructed; matches the upstream behaviour of
/// `requests.get(url)` (which lazily uses `requests.api.session`).
fn default_session() -> &'static Session {
    static DEFAULT: OnceLock<Session> = OnceLock::new();
    DEFAULT.get_or_init(Session::new)
}

// fn:get provider=synthetic model=requests-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
/// Stateless HTTP GET. Mirrors `requests.get(url)`.
pub fn get(url: &str) -> Result<Response, HttpError> {
    default_session().get(url)
}

// fn:post provider=synthetic model=requests-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
/// Stateless HTTP POST. Mirrors `requests.post(url, data)`.
pub fn post(url: &str, body: &[u8]) -> Result<Response, HttpError> {
    default_session().post(url, body)
}

// fn:put provider=synthetic model=requests-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
/// Stateless HTTP PUT. Mirrors `requests.put(url, data)`.
pub fn put(url: &str, body: &[u8]) -> Result<Response, HttpError> {
    default_session().put(url, body)
}

// fn:patch provider=synthetic model=requests-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
/// Stateless HTTP PATCH. Mirrors `requests.patch(url, data)`.
pub fn patch(url: &str, body: &[u8]) -> Result<Response, HttpError> {
    default_session().patch(url, body)
}

// fn:delete provider=synthetic model=requests-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
/// Stateless HTTP DELETE. Mirrors `requests.delete(url)`.
pub fn delete(url: &str) -> Result<Response, HttpError> {
    default_session().delete(url)
}

// fn:head provider=synthetic model=requests-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
/// Stateless HTTP HEAD. Mirrors `requests.head(url)`.
pub fn head(url: &str) -> Result<Response, HttpError> {
    default_session().head(url)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn http_method_round_trips_to_reqwest() {
        assert_eq!(HttpMethod::Get.as_reqwest(), reqwest::Method::GET);
        assert_eq!(HttpMethod::Post.as_reqwest(), reqwest::Method::POST);
        assert_eq!(HttpMethod::Put.as_reqwest(), reqwest::Method::PUT);
        assert_eq!(HttpMethod::Patch.as_reqwest(), reqwest::Method::PATCH);
        assert_eq!(HttpMethod::Delete.as_reqwest(), reqwest::Method::DELETE);
        assert_eq!(HttpMethod::Head.as_reqwest(), reqwest::Method::HEAD);
    }

    #[test]
    fn response_status_observers_project_inner_state() {
        let mut headers = HashMap::new();
        headers.insert("content-type".into(), "application/json".into());
        let resp = Response::from_parts(200, headers.clone(), b"{\"x\":1}".to_vec());
        assert_eq!(resp.status_code(), 200);
        assert!(resp.ok());
        assert_eq!(
            resp.headers().get("content-type").map(String::as_str),
            Some("application/json")
        );
    }

    #[test]
    fn response_ok_is_false_for_non_2xx() {
        for code in [199u16, 300, 404, 500, 0, 599] {
            let r = Response::from_parts(code, HashMap::new(), Vec::new());
            assert!(!r.ok(), "ok() must be false for {code}");
        }
        for code in [200u16, 201, 204, 299] {
            let r = Response::from_parts(code, HashMap::new(), Vec::new());
            assert!(r.ok(), "ok() must be true for {code}");
        }
    }

    #[test]
    fn response_json_decodes_valid_payload() {
        let r = Response::from_parts(200, HashMap::new(), b"{\"a\":1,\"b\":\"x\"}".to_vec());
        let v = r.json().expect("json decode");
        assert_eq!(v.get("a").and_then(|x| x.as_i64()), Some(1));
        assert_eq!(v.get("b").and_then(|x| x.as_str()), Some("x"));
    }

    #[test]
    fn response_json_rejects_malformed() {
        let r = Response::from_parts(200, HashMap::new(), b"not-json".to_vec());
        let err = r.json().expect_err("must fail");
        assert_eq!(err.kind, HttpErrorKind::DecodeBody);
    }

    #[test]
    fn response_text_decodes_utf8() {
        let r = Response::from_parts(200, HashMap::new(), "héllo".as_bytes().to_vec());
        assert_eq!(r.text().expect("utf8"), "héllo");
    }

    #[test]
    fn response_text_rejects_invalid_utf8() {
        // 0xC0 0x80 is overlong-NUL, invalid in standard utf-8.
        let r = Response::from_parts(200, HashMap::new(), vec![0xc0, 0x80]);
        let err = r.text().expect_err("invalid utf-8");
        assert_eq!(err.kind, HttpErrorKind::DecodeBody);
    }

    #[test]
    fn http_error_display_carries_kind() {
        let e = HttpError::network("dns lookup failed");
        let s = format!("{e}");
        assert!(s.contains("network"));
        assert!(s.contains("dns lookup"));
    }

    #[test]
    fn invalid_url_is_routed_through_invalid_url_kind() {
        let session = Session::new();
        let err = session
            .get("not a url")
            .expect_err("invalid scheme must error");
        assert_eq!(err.kind, HttpErrorKind::InvalidUrl);
    }

    #[test]
    fn invalid_scheme_is_invalid_url() {
        let session = Session::new();
        // reqwest::Url::parse rejects relative URLs without a base.
        let err = session
            .get("/relative/path")
            .expect_err("relative URL must error");
        assert_eq!(err.kind, HttpErrorKind::InvalidUrl);
    }
}
