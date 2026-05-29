//! ADR-0080 Phase-1b-iii — OpenAPI schema EMISSION.
//!
//! This module is the OpenAPI EMITTER: it derives an OpenAPI 3.1 document
//! describing the `route_validated` routes a `.cb` app registered. The
//! load-bearing elegance property (ADR-0080 §2 Q4, §5.3, §3 footgun #4 —
//! "cannot drift"):
//!
//! > The OpenAPI schema is DERIVED FROM THE SAME SOURCE THE VALIDATOR
//! > READS — the compact schema descriptor + [`crate::validation::parse_schema`]
//! > (the parsed [`crate::validation::FieldSpec`] list the
//! > `route_validated` trampoline built from the MIR-injected
//! > schema-suffix). There is NO second, hand-written schema declaration.
//! > The int-range bound the validator enforces
//! > ([`crate::validation::FieldSpec`]'s `lo`/`hi`) IS the bound this
//! > emitter advertises (`minimum`/`maximum`) — they are two projections
//! > of ONE parse, so they provably cannot diverge.
//!
//! Concretely: [`field_schema`] turns ONE parsed `FieldSpec` into its
//! OpenAPI JSON; [`body_schema_object`] walks the WHOLE `parse_schema`
//! output for a body descriptor into a `{"type":"object","properties":…}`;
//! [`build_openapi_doc`] assembles the enclosing document (the `openapi`
//! version, `info`, `paths` for the registered validated routes, and
//! `components/schemas`).
//!
//! # Field → OpenAPI mapping (ADR-0080 §5.3)
//!
//! ```text
//! str                         → {"type":"string"}
//! i64                         → {"type":"integer"}
//! f64                         → {"type":"number"}
//! bool                        → {"type":"boolean"}
//! i64 where 0 <= self         → {"type":"integer","minimum":0}
//! i64 where self <= 100       → {"type":"integer","maximum":100}
//! i64 where 0 <= self <= 100  → {"type":"integer","minimum":0,"maximum":100}
//! ```
//!
//! (str length / pattern bounds are Phase-2/3, ADR-0080 §6 — not emitted
//! here.) The output is a `serde_json::Value` so the serving surface
//! ([`crate::app::App::serve_openapi`]) can render it with
//! `serde_json::to_string` — `openapi.json` is a Rust-assembled JSON
//! string response, NOT a `.cb`-struct serialization (that is the deferred
//! §9 `.cb`↔serde bridge).

use serde_json::{Map, Value, json};

use crate::validation::{FieldSpec, body_name, parse_schema};

/// The OpenAPI version this emitter targets (ADR-0080 §5.3 — OpenAPI 3.1).
const OPENAPI_VERSION: &str = "3.1.0";

/// Metadata for one `route_validated` registration the app accumulated
/// (ADR-0080 §5.4 / Phase-1b-iii): the HTTP `method`, the route `path`,
/// and the compact body-schema descriptor the trampoline received (the
/// SAME string the validator parses — footgun #4). The OpenAPI doc lists
/// these.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedRouteMeta {
    /// The HTTP method, uppercased (`POST`, `PUT`, …).
    pub method: String,
    /// The route path (e.g. `/scores`).
    pub path: String,
    /// The compact body-schema descriptor (the `route_validated` `schema`
    /// arg — the validator's source; see [`crate::validation`]).
    pub schema: String,
}

/// The body class's schema name for `components/schemas/<name>`. Reads the
/// `# <BodyName>` header line of the descriptor (the SAME string the
/// validator reads); falls back to `"RequestBody"` when the descriptor
/// carries no name (a defensive empty-schema case — the type checker has
/// already accepted the program).
fn schema_name(meta: &ValidatedRouteMeta) -> String {
    body_name(&meta.schema).unwrap_or_else(|| "RequestBody".to_string())
}

/// Derive ONE field's OpenAPI 3.1 schema object from its parsed
/// [`FieldSpec`] (ADR-0080 §5.3). The `type` keyword comes from the field
/// kind; an int-range refinement contributes `minimum` (from `lo`) and/or
/// `maximum` (from `hi`) — the EXACT SAME `lo`/`hi` the validator
/// range-checks against, so the advertised bound cannot drift from the
/// enforced one (footgun #4).
///
/// An `Any` field (a non-Phase-1b-ii scalar) has no statically-known type,
/// so the emitter yields an empty schema `{}` (OpenAPI's "any value") —
/// honestly advertising "this field is present but unconstrained" rather
/// than guessing a type.
///
/// `pub(crate)` — it takes a crate-internal [`FieldSpec`] (the validator's
/// parsed representation); the public surface is [`body_schema_object`] /
/// [`build_openapi_doc`] / [`openapi_json`], which take `&str` / `&[…]`.
#[must_use]
pub(crate) fn field_schema(spec: &FieldSpec) -> Value {
    let mut obj = Map::new();
    if let Some(ty) = spec.kind.openapi_type() {
        obj.insert("type".to_string(), Value::String(ty.to_string()));
    }
    // The int-range refinement (`minimum`/`maximum`) — the SAME `lo`/`hi`
    // the validator enforces (ADR-0080 §5.3, cannot drift). Only an i64
    // field carries these; `parse_schema` leaves them `None` otherwise.
    if let Some(lo) = spec.lo {
        obj.insert("minimum".to_string(), Value::Number(lo.into()));
    }
    if let Some(hi) = spec.hi {
        obj.insert("maximum".to_string(), Value::Number(hi.into()));
    }
    Value::Object(obj)
}

/// Walk the WHOLE body-schema descriptor (via [`parse_schema`] — the SAME
/// parse the validator runs) into an OpenAPI `object` schema:
/// `{"type":"object","properties":{<field>:<field_schema>,…}}` (ADR-0080
/// §5.3). The properties preserve `parse_schema`'s deterministic field
/// order. The header line (`# <BodyName>`) is skipped by `parse_schema`
/// for free (no TAB).
#[must_use]
pub fn body_schema_object(schema: &str) -> Value {
    let specs = parse_schema(schema);
    let mut properties = Map::new();
    for spec in &specs {
        properties.insert(spec.name.clone(), field_schema(spec));
    }
    json!({
        "type": "object",
        "properties": Value::Object(properties),
    })
}

/// Assemble the enclosing OpenAPI 3.1 document for the registered
/// validated routes (ADR-0080 §5.3). Builds:
///
/// - the `openapi` version marker (3.1.0);
/// - an `info` block (title + version);
/// - `paths` — one entry per route, each with its method's
///   `requestBody` `$ref`-ing the body component;
/// - `components/schemas/<BodyName>` — the body schema object derived by
///   walking each route's descriptor through [`body_schema_object`] (the
///   cannot-drift single source).
///
/// Every projection here reads only the accumulated [`ValidatedRouteMeta`]
/// (method/path/descriptor) — the SAME descriptor strings the validator
/// uses — so the doc and the runtime validation cannot diverge.
#[must_use]
pub fn build_openapi_doc(routes: &[ValidatedRouteMeta]) -> Value {
    let mut paths = Map::new();
    let mut schemas = Map::new();

    for meta in routes {
        let name = schema_name(meta);
        // The body component, derived from the descriptor (footgun #4).
        schemas.insert(name.clone(), body_schema_object(&meta.schema));

        // The path-item: `{ <method-lowercase>: { requestBody: {...},
        // responses: {...} } }`. Multiple methods on one path merge into
        // the same path-item object.
        let method_key = meta.method.to_ascii_lowercase();
        let operation = json!({
            "requestBody": {
                "required": true,
                "content": {
                    "application/json": {
                        "schema": { "$ref": format!("#/components/schemas/{name}") }
                    }
                }
            },
            "responses": {
                "200": { "description": "validated" },
                "422": { "description": "request body failed validation" }
            }
        });
        let entry = paths
            .entry(meta.path.clone())
            .or_insert_with(|| Value::Object(Map::new()));
        if let Value::Object(item) = entry {
            item.insert(method_key, operation);
        }
    }

    json!({
        "openapi": OPENAPI_VERSION,
        "info": {
            "title": "Cobrust pit API",
            "version": "0.1.0"
        },
        "paths": Value::Object(paths),
        "components": {
            "schemas": Value::Object(schemas)
        }
    })
}

/// Assemble the OpenAPI doc and render it to a JSON string (the body of
/// the `GET /openapi.json` response — ADR-0080 §5.3). `openapi.json` is a
/// Rust-assembled JSON string, not a `.cb`-struct serialization (the
/// deferred §9 bridge).
#[must_use]
pub fn openapi_json(routes: &[ValidatedRouteMeta]) -> String {
    let doc = build_openapi_doc(routes);
    serde_json::to_string(&doc).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The §6 Phase-1 body descriptor: `name: str`, `rank: i64 where
    /// 0 <= self <= 100`, named `CreateScore` via the header line.
    const SCHEMA: &str = "# CreateScore\nname\tstr\nrank\ti64:0:100";

    #[test]
    fn name_field_schema_is_plain_string() {
        // ADR-0080 §5.3 — a plain `str` field → {type:string}, no bounds.
        let specs = parse_schema(SCHEMA);
        let name = specs.iter().find(|s| s.name == "name").expect("name field");
        let v = field_schema(name);
        assert_eq!(v["type"], "string");
        assert!(v.get("minimum").is_none(), "plain str has no minimum");
        assert!(v.get("maximum").is_none(), "plain str has no maximum");
    }

    #[test]
    fn rank_field_schema_carries_int_range_bounds() {
        // ADR-0080 §5.3 — rank (i64 where 0<=self<=100) → {type:integer,
        // minimum:0, maximum:100}. The SAME lo/hi the validator enforces.
        let specs = parse_schema(SCHEMA);
        let rank = specs.iter().find(|s| s.name == "rank").expect("rank field");
        let v = field_schema(rank);
        assert_eq!(v["type"], "integer");
        assert_eq!(v["minimum"], 0);
        assert_eq!(v["maximum"], 100);
    }

    #[test]
    fn one_sided_lower_bound_emits_only_minimum() {
        // `0 <= self` (no upper bound) → minimum:0, NO maximum.
        let specs = parse_schema("n\ti64:0:");
        let n = &specs[0];
        let v = field_schema(n);
        assert_eq!(v["minimum"], 0);
        assert!(v.get("maximum").is_none());
    }

    #[test]
    fn body_schema_object_shape() {
        let v = body_schema_object(SCHEMA);
        assert_eq!(v["type"], "object");
        assert_eq!(v["properties"]["name"]["type"], "string");
        assert_eq!(v["properties"]["rank"]["type"], "integer");
        assert_eq!(v["properties"]["rank"]["maximum"], 100);
    }

    #[test]
    fn full_doc_has_version_components_and_paths() {
        let routes = vec![ValidatedRouteMeta {
            method: "POST".to_string(),
            path: "/scores".to_string(),
            schema: SCHEMA.to_string(),
        }];
        let doc = build_openapi_doc(&routes);
        // OpenAPI version marker.
        assert_eq!(doc["openapi"], OPENAPI_VERSION);
        // info block.
        assert!(doc["info"]["title"].is_string());
        // The body schema component, keyed by the descriptor's `# Name`.
        let schema = &doc["components"]["schemas"]["CreateScore"];
        assert_eq!(schema["properties"]["rank"]["minimum"], 0);
        assert_eq!(schema["properties"]["rank"]["maximum"], 100);
        // The path entry references the component.
        let op = &doc["paths"]["/scores"]["post"];
        assert_eq!(
            op["requestBody"]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/CreateScore"
        );
    }

    #[test]
    fn cannot_drift_advertised_bound_equals_parsed_bound() {
        // The CANNOT-DRIFT property in isolation: the bound the OpenAPI doc
        // advertises (`maximum`) is read from the SAME parse_schema output
        // a validator would range-check. They come from one source, so the
        // advertised `maximum` equals the parsed `hi`.
        let specs = parse_schema(SCHEMA);
        let rank = specs.iter().find(|s| s.name == "rank").expect("rank");
        let parsed_hi = rank.hi.expect("rank has an upper bound");
        let doc = build_openapi_doc(&[ValidatedRouteMeta {
            method: "POST".to_string(),
            path: "/scores".to_string(),
            schema: SCHEMA.to_string(),
        }]);
        let advertised =
            doc["components"]["schemas"]["CreateScore"]["properties"]["rank"]["maximum"]
                .as_i64()
                .expect("maximum present");
        assert_eq!(
            advertised, parsed_hi,
            "the advertised maximum must equal the parsed bound — one source"
        );
        assert_eq!(advertised, 100);
    }

    #[test]
    fn json_render_is_valid_json() {
        let routes = vec![ValidatedRouteMeta {
            method: "POST".to_string(),
            path: "/scores".to_string(),
            schema: SCHEMA.to_string(),
        }];
        let s = openapi_json(&routes);
        let parsed: Value = serde_json::from_str(&s).expect("openapi_json is valid JSON");
        assert_eq!(parsed["openapi"], OPENAPI_VERSION);
    }

    #[test]
    fn missing_body_name_falls_back() {
        // A descriptor with no `# Name` header line → fallback component
        // name. The fields still render correctly.
        let routes = vec![ValidatedRouteMeta {
            method: "POST".to_string(),
            path: "/x".to_string(),
            schema: "rank\ti64:0:100".to_string(),
        }];
        let doc = build_openapi_doc(&routes);
        let schema = &doc["components"]["schemas"]["RequestBody"];
        assert_eq!(schema["properties"]["rank"]["maximum"], 100);
    }
}
