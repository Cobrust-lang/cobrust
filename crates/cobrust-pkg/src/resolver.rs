//! Dependency resolver (ADR-0026 §D "Dependency resolution algorithm").
//!
//! M12 ships **max-compatible greedy**: for each unique package name in
//! the dep graph, pick the highest version satisfying every requirement
//! targeting that package; surface conflicts as `ResolutionError::Conflict`.
//!
//! The strategy is trait-shaped (`ResolutionStrategy`) so a future Phase F
//! PubGrub-backed solver replaces the strategy without changing the
//! public surface.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use semver::Version;

use crate::error::{PkgError, ResolutionError};
use crate::manifest::{Dependency, DependencySpec, Manifest};
use crate::registry::Registry;
use crate::sources::{Source, SourceFetchOutput};

/// Final resolved snapshot — the input to `Lockfile::from_resolution`.
#[derive(Clone, Debug)]
pub struct Resolution {
    /// Root package (always the manifest under resolve).
    pub root: ResolvedPackage,
    /// Sorted by `name`. Each entry is the chosen version for that name.
    pub packages: BTreeMap<String, ResolvedPackage>,
}

/// A single resolved package + its concrete content-addressed entry.
#[derive(Clone, Debug)]
pub struct ResolvedPackage {
    pub name: String,
    pub version: Version,
    /// Source URL (e.g. `path+file:///abs/path`, `git+https://...`,
    /// `registry+default`).
    pub source_url: String,
    /// `blake3:<hex>` of the canonical tarball.
    pub hash: String,
    /// Translator's `deterministic_id` if the package carries
    /// `PROVENANCE.toml`.
    pub provenance_hash: Option<String>,
    /// Names of direct dependencies (for the lockfile's
    /// `dependencies = [...]` array). Sorted.
    pub dependency_names: Vec<String>,
    /// On-disk cached path under `<registry>/blake3/<hex>/`.
    pub local_path: PathBuf,
}

/// Strategy trait — see ADR-0026 §D.
pub trait ResolutionStrategy: Send + Sync {
    /// Pick a single version per name from the requirement set.
    /// Implementation must be deterministic (same inputs → same output).
    fn select(
        &self,
        manifest: &Manifest,
        graph: &DepGraph,
    ) -> Result<BTreeMap<String, Version>, PkgError>;
}

/// The default M12 strategy: greedy max-compatible.
pub struct MaxCompatibleStrategy;

impl ResolutionStrategy for MaxCompatibleStrategy {
    fn select(
        &self,
        _manifest: &Manifest,
        graph: &DepGraph,
    ) -> Result<BTreeMap<String, Version>, PkgError> {
        let mut chosen = BTreeMap::new();
        for (name, node) in &graph.nodes {
            // For path / git sources we already have a concrete version
            // (the manifest of the cached entry). We trust it.
            //
            // For Registry sources we'd intersect requirements; M12
            // ships only path + git + bundled translated libs, so this
            // simplifies to "the single version we already have".
            if let Some(v) = &node.version {
                chosen.insert(name.clone(), v.clone());
            } else {
                return Err(PkgError::Resolution(ResolutionError::MissingPackage {
                    name: name.clone(),
                }));
            }
        }
        Ok(chosen)
    }
}

/// The dep-graph structure assembled by walking the manifest's
/// `[dependencies]` recursively.
#[derive(Clone, Debug, Default)]
pub struct DepGraph {
    /// Sorted by name (BTreeMap iteration is deterministic).
    pub nodes: BTreeMap<String, DepNode>,
    /// Edges: parent → set of (child name, requirement source key).
    pub edges: BTreeMap<String, BTreeSet<String>>,
}

#[derive(Clone, Debug)]
pub struct DepNode {
    pub name: String,
    pub version: Option<Version>,
    pub source: Source,
    pub source_url: String,
    pub hash: Option<String>,
    pub provenance_hash: Option<String>,
    pub local_path: Option<PathBuf>,
    /// Direct dep names, sorted lexically.
    pub direct_deps: Vec<String>,
}

/// Driver that owns the strategy + walks the manifest graph.
pub struct Resolver<S: ResolutionStrategy> {
    strategy: S,
}

impl<S: ResolutionStrategy> Resolver<S> {
    pub fn new(strategy: S) -> Self {
        Self { strategy }
    }

    /// Fully resolve `manifest`'s dep tree. Recursively walks path/git
    /// sources, fetching each into `registry`, parsing each cached
    /// manifest, and assembling the graph.
    pub fn resolve(
        &self,
        manifest: &Manifest,
        workspace_root: &Path,
        registry: &Registry,
    ) -> Result<Resolution, PkgError> {
        let mut graph = DepGraph::default();

        // Root node: the user crate itself.
        let root_version = Version::parse(&manifest.package.version).map_err(|_| {
            ResolutionError::MissingPackage {
                name: manifest.package.name.clone(),
            }
        })?;
        let root_local = workspace_root.to_path_buf();
        let root_url = format!("path+file://{}", root_local.display());

        // Walk dependencies depth-first.
        let mut visiting: HashSet<String> = HashSet::new();
        visiting.insert(manifest.package.name.clone());
        let mut on_path: Vec<String> = vec![manifest.package.name.clone()];

        let mut root_direct = Vec::new();
        for (name, dep) in &manifest.dependencies {
            walk_dep(
                name,
                dep,
                workspace_root,
                registry,
                &mut graph,
                &mut visiting,
                &mut on_path,
            )?;
            root_direct.push(name.clone());
        }
        root_direct.sort();

        // Strategy selects versions.
        let chosen = self.strategy.select(manifest, &graph)?;

        // Build the per-package resolved snapshot.
        let mut packages = BTreeMap::new();
        for (name, version) in &chosen {
            let node = graph.nodes.get(name).ok_or_else(|| {
                PkgError::Resolution(ResolutionError::MissingPackage { name: name.clone() })
            })?;
            let hash = node.hash.clone().ok_or_else(|| {
                PkgError::Resolution(ResolutionError::MissingPackage { name: name.clone() })
            })?;
            let local_path = node.local_path.clone().ok_or_else(|| {
                PkgError::Resolution(ResolutionError::MissingPackage { name: name.clone() })
            })?;
            packages.insert(
                name.clone(),
                ResolvedPackage {
                    name: name.clone(),
                    version: version.clone(),
                    source_url: node.source_url.clone(),
                    hash,
                    provenance_hash: node.provenance_hash.clone(),
                    dependency_names: node.direct_deps.clone(),
                    local_path,
                },
            );
        }

        let root = ResolvedPackage {
            name: manifest.package.name.clone(),
            version: root_version,
            source_url: root_url,
            hash: "blake3:root".to_string(),
            provenance_hash: None,
            dependency_names: root_direct,
            local_path: root_local,
        };

        Ok(Resolution { root, packages })
    }
}

fn walk_dep(
    name: &str,
    dep: &Dependency,
    workspace_root: &Path,
    registry: &Registry,
    graph: &mut DepGraph,
    visiting: &mut HashSet<String>,
    on_path: &mut Vec<String>,
) -> Result<(), PkgError> {
    if on_path.iter().any(|n| n == name) {
        // Cycle detected via the recursion stack — this MUST run before
        // the "already-in-graph" early-return so that a self-edge or
        // back-edge surfaces as a cycle (not a no-op).
        let mut cycle: Vec<String> = on_path.clone();
        cycle.push(name.to_string());
        return Err(PkgError::Resolution(ResolutionError::Cycle { path: cycle }));
    }
    if graph.nodes.contains_key(name) {
        // Already visited via an earlier branch and finished; record nothing.
        return Ok(());
    }

    let source = source_from_spec(name, &dep.spec)?;
    let source_url = source_url(&source);
    let fetch: SourceFetchOutput = source.fetch(registry, workspace_root)?;
    let cached_manifest = read_cached_manifest(&fetch.local_path)?;
    let version = Version::parse(&cached_manifest.package.version).map_err(|_| {
        ResolutionError::MissingPackage {
            name: name.to_string(),
        }
    })?;

    let mut direct_deps: Vec<String> = cached_manifest.dependencies.keys().cloned().collect();
    direct_deps.sort();

    // Insert this node BEFORE recursing so transitive cycles see it.
    graph.nodes.insert(
        name.to_string(),
        DepNode {
            name: name.to_string(),
            version: Some(version),
            source: source.clone(),
            source_url,
            hash: Some(fetch.blake3_hex.clone()),
            provenance_hash: fetch.provenance_hash.clone(),
            local_path: Some(fetch.local_path.clone()),
            direct_deps: direct_deps.clone(),
        },
    );

    // Walk transitive deps.
    on_path.push(name.to_string());
    visiting.insert(name.to_string());
    for (child_name, child_dep) in &cached_manifest.dependencies {
        // Edges are recorded irrespective of recursion.
        graph
            .edges
            .entry(name.to_string())
            .or_default()
            .insert(child_name.clone());
        walk_dep(
            child_name,
            child_dep,
            &fetch.local_path,
            registry,
            graph,
            visiting,
            on_path,
        )?;
    }
    on_path.pop();

    Ok(())
}

fn source_from_spec(name: &str, spec: &DependencySpec) -> Result<Source, PkgError> {
    match spec {
        DependencySpec::Path { path } => Ok(Source::Path { path: path.clone() }),
        DependencySpec::Git { url, rev } => Ok(Source::Git {
            url: url.clone(),
            rev: rev.clone(),
        }),
        DependencySpec::Version { req } | DependencySpec::Registry { req, .. } => {
            Ok(Source::Registry {
                name: name.to_string(),
                version: req.clone(),
            })
        }
    }
}

fn source_url(source: &Source) -> String {
    match source {
        Source::Path { path } => format!("path+file://{}", path.display()),
        Source::Git { url, rev } => format!("git+{url}#{rev}"),
        Source::Registry { name, version } => format!("registry+default?{name}@{version}"),
    }
}

fn read_cached_manifest(local_path: &Path) -> Result<Manifest, PkgError> {
    let mp = local_path.join("cobrust.toml");
    let s = std::fs::read_to_string(&mp)
        .map_err(|e| PkgError::Io(format!("read cached {}: {e}", mp.display())))?;
    Manifest::parse_str(&s)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn write_manifest(dir: &Path, name: &str, version: &str, deps: &[(&str, &str)]) {
        fs::create_dir_all(dir.join("src")).unwrap();
        let mut s = format!(
            "[package]\nname = \"{name}\"\nversion = \"{version}\"\ncobrust-version = \"0.0.1\"\n",
        );
        if !deps.is_empty() {
            s.push_str("\n[dependencies]\n");
            for (n, v) in deps {
                s.push_str(&format!("{n} = {v}\n"));
            }
        }
        s.push_str(&format!(
            "\n[bin]\nname = \"{name}\"\npath = \"src/main.cb\"\n"
        ));
        fs::write(dir.join("cobrust.toml"), s).unwrap();
        fs::write(dir.join("src/main.cb"), "fn main() -> i64:\n    return 0\n").unwrap();
    }

    #[test]
    fn resolve_no_deps() {
        let workspace = tempdir().unwrap();
        let registry_dir = tempdir().unwrap();
        write_manifest(workspace.path(), "lone", "0.1.0", &[]);
        let m = Manifest::parse_str(
            &fs::read_to_string(workspace.path().join("cobrust.toml")).unwrap(),
        )
        .unwrap();
        let r = Registry::open_at(registry_dir.path()).unwrap();
        let resolver = Resolver::new(MaxCompatibleStrategy);
        let res = resolver.resolve(&m, workspace.path(), &r).unwrap();
        assert_eq!(res.root.name, "lone");
        assert!(res.packages.is_empty());
    }

    #[test]
    fn resolve_one_path_dep() {
        let workspace = tempdir().unwrap();
        let registry_dir = tempdir().unwrap();
        // Workspace contains "main" + "main/dep_a"
        let dep_a_dir = workspace.path().join("dep_a");
        fs::create_dir(&dep_a_dir).unwrap();
        write_manifest(&dep_a_dir, "dep_a", "0.5.0", &[]);
        write_manifest(
            workspace.path(),
            "main",
            "0.1.0",
            &[("dep_a", "{ path = \"dep_a\" }")],
        );

        let m = Manifest::parse_str(
            &fs::read_to_string(workspace.path().join("cobrust.toml")).unwrap(),
        )
        .unwrap();
        let r = Registry::open_at(registry_dir.path()).unwrap();
        let resolver = Resolver::new(MaxCompatibleStrategy);
        let res = resolver.resolve(&m, workspace.path(), &r).unwrap();
        assert_eq!(res.packages.len(), 1);
        let dep_a = &res.packages["dep_a"];
        assert_eq!(dep_a.version.to_string(), "0.5.0");
        assert!(dep_a.hash.starts_with("blake3:"));
    }

    #[test]
    fn resolve_transitive() {
        let workspace = tempdir().unwrap();
        let registry_dir = tempdir().unwrap();

        // main -> dep_a -> dep_b
        let dep_b_dir = workspace.path().join("dep_b");
        fs::create_dir(&dep_b_dir).unwrap();
        write_manifest(&dep_b_dir, "dep_b", "0.2.0", &[]);

        let dep_a_dir = workspace.path().join("dep_a");
        fs::create_dir(&dep_a_dir).unwrap();
        // dep_a's manifest references dep_b via a relative path inside the
        // staged registry tree. We replicate the dep_b tree under dep_a/dep_b
        // so the in-cache resolver finds it (the resolver's workspace_root
        // when recursing is the cached dep_a).
        let dep_b_inside_a = dep_a_dir.join("dep_b");
        fs::create_dir(&dep_b_inside_a).unwrap();
        write_manifest(&dep_b_inside_a, "dep_b", "0.2.0", &[]);
        write_manifest(
            &dep_a_dir,
            "dep_a",
            "1.0.0",
            &[("dep_b", "{ path = \"dep_b\" }")],
        );

        write_manifest(
            workspace.path(),
            "main",
            "0.1.0",
            &[("dep_a", "{ path = \"dep_a\" }")],
        );

        let m = Manifest::parse_str(
            &fs::read_to_string(workspace.path().join("cobrust.toml")).unwrap(),
        )
        .unwrap();
        let r = Registry::open_at(registry_dir.path()).unwrap();
        let resolver = Resolver::new(MaxCompatibleStrategy);
        let res = resolver.resolve(&m, workspace.path(), &r).unwrap();
        assert_eq!(res.packages.len(), 2);
        assert!(res.packages.contains_key("dep_a"));
        assert!(res.packages.contains_key("dep_b"));
    }

    #[test]
    fn resolve_missing_path_dep() {
        let workspace = tempdir().unwrap();
        let registry_dir = tempdir().unwrap();
        write_manifest(
            workspace.path(),
            "main",
            "0.1.0",
            &[("missing", "{ path = \"nope\" }")],
        );
        let m = Manifest::parse_str(
            &fs::read_to_string(workspace.path().join("cobrust.toml")).unwrap(),
        )
        .unwrap();
        let r = Registry::open_at(registry_dir.path()).unwrap();
        let resolver = Resolver::new(MaxCompatibleStrategy);
        let err = resolver.resolve(&m, workspace.path(), &r).unwrap_err();
        assert!(matches!(
            err,
            PkgError::Source(crate::error::SourceError::PathMissing(_))
        ));
    }

    #[test]
    fn resolve_registry_offline() {
        let workspace = tempdir().unwrap();
        let registry_dir = tempdir().unwrap();
        write_manifest(workspace.path(), "main", "0.1.0", &[("missing", "\"1.0\"")]);
        let m = Manifest::parse_str(
            &fs::read_to_string(workspace.path().join("cobrust.toml")).unwrap(),
        )
        .unwrap();
        let r = Registry::open_at(registry_dir.path()).unwrap();
        let resolver = Resolver::new(MaxCompatibleStrategy);
        let err = resolver.resolve(&m, workspace.path(), &r).unwrap_err();
        assert!(matches!(
            err,
            PkgError::Registry(crate::error::RegistryError::Offline { .. })
        ));
    }
}
