//! User-crate `cobrust.toml` parser + validator (ADR-0026 §"Manifest schema").

use std::collections::BTreeMap;
use std::path::PathBuf;

use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};

use crate::error::{ManifestError, PkgError};

/// Top-level user-crate manifest.
///
/// Schema (binding — ADR-0026 §"Manifest schema (binding)"):
///
/// ```toml
/// [package]
/// name = "my_app"
/// version = "0.1.0"
/// cobrust-version = "0.0.1"
/// authors = ["..."]
/// license = "Apache-2.0 OR MIT"
/// description = "..."
///
/// [dependencies]
/// cobrust-tomli = { path = "../cobrust-tomli" }
/// my_lib       = { git = "https://...", rev = "abc123" }
/// serde-like   = "1.2"
///
/// [dev-dependencies]
/// test_helpers = { path = "./test_helpers" }
///
/// [bin]
/// name = "my_app"
/// path = "src/main.cb"
///
/// [lib]
/// name = "my_app_lib"
/// path = "src/lib.cb"
///
/// [[test]]
/// name = "smoke"
/// path = "tests/smoke.cb"
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct Manifest {
    pub package: PackageTable,
    /// Sorted by key for deterministic round-trip.
    pub dependencies: BTreeMap<String, Dependency>,
    pub dev_dependencies: BTreeMap<String, Dependency>,
    pub bin: Option<BinTable>,
    pub lib: Option<LibTable>,
    pub tests: Vec<TestTable>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PackageTable {
    pub name: String,
    pub version: String,
    #[serde(rename = "cobrust-version")]
    pub cobrust_version: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Dependency {
    /// Original key (the user's `[dependencies.<name>]` row name).
    pub name: String,
    pub spec: DependencySpec,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DependencySpec {
    /// Bare-string semver shorthand: `dep = "1.2"`.
    /// At M12 this resolves to a registry source which returns Offline.
    Version { req: VersionReq },
    /// Path source: `{ path = "../foo" }`.
    Path { path: PathBuf },
    /// Git source: `{ git = "...", rev = "..." }`.
    Git { url: String, rev: String },
    /// Explicit registry: `{ version = "...", registry = "default" }`.
    Registry { req: VersionReq, registry: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BinTable {
    pub name: String,
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LibTable {
    pub name: String,
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TestTable {
    pub name: String,
    pub path: String,
}

// ----- Raw TOML view (for the parse pass) ------------------------------------

#[derive(Deserialize)]
struct RawManifest {
    package: Option<PackageTable>,
    router: Option<toml::Value>,
    #[serde(default)]
    dependencies: BTreeMap<String, RawDependency>,
    #[serde(default, rename = "dev-dependencies")]
    dev_dependencies: BTreeMap<String, RawDependency>,
    bin: Option<BinTable>,
    lib: Option<LibTable>,
    #[serde(default, rename = "test")]
    test: Vec<TestTable>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawDependency {
    Bare(String),
    Table {
        version: Option<String>,
        path: Option<String>,
        git: Option<String>,
        rev: Option<String>,
        registry: Option<String>,
    },
}

// ----- Public API ------------------------------------------------------------

impl Manifest {
    /// Parse + validate a `cobrust.toml` string.
    pub fn parse_str(contents: &str) -> Result<Self, PkgError> {
        let raw: RawManifest =
            toml::from_str(contents).map_err(|e| ManifestError::TomlParse(format!("{e}")))?;

        // Reject if this looks like an LLM-router config (M3 ADR-0004).
        // Per ADR-0026 §B (Option C namespace collision).
        if raw.package.is_none() && raw.router.is_some() {
            return Err(PkgError::Manifest(ManifestError::IsRouterConfig));
        }

        let package = raw
            .package
            .ok_or_else(|| ManifestError::MissingField("package".into()))?;

        validate_name(&package.name)?;
        validate_version(&package.name, &package.version)?;
        validate_version(&package.name, &package.cobrust_version)?;

        let mut dependencies = BTreeMap::new();
        for (k, v) in raw.dependencies {
            let dep = lower_dependency(&k, v)?;
            dependencies.insert(k, dep);
        }
        let mut dev_dependencies = BTreeMap::new();
        for (k, v) in raw.dev_dependencies {
            let dep = lower_dependency(&k, v)?;
            dev_dependencies.insert(k, dep);
        }

        if raw.bin.is_none() && raw.lib.is_none() {
            return Err(PkgError::Manifest(ManifestError::NoTarget));
        }
        if let (Some(b), Some(l)) = (&raw.bin, &raw.lib) {
            if b.path == l.path {
                return Err(PkgError::Manifest(ManifestError::ConflictingPaths {
                    path: b.path.clone(),
                }));
            }
        }

        Ok(Manifest {
            package,
            dependencies,
            dev_dependencies,
            bin: raw.bin,
            lib: raw.lib,
            tests: raw.test,
        })
    }

    /// Serialize the manifest to a canonical TOML string. Field order is
    /// deterministic (per ADR-0026 §"Lockfile schema (binding)" determinism
    /// rules — same canonicalization applies to manifest hashing).
    ///
    /// This is the input to `manifest_hash`.
    pub fn canonical_toml(&self) -> String {
        let mut out = String::new();
        out.push_str("[package]\n");
        out.push_str(&format!("name = \"{}\"\n", self.package.name));
        out.push_str(&format!("version = \"{}\"\n", self.package.version));
        out.push_str(&format!(
            "cobrust-version = \"{}\"\n",
            self.package.cobrust_version
        ));
        if !self.package.authors.is_empty() {
            out.push_str("authors = [");
            let mut first = true;
            for a in &self.package.authors {
                if !first {
                    out.push_str(", ");
                }
                first = false;
                out.push_str(&format!(
                    "\"{}\"",
                    a.replace('\\', "\\\\").replace('"', "\\\"")
                ));
            }
            out.push_str("]\n");
        }
        if let Some(license) = &self.package.license {
            out.push_str(&format!("license = \"{license}\"\n"));
        }
        if let Some(desc) = &self.package.description {
            out.push_str(&format!(
                "description = \"{}\"\n",
                desc.replace('\\', "\\\\").replace('"', "\\\"")
            ));
        }

        if !self.dependencies.is_empty() {
            out.push_str("\n[dependencies]\n");
            for (k, v) in &self.dependencies {
                out.push_str(&format!("{k} = {}\n", dep_to_canonical(&v.spec)));
            }
        }
        if !self.dev_dependencies.is_empty() {
            out.push_str("\n[dev-dependencies]\n");
            for (k, v) in &self.dev_dependencies {
                out.push_str(&format!("{k} = {}\n", dep_to_canonical(&v.spec)));
            }
        }
        if let Some(b) = &self.bin {
            out.push_str(&format!(
                "\n[bin]\nname = \"{}\"\npath = \"{}\"\n",
                b.name, b.path
            ));
        }
        if let Some(l) = &self.lib {
            out.push_str(&format!(
                "\n[lib]\nname = \"{}\"\npath = \"{}\"\n",
                l.name, l.path
            ));
        }
        for t in &self.tests {
            out.push_str(&format!(
                "\n[[test]]\nname = \"{}\"\npath = \"{}\"\n",
                t.name, t.path
            ));
        }

        out
    }

    /// Compute the manifest's blake3 hash from its canonical TOML form.
    pub fn manifest_hash(&self) -> String {
        let canonical = self.canonical_toml();
        let h = blake3::hash(canonical.as_bytes());
        format!("blake3:{}", h.to_hex())
    }

    /// Return true if the contents look like a user-crate manifest
    /// (has `[package]` table and no `[router]` table). Used by
    /// `find_manifest` to distinguish from M3 router configs.
    pub fn looks_like_user_crate(contents: &str) -> bool {
        // Cheap check: look for `[package]` table.
        let has_package = contents
            .lines()
            .any(|l| l.trim_start().starts_with("[package]"));
        let has_router = contents
            .lines()
            .any(|l| l.trim_start().starts_with("[router]"));
        has_package && !has_router
    }
}

fn dep_to_canonical(spec: &DependencySpec) -> String {
    match spec {
        DependencySpec::Version { req } => format!("\"{req}\""),
        DependencySpec::Path { path } => {
            format!("{{ path = \"{}\" }}", path.display())
        }
        DependencySpec::Git { url, rev } => {
            format!("{{ git = \"{url}\", rev = \"{rev}\" }}")
        }
        DependencySpec::Registry { req, registry } => {
            format!("{{ version = \"{req}\", registry = \"{registry}\" }}")
        }
    }
}

fn lower_dependency(name: &str, raw: RawDependency) -> Result<Dependency, PkgError> {
    validate_name(name)?;
    let spec = match raw {
        RawDependency::Bare(s) => {
            let req = VersionReq::parse(&s).map_err(|e| ManifestError::InvalidDependency {
                name: name.to_string(),
                reason: format!("invalid semver req `{s}`: {e}"),
            })?;
            DependencySpec::Version { req }
        }
        RawDependency::Table {
            version,
            path,
            git,
            rev,
            registry,
        } => {
            // Mutual exclusion: exactly one of {path, git, version+registry, version}.
            let n_set = [version.is_some(), path.is_some(), git.is_some()]
                .iter()
                .filter(|b| **b)
                .count();
            if n_set == 0 {
                return Err(PkgError::Manifest(ManifestError::InvalidDependency {
                    name: name.to_string(),
                    reason: "must declare one of: path, git, version".into(),
                }));
            }
            if let Some(p) = path {
                if version.is_some() || git.is_some() || registry.is_some() {
                    return Err(PkgError::Manifest(ManifestError::InvalidDependency {
                        name: name.to_string(),
                        reason: "path source cannot be combined with version/git/registry".into(),
                    }));
                }
                DependencySpec::Path {
                    path: PathBuf::from(p),
                }
            } else if let Some(g) = git {
                let r = rev.ok_or_else(|| ManifestError::InvalidDependency {
                    name: name.to_string(),
                    reason: "git source requires `rev = \"...\"`".into(),
                })?;
                DependencySpec::Git { url: g, rev: r }
            } else {
                let v = version.expect("checked: at least one of version/path/git is set");
                let req = VersionReq::parse(&v).map_err(|e| ManifestError::InvalidDependency {
                    name: name.to_string(),
                    reason: format!("invalid semver req `{v}`: {e}"),
                })?;
                let r = registry.unwrap_or_else(|| "default".to_string());
                DependencySpec::Registry { req, registry: r }
            }
        }
    };
    Ok(Dependency {
        name: name.to_string(),
        spec,
    })
}

fn validate_name(name: &str) -> Result<(), PkgError> {
    if name.is_empty() || name.len() > 64 {
        return Err(PkgError::Manifest(ManifestError::InvalidName(name.into())));
    }
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return Err(PkgError::Manifest(ManifestError::InvalidName(name.into())));
    };
    if !first.is_ascii_alphabetic() {
        return Err(PkgError::Manifest(ManifestError::InvalidName(name.into())));
    }
    for c in chars {
        if !(c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            return Err(PkgError::Manifest(ManifestError::InvalidName(name.into())));
        }
    }
    Ok(())
}

fn validate_version(name: &str, version: &str) -> Result<(), PkgError> {
    Version::parse(version).map_err(|_| ManifestError::InvalidVersion {
        name: name.to_string(),
        version: version.to_string(),
    })?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_manifest() {
        let toml_str = r#"
[package]
name = "my_app"
version = "0.1.0"
cobrust-version = "0.0.1"

[bin]
name = "my_app"
path = "src/main.cb"
"#;
        let m = Manifest::parse_str(toml_str).unwrap();
        assert_eq!(m.package.name, "my_app");
        assert_eq!(m.package.version, "0.1.0");
        assert!(m.dependencies.is_empty());
        assert!(m.bin.is_some());
        assert!(m.lib.is_none());
    }

    #[test]
    fn parse_with_dependencies() {
        let toml_str = r#"
[package]
name = "my_app"
version = "0.1.0"
cobrust-version = "0.0.1"

[dependencies]
cobrust-tomli = { path = "../cobrust-tomli" }
serde-like = "1.2"

[bin]
name = "my_app"
path = "src/main.cb"
"#;
        let m = Manifest::parse_str(toml_str).unwrap();
        assert_eq!(m.dependencies.len(), 2);
        assert!(matches!(
            m.dependencies["cobrust-tomli"].spec,
            DependencySpec::Path { .. }
        ));
        assert!(matches!(
            m.dependencies["serde-like"].spec,
            DependencySpec::Version { .. }
        ));
    }

    #[test]
    fn rejects_router_config() {
        let toml_str = r#"
[router]
default_strategy = "quality"
cache_dir = ".cobrust/llm_cache"

[providers.anthropic]
kind = "anthropic"
"#;
        let err = Manifest::parse_str(toml_str).unwrap_err();
        assert!(matches!(
            err,
            PkgError::Manifest(ManifestError::IsRouterConfig)
        ));
    }

    #[test]
    fn rejects_no_target() {
        let toml_str = r#"
[package]
name = "my_app"
version = "0.1.0"
cobrust-version = "0.0.1"
"#;
        let err = Manifest::parse_str(toml_str).unwrap_err();
        assert!(matches!(err, PkgError::Manifest(ManifestError::NoTarget)));
    }

    #[test]
    fn rejects_invalid_name() {
        let toml_str = r#"
[package]
name = "1bad"
version = "0.1.0"
cobrust-version = "0.0.1"
[bin]
name = "1bad"
path = "src/main.cb"
"#;
        let err = Manifest::parse_str(toml_str).unwrap_err();
        assert!(matches!(
            err,
            PkgError::Manifest(ManifestError::InvalidName(_))
        ));
    }

    #[test]
    fn rejects_invalid_version() {
        let toml_str = r#"
[package]
name = "ok"
version = "not-a-version"
cobrust-version = "0.0.1"
[bin]
name = "ok"
path = "src/main.cb"
"#;
        let err = Manifest::parse_str(toml_str).unwrap_err();
        assert!(matches!(
            err,
            PkgError::Manifest(ManifestError::InvalidVersion { .. })
        ));
    }

    #[test]
    fn canonical_toml_round_trip() {
        let toml_str = r#"
[package]
name = "rt"
version = "0.1.0"
cobrust-version = "0.0.1"

[dependencies]
zebra = { path = "../zebra" }
alpha = "1.0"

[bin]
name = "rt"
path = "src/main.cb"
"#;
        let m = Manifest::parse_str(toml_str).unwrap();
        let canonical = m.canonical_toml();
        let m2 = Manifest::parse_str(&canonical).unwrap();
        assert_eq!(m, m2);
    }

    #[test]
    fn looks_like_user_crate_yes() {
        assert!(Manifest::looks_like_user_crate("[package]\nname=\"x\""));
    }

    #[test]
    fn looks_like_user_crate_no() {
        assert!(!Manifest::looks_like_user_crate(
            "[router]\ndefault_strategy=\"x\""
        ));
    }

    #[test]
    fn looks_like_user_crate_mixed() {
        // Has both — treat as router-flavor (refuses to identify as user).
        assert!(!Manifest::looks_like_user_crate(
            "[package]\nname=\"x\"\n[router]\ndefault_strategy=\"x\""
        ));
    }

    #[test]
    fn manifest_hash_is_deterministic() {
        let toml_a = r#"
[package]
name = "h"
version = "0.1.0"
cobrust-version = "0.0.1"

[dependencies]
b = "1.0"
a = "2.0"

[bin]
name = "h"
path = "src/main.cb"
"#;
        let toml_b = r#"
[package]
name = "h"
version = "0.1.0"
cobrust-version = "0.0.1"

[dependencies]
a = "2.0"
b = "1.0"

[bin]
name = "h"
path = "src/main.cb"
"#;
        let ma = Manifest::parse_str(toml_a).unwrap();
        let mb = Manifest::parse_str(toml_b).unwrap();
        // BTreeMap normalizes order; canonical TOML is identical; hash matches.
        assert_eq!(ma.manifest_hash(), mb.manifest_hash());
    }

    #[test]
    fn rejects_conflicting_paths() {
        let toml_str = r#"
[package]
name = "c"
version = "0.1.0"
cobrust-version = "0.0.1"

[bin]
name = "c"
path = "src/lib.cb"

[lib]
name = "c_lib"
path = "src/lib.cb"
"#;
        let err = Manifest::parse_str(toml_str).unwrap_err();
        assert!(matches!(
            err,
            PkgError::Manifest(ManifestError::ConflictingPaths { .. })
        ));
    }

    #[test]
    fn parse_dev_dependencies() {
        let toml_str = r#"
[package]
name = "d"
version = "0.1.0"
cobrust-version = "0.0.1"

[dev-dependencies]
test_helpers = { path = "./helpers" }

[bin]
name = "d"
path = "src/main.cb"
"#;
        let m = Manifest::parse_str(toml_str).unwrap();
        assert_eq!(m.dev_dependencies.len(), 1);
    }

    #[test]
    fn parse_multiple_tests() {
        let toml_str = r#"
[package]
name = "t"
version = "0.1.0"
cobrust-version = "0.0.1"

[bin]
name = "t"
path = "src/main.cb"

[[test]]
name = "smoke"
path = "tests/smoke.cb"

[[test]]
name = "deep"
path = "tests/deep.cb"
"#;
        let m = Manifest::parse_str(toml_str).unwrap();
        assert_eq!(m.tests.len(), 2);
        assert_eq!(m.tests[0].name, "smoke");
        assert_eq!(m.tests[1].name, "deep");
    }

    #[test]
    fn parse_git_dependency() {
        let toml_str = r#"
[package]
name = "g"
version = "0.1.0"
cobrust-version = "0.0.1"

[dependencies]
remote = { git = "https://github.com/x/y", rev = "abc123" }

[bin]
name = "g"
path = "src/main.cb"
"#;
        let m = Manifest::parse_str(toml_str).unwrap();
        match &m.dependencies["remote"].spec {
            DependencySpec::Git { url, rev } => {
                assert_eq!(url, "https://github.com/x/y");
                assert_eq!(rev, "abc123");
            }
            _ => panic!("expected git source"),
        }
    }

    #[test]
    fn rejects_git_without_rev() {
        let toml_str = r#"
[package]
name = "g"
version = "0.1.0"
cobrust-version = "0.0.1"

[dependencies]
remote = { git = "https://github.com/x/y" }

[bin]
name = "g"
path = "src/main.cb"
"#;
        let err = Manifest::parse_str(toml_str).unwrap_err();
        assert!(matches!(
            err,
            PkgError::Manifest(ManifestError::InvalidDependency { .. })
        ));
    }

    #[test]
    fn rejects_path_plus_version() {
        let toml_str = r#"
[package]
name = "g"
version = "0.1.0"
cobrust-version = "0.0.1"

[dependencies]
mixed = { path = "../m", version = "1.0" }

[bin]
name = "g"
path = "src/main.cb"
"#;
        let err = Manifest::parse_str(toml_str).unwrap_err();
        assert!(matches!(
            err,
            PkgError::Manifest(ManifestError::InvalidDependency { .. })
        ));
    }
}
