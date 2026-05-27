// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: flask 3.0 (web-server surface)
// oracle: cpython 3.11 (module: flask)
// see PROVENANCE.toml for the full manifest.

//! The inbound [`Request`] handed to every handler.
//!
//! Mirrors the slice of Flask's `flask.request` that a handler reaches
//! for first: `request.method`, `request.path`, `request.view_args`
//! (path params), `request.args` (query string), `request.headers`,
//! `request.get_data()` / `request.data` (raw body), and
//! `request.get_json()`.

use std::collections::HashMap;

use crate::error::PitError;

/// An inbound HTTP request. Constructed by the server before the
/// matched handler runs; handlers receive it by value.
///
/// Constitution §5.1: ≤ 7 public fields. The body is owned bytes; the
/// accessor methods project the rest.
#[derive(Clone, Debug)]
pub struct Request {
    method: String,
    path: String,
    path_params: HashMap<String, String>,
    query: HashMap<String, String>,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

impl Request {
    /// Build a request from its primitive parts. Public so the in-process
    /// test harness (and future codegen-side wiring) can synthesize one
    /// without a live socket.
    #[must_use]
    pub fn from_parts(
        method: impl Into<String>,
        path: impl Into<String>,
        path_params: HashMap<String, String>,
        query: HashMap<String, String>,
        headers: HashMap<String, String>,
        body: Vec<u8>,
    ) -> Self {
        Self {
            method: method.into(),
            path: path.into(),
            path_params,
            query,
            headers,
            body,
        }
    }

    /// HTTP method, uppercased (`"GET"`, `"POST"`, …). Mirrors
    /// `request.method`.
    #[must_use]
    pub fn method(&self) -> &str {
        &self.method
    }

    /// Request path (no query string). Mirrors `request.path`.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// A single path parameter captured from a `<name>` route segment.
    /// Mirrors `request.view_args[name]` (returns `None` if absent).
    #[must_use]
    pub fn path_param(&self, name: &str) -> Option<&str> {
        self.path_params.get(name).map(String::as_str)
    }

    /// All captured path parameters. Mirrors `request.view_args`.
    #[must_use]
    pub fn path_params(&self) -> &HashMap<String, String> {
        &self.path_params
    }

    /// A single query-string parameter. Mirrors
    /// `request.args.get(name)`.
    #[must_use]
    pub fn query(&self, name: &str) -> Option<&str> {
        self.query.get(name).map(String::as_str)
    }

    /// All query-string parameters. Mirrors `request.args`.
    #[must_use]
    pub fn query_params(&self) -> &HashMap<String, String> {
        &self.query
    }

    /// A single request header by (case-insensitive) name. Mirrors
    /// `request.headers.get(name)`.
    ///
    /// Header keys are stored lowercased (HTTP header names are
    /// case-insensitive), so lookups are case-insensitive too.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }

    /// All request headers (keys lowercased). Mirrors
    /// `request.headers`.
    #[must_use]
    pub fn headers(&self) -> &HashMap<String, String> {
        &self.headers
    }

    /// Raw request body bytes. Mirrors `request.get_data()` /
    /// `request.data`.
    #[must_use]
    pub fn body(&self) -> &[u8] {
        &self.body
    }

    /// Request body decoded as UTF-8. Mirrors
    /// `request.get_data(as_text=True)`.
    ///
    /// # Errors
    /// Returns [`PitError`] (`Runtime` kind) for a non-UTF-8 body.
    pub fn text(&self) -> Result<String, PitError> {
        String::from_utf8(self.body.clone())
            .map_err(|e| PitError::runtime(format!("body is not utf-8: {e}")))
    }

    /// Parse the request body as JSON. Mirrors `request.get_json()`.
    ///
    /// # Errors
    /// Returns [`PitError`] (`Runtime` kind) for a malformed / non-JSON
    /// body.
    pub fn json(&self) -> Result<serde_json::Value, PitError> {
        serde_json::from_slice::<serde_json::Value>(&self.body)
            .map_err(|e| PitError::runtime(format!("body is not valid json: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> Request {
        let mut pp = HashMap::new();
        pp.insert("id".to_owned(), "42".to_owned());
        let mut q = HashMap::new();
        q.insert("verbose".to_owned(), "1".to_owned());
        let mut h = HashMap::new();
        h.insert("content-type".to_owned(), "application/json".to_owned());
        Request::from_parts("POST", "/users/42", pp, q, h, br#"{"name":"ada"}"#.to_vec())
    }

    #[test]
    fn accessors_project_inner_state() {
        let r = req();
        assert_eq!(r.method(), "POST");
        assert_eq!(r.path(), "/users/42");
        assert_eq!(r.path_param("id"), Some("42"));
        assert_eq!(r.path_param("missing"), None);
        assert_eq!(r.query("verbose"), Some("1"));
        // Header lookup is case-insensitive.
        assert_eq!(r.header("Content-Type"), Some("application/json"));
    }

    #[test]
    fn json_body_round_trips() {
        let r = req();
        let v = r.json().expect("json");
        assert_eq!(
            v.get("name").and_then(serde_json::Value::as_str),
            Some("ada")
        );
    }

    #[test]
    fn text_rejects_invalid_utf8() {
        let r = Request::from_parts(
            "GET",
            "/",
            HashMap::new(),
            HashMap::new(),
            HashMap::new(),
            vec![0xc0, 0x80],
        );
        assert!(r.text().is_err());
    }
}
