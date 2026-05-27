// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: flask 3.0 (web-server surface)
// oracle: cpython 3.11 (module: flask)
// see PROVENANCE.toml for the full manifest.

//! The outbound [`Response`] a handler returns.
//!
//! Mirrors the constructors a Flask handler reaches for: a bare string
//! (text response), `jsonify(value)` (JSON response), and an explicit
//! `(body, status)` / `Response(body, status, headers)`.

use std::collections::HashMap;

/// An outbound HTTP response. A handler returns one of these; the
/// server serializes it onto the wire.
///
/// Constitution §5.1: 0 public fields — observers project the state.
/// Mirrors `flask.Response` minus the streaming / cookie surfaces.
#[derive(Clone, Debug)]
pub struct Response {
    status: u16,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

impl Response {
    /// A `200 OK` plain-text response. Mirrors a Flask handler that
    /// `return`s a bare `str` (default `Content-Type: text/html;
    /// charset=utf-8`, matching Flask's default for string returns).
    #[must_use]
    pub fn text(body: impl Into<String>) -> Self {
        let mut headers = HashMap::new();
        headers.insert(
            "content-type".to_owned(),
            "text/html; charset=utf-8".to_owned(),
        );
        Self {
            status: 200,
            headers,
            body: body.into().into_bytes(),
        }
    }

    /// A `200 OK` JSON response. Mirrors `flask.jsonify(value)` /
    /// returning a dict (Flask 1.1+ auto-jsonifies dict returns).
    /// Sets `Content-Type: application/json`.
    #[must_use]
    pub fn json(value: &serde_json::Value) -> Self {
        let body = serde_json::to_vec(value).unwrap_or_else(|_| b"null".to_vec());
        let mut headers = HashMap::new();
        headers.insert("content-type".to_owned(), "application/json".to_owned());
        Self {
            status: 200,
            headers,
            body,
        }
    }

    /// Build a response from primitive parts. Mirrors the explicit
    /// `Response(body, status, headers)` constructor.
    #[must_use]
    pub fn from_parts(status: u16, headers: HashMap<String, String>, body: Vec<u8>) -> Self {
        Self {
            status,
            headers,
            body,
        }
    }

    /// Override the status code, builder-style. Mirrors a Flask handler
    /// returning `(body, status)`.
    #[must_use]
    pub fn with_status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }

    /// Add / override a header, builder-style. Mirrors
    /// `response.headers[name] = value`.
    #[must_use]
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers
            .insert(name.into().to_ascii_lowercase(), value.into());
        self
    }

    /// HTTP status code. Mirrors `response.status_code`.
    #[must_use]
    pub fn status_code(&self) -> u16 {
        self.status
    }

    /// Response headers (keys lowercased). Mirrors `response.headers`.
    #[must_use]
    pub fn headers(&self) -> &HashMap<String, String> {
        &self.headers
    }

    /// Raw response body bytes. Mirrors `response.get_data()`.
    #[must_use]
    pub fn body(&self) -> &[u8] {
        &self.body
    }

    /// Consume the response, yielding its primitive parts. Used by the
    /// server to write the response onto the wire.
    #[must_use]
    pub fn into_parts(self) -> (u16, HashMap<String, String>, Vec<u8>) {
        (self.status, self.headers, self.body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_defaults_to_200_and_html_content_type() {
        let r = Response::text("hello");
        assert_eq!(r.status_code(), 200);
        assert_eq!(r.body(), b"hello");
        assert_eq!(
            r.headers().get("content-type").map(String::as_str),
            Some("text/html; charset=utf-8")
        );
    }

    #[test]
    fn json_sets_content_type_and_serializes() {
        let v = serde_json::json!({"ok": true, "n": 7});
        let r = Response::json(&v);
        assert_eq!(r.status_code(), 200);
        assert_eq!(
            r.headers().get("content-type").map(String::as_str),
            Some("application/json")
        );
        let parsed: serde_json::Value = serde_json::from_slice(r.body()).expect("json");
        assert_eq!(parsed.get("n").and_then(serde_json::Value::as_i64), Some(7));
    }

    #[test]
    fn builders_override_status_and_header() {
        let r = Response::text("nope")
            .with_status(404)
            .with_header("X-Reason", "missing");
        assert_eq!(r.status_code(), 404);
        // Header name is lowercased on insert.
        assert_eq!(
            r.headers().get("x-reason").map(String::as_str),
            Some("missing")
        );
    }
}
