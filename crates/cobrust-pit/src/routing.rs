// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: flask 3.0 (web-server surface)
// oracle: cpython 3.11 (module: flask)
// see PROVENANCE.toml for the full manifest.

//! Route table + path matching.
//!
//! Mirrors Flask's `@app.route("/users/<id>")` rule syntax for the
//! common case: literal segments and `<name>` capture segments. The
//! match is segment-by-segment (no converters / regex / optional
//! trailing-slash redirect in this increment — see Non-goals in
//! `docs/agent/modules/pit.md`).

use std::collections::HashMap;

use crate::error::PitError;

/// A compiled route pattern: a sequence of literal-or-capture segments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Segment {
    /// A literal path segment that must match exactly.
    Literal(String),
    /// A `<name>` capture segment that binds any single segment to
    /// `name`.
    Param(String),
}

/// A compiled route: its segments + the original pattern (for error
/// messages and duplicate detection).
#[derive(Clone, Debug)]
pub(crate) struct RoutePattern {
    segments: Vec<Segment>,
    raw: String,
}

impl RoutePattern {
    /// Compile a Flask-style rule string (`"/users/<id>"`) into a
    /// [`RoutePattern`].
    ///
    /// # Errors
    /// Returns [`PitError`] (`InvalidRoute` kind) for an empty path, a
    /// path not starting with `/`, or an unbalanced `<...>` segment.
    pub(crate) fn compile(raw: &str) -> Result<Self, PitError> {
        if raw.is_empty() || !raw.starts_with('/') {
            return Err(PitError::invalid_route(format!(
                "route must be a non-empty path starting with '/': {raw:?}"
            )));
        }
        let mut segments = Vec::new();
        // Split on '/', skipping the leading empty segment. A trailing
        // slash yields a trailing empty literal, which we keep so that
        // "/a/" and "/a" are distinct (matching Werkzeug's strict slash
        // for the literal case in this increment).
        for seg in raw.split('/').skip(1) {
            if let Some(inner) = seg.strip_prefix('<') {
                let name = inner.strip_suffix('>').ok_or_else(|| {
                    PitError::invalid_route(format!("unterminated '<...>' in route: {raw:?}"))
                })?;
                if name.is_empty() || name.contains('<') || name.contains('>') {
                    return Err(PitError::invalid_route(format!(
                        "malformed capture segment in route: {raw:?}"
                    )));
                }
                segments.push(Segment::Param(name.to_owned()));
            } else if seg.contains('<') || seg.contains('>') {
                return Err(PitError::invalid_route(format!(
                    "stray '<' or '>' in literal segment of route: {raw:?}"
                )));
            } else {
                segments.push(Segment::Literal(seg.to_owned()));
            }
        }
        Ok(Self {
            segments,
            raw: raw.to_owned(),
        })
    }

    /// The original rule string.
    pub(crate) fn raw(&self) -> &str {
        &self.raw
    }

    /// Attempt to match a concrete request path against this pattern.
    /// On success, returns the captured path parameters.
    pub(crate) fn match_path(&self, path: &str) -> Option<HashMap<String, String>> {
        let parts: Vec<&str> = path.split('/').skip(1).collect();
        if parts.len() != self.segments.len() {
            return None;
        }
        let mut params = HashMap::new();
        for (seg, part) in self.segments.iter().zip(parts.iter()) {
            match seg {
                Segment::Literal(lit) => {
                    if lit != part {
                        return None;
                    }
                }
                Segment::Param(name) => {
                    // A capture cannot match an empty segment (e.g. the
                    // tail of a trailing slash), matching Flask's
                    // requirement that `<id>` bind a non-empty value.
                    if part.is_empty() {
                        return None;
                    }
                    params.insert(name.clone(), (*part).to_owned());
                }
            }
        }
        Some(params)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_literal_and_capture_segments() {
        let r = RoutePattern::compile("/users/<id>/posts").expect("compile");
        assert_eq!(
            r.segments,
            vec![
                Segment::Literal("users".to_owned()),
                Segment::Param("id".to_owned()),
                Segment::Literal("posts".to_owned()),
            ]
        );
    }

    #[test]
    fn rejects_bad_routes() {
        assert!(RoutePattern::compile("").is_err());
        assert!(RoutePattern::compile("no-leading-slash").is_err());
        assert!(RoutePattern::compile("/users/<id").is_err());
        assert!(RoutePattern::compile("/users/<>").is_err());
        assert!(RoutePattern::compile("/a<b/c").is_err());
    }

    #[test]
    fn matches_and_captures() {
        let r = RoutePattern::compile("/users/<id>").expect("compile");
        let caps = r.match_path("/users/42").expect("match");
        assert_eq!(caps.get("id").map(String::as_str), Some("42"));
        // Wrong segment count does not match.
        assert!(r.match_path("/users/42/extra").is_none());
        assert!(r.match_path("/users").is_none());
        // Empty capture does not match.
        assert!(r.match_path("/users/").is_none());
    }

    #[test]
    fn literal_root_matches_only_root() {
        let r = RoutePattern::compile("/").expect("compile");
        assert!(r.match_path("/").is_some());
        assert!(r.match_path("/x").is_none());
    }
}
