//! Source-library → Cobrust crate-name mapping (ADR-0071 cobra-rebrand).
//!
//! The translator pipeline consumes a [`crate::pipeline::PyLibrary`]
//! keyed by its **source** Python library name (`tomli`, `numpy`,
//! `sqlite3`, …). ADR-0071 §3 renamed the *Cobrust-facing* identity of
//! the 7 in-flight translated crates to distinctive cobra-themed names
//! (`tomli → nest`, `numpy → coil`, …). The source-side provenance
//! (`SourceSection.library`, oracle module names, corpus paths) stays
//! the Python name; only the emitted Rust crate's `Cargo.toml` `name`
//! and the on-disk crate directory shift.
//!
//! This module owns the single source of truth for that mapping.
//! Sources not in the table fall through to the source name (preserving
//! translator behavior for not-yet-rebranded libs); the pipeline calls
//! [`source_to_cobra`] at every site that previously wrote
//! `format!("cobrust-{}", library.library)`.
//!
//! See ADR-0071 §3 for the cobra-name table and §4 "Consequences" for
//! the source-vs-Cobrust-identity convention this module enforces.

/// Map a source Python library name (`numpy`, `tomli`, …) to the bare
/// cobra word used in the Cobrust-facing crate name (`coil`, `nest`,
/// …). Sources not in the table fall through to the source name.
///
/// The full Cobrust crate name is constructed by callers as
/// `format!("cobrust-{}", source_to_cobra(library))`. The on-disk crate
/// directory mirrors that name (`crates/cobrust-coil/`).
///
/// ## Source-vs-Cobrust convention (ADR-0071 §3/§4)
///
/// - The **source-library identity** (`numpy`, `sqlite3`, …) stays
///   Python-named everywhere it documents *what was translated*:
///   `PROVENANCE.toml` `[source]` block, oracle module references,
///   `corpus/<source>/` paths, translation-header comments.
/// - The **Cobrust-facing identity** (`coil`, `den`, …) is the bare
///   cobra word used in the workspace crate name, the PyO3 module
///   name, and the `python/<cobra>_init.py` wrapper.
///
/// This function is the boundary: input is source-named, output is
/// Cobrust-named.
#[must_use]
pub fn source_to_cobra(source: &str) -> &str {
    match source {
        "numpy" => "coil",
        "sqlite3" => "den",
        "requests" => "strike",
        "msgpack" => "scale",
        "tomli" => "nest",
        "dateutil" => "molt",
        "click" => "hood",
        // Sources not yet rebranded fall through to the source name.
        // The Cobrust crate name remains `cobrust-<source>`; the
        // pipeline behavior is unchanged for these libs.
        other => other,
    }
}

/// Convenience: full Cobrust crate name (`cobrust-<cobra>`) derived
/// from a source library name. Mirrors the previous inline
/// `format!("cobrust-{}", library.library)` call shape.
#[must_use]
pub fn cobra_crate_name(source: &str) -> String {
    format!("cobrust-{}", source_to_cobra(source))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_all_seven_rebranded_libs() {
        // Pinned by ADR-0071 §3 naming table.
        assert_eq!(source_to_cobra("numpy"), "coil");
        assert_eq!(source_to_cobra("sqlite3"), "den");
        assert_eq!(source_to_cobra("requests"), "strike");
        assert_eq!(source_to_cobra("msgpack"), "scale");
        assert_eq!(source_to_cobra("tomli"), "nest");
        assert_eq!(source_to_cobra("dateutil"), "molt");
        assert_eq!(source_to_cobra("click"), "hood");
    }

    #[test]
    fn unknown_sources_fall_through() {
        // Sources not in the table preserve the translator's prior
        // behavior: the Cobrust crate name remains `cobrust-<source>`.
        assert_eq!(source_to_cobra("future_lib"), "future_lib");
        assert_eq!(source_to_cobra("toml"), "toml");
        assert_eq!(source_to_cobra(""), "");
    }

    #[test]
    fn cobra_crate_name_composes_full_workspace_name() {
        assert_eq!(cobra_crate_name("numpy"), "cobrust-coil");
        assert_eq!(cobra_crate_name("tomli"), "cobrust-nest");
        assert_eq!(cobra_crate_name("future_lib"), "cobrust-future_lib");
    }
}
