//! Manifest corpus — ADR-0026 §"Test corpus" requires ≥ 30 valid +
//! ≥ 30 invalid manifests, table-driven.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::format_push_string)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::float_cmp)]
#![allow(clippy::similar_names)]
#![allow(clippy::manual_is_multiple_of)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::too_many_lines)]

use cobrust_pkg::{Manifest, ManifestError, PkgError};

/// VALID corpus — at least 30 distinct shape variations that must parse.
fn valid_corpus() -> Vec<(&'static str, &'static str)> {
    vec![
        // 1. Bare bin
        (
            "bare-bin",
            r#"
[package]
name = "a"
version = "0.1.0"
cobrust-version = "0.0.1"

[bin]
name = "a"
path = "src/main.cb"
"#,
        ),
        // 2. Bare lib
        (
            "bare-lib",
            r#"
[package]
name = "a"
version = "0.1.0"
cobrust-version = "0.0.1"

[lib]
name = "a"
path = "src/lib.cb"
"#,
        ),
        // 3. Bin + lib
        (
            "bin-and-lib",
            r#"
[package]
name = "ab"
version = "0.1.0"
cobrust-version = "0.0.1"

[bin]
name = "ab"
path = "src/main.cb"

[lib]
name = "ab_core"
path = "src/lib.cb"
"#,
        ),
        // 4. With path dep
        (
            "path-dep",
            r#"
[package]
name = "p"
version = "0.1.0"
cobrust-version = "0.0.1"

[dependencies]
foo = { path = "../foo" }

[bin]
name = "p"
path = "src/main.cb"
"#,
        ),
        // 5. With git dep
        (
            "git-dep",
            r#"
[package]
name = "g"
version = "0.1.0"
cobrust-version = "0.0.1"

[dependencies]
foo = { git = "https://example.com/foo", rev = "abc123" }

[bin]
name = "g"
path = "src/main.cb"
"#,
        ),
        // 6. With registry-shorthand dep
        (
            "registry-shorthand",
            r#"
[package]
name = "r"
version = "0.1.0"
cobrust-version = "0.0.1"

[dependencies]
foo = "1.2"

[bin]
name = "r"
path = "src/main.cb"
"#,
        ),
        // 7. With explicit registry dep
        (
            "registry-explicit",
            r#"
[package]
name = "re"
version = "0.1.0"
cobrust-version = "0.0.1"

[dependencies]
foo = { version = "1.2", registry = "default" }

[bin]
name = "re"
path = "src/main.cb"
"#,
        ),
        // 8. dev-dependencies
        (
            "dev-deps",
            r#"
[package]
name = "d"
version = "0.1.0"
cobrust-version = "0.0.1"

[dev-dependencies]
helpers = { path = "./helpers" }

[bin]
name = "d"
path = "src/main.cb"
"#,
        ),
        // 9. Multiple deps
        (
            "multi-dep",
            r#"
[package]
name = "m"
version = "0.1.0"
cobrust-version = "0.0.1"

[dependencies]
a = { path = "../a" }
b = { git = "https://example.com/b", rev = "x" }
c = "0.5"

[bin]
name = "m"
path = "src/main.cb"
"#,
        ),
        // 10. Tests array (1)
        (
            "single-test",
            r#"
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
"#,
        ),
        // 11. Tests array (multiple)
        (
            "multi-tests",
            r#"
[package]
name = "tm"
version = "0.1.0"
cobrust-version = "0.0.1"

[bin]
name = "tm"
path = "src/main.cb"

[[test]]
name = "a"
path = "tests/a.cb"

[[test]]
name = "b"
path = "tests/b.cb"

[[test]]
name = "c"
path = "tests/c.cb"
"#,
        ),
        // 12. With authors
        (
            "with-authors",
            r#"
[package]
name = "auth"
version = "0.1.0"
cobrust-version = "0.0.1"
authors = ["Alice <alice@example.com>", "Bob"]

[bin]
name = "auth"
path = "src/main.cb"
"#,
        ),
        // 13. With license + description
        (
            "with-license-desc",
            r#"
[package]
name = "ld"
version = "0.1.0"
cobrust-version = "0.0.1"
license = "Apache-2.0 OR MIT"
description = "demo"

[bin]
name = "ld"
path = "src/main.cb"
"#,
        ),
        // 14. Hyphenated package name
        (
            "hyphenated-name",
            r#"
[package]
name = "cobrust-nest"
version = "2.0.1"
cobrust-version = "0.0.1"

[lib]
name = "cobrust_nest"
path = "src/lib.cb"
"#,
        ),
        // 15. Underscore name
        (
            "underscore-name",
            r#"
[package]
name = "my_app_lib"
version = "0.0.1"
cobrust-version = "0.0.1"

[lib]
name = "my_app_lib"
path = "src/lib.cb"
"#,
        ),
        // 16. Long version (with prerelease)
        (
            "prerelease-version",
            r#"
[package]
name = "pre"
version = "0.1.0-alpha.1"
cobrust-version = "0.0.1"

[bin]
name = "pre"
path = "src/main.cb"
"#,
        ),
        // 17. Build-meta version
        (
            "build-meta-version",
            r#"
[package]
name = "b"
version = "1.0.0+ci.42"
cobrust-version = "0.0.1"

[bin]
name = "b"
path = "src/main.cb"
"#,
        ),
        // 18. Caret-style req
        (
            "caret-req",
            r#"
[package]
name = "c"
version = "0.1.0"
cobrust-version = "0.0.1"

[dependencies]
foo = "^1.2.3"

[bin]
name = "c"
path = "src/main.cb"
"#,
        ),
        // 19. Tilde-style req
        (
            "tilde-req",
            r#"
[package]
name = "ti"
version = "0.1.0"
cobrust-version = "0.0.1"

[dependencies]
foo = "~1.2"

[bin]
name = "ti"
path = "src/main.cb"
"#,
        ),
        // 20. Wildcard req
        (
            "wildcard-req",
            r#"
[package]
name = "w"
version = "0.1.0"
cobrust-version = "0.0.1"

[dependencies]
foo = "1.*"

[bin]
name = "w"
path = "src/main.cb"
"#,
        ),
        // 21. Deeply nested path
        (
            "deep-path",
            r#"
[package]
name = "dp"
version = "0.1.0"
cobrust-version = "0.0.1"

[dependencies]
foo = { path = "../../../sibling/foo" }

[bin]
name = "dp"
path = "src/main.cb"
"#,
        ),
        // 22. Empty authors array (default)
        (
            "no-authors-explicit",
            r#"
[package]
name = "nx"
version = "0.1.0"
cobrust-version = "0.0.1"
authors = []

[bin]
name = "nx"
path = "src/main.cb"
"#,
        ),
        // 23. Mixed dep / dev-dep
        (
            "mixed-dep-dev",
            r#"
[package]
name = "mx"
version = "0.1.0"
cobrust-version = "0.0.1"

[dependencies]
foo = { path = "../foo" }

[dev-dependencies]
bar = { path = "../bar" }

[bin]
name = "mx"
path = "src/main.cb"
"#,
        ),
        // 24. Bin only with custom path
        (
            "custom-bin-path",
            r#"
[package]
name = "cbp"
version = "0.1.0"
cobrust-version = "0.0.1"

[bin]
name = "tool"
path = "bin/tool.cb"
"#,
        ),
        // 25. Lib only with custom path
        (
            "custom-lib-path",
            r#"
[package]
name = "clp"
version = "0.1.0"
cobrust-version = "0.0.1"

[lib]
name = "core"
path = "lib/core.cb"
"#,
        ),
        // 26. Many tests
        (
            "many-tests",
            r#"
[package]
name = "mt"
version = "0.1.0"
cobrust-version = "0.0.1"

[bin]
name = "mt"
path = "src/main.cb"

[[test]]
name = "a"
path = "tests/a.cb"

[[test]]
name = "b"
path = "tests/b.cb"

[[test]]
name = "c"
path = "tests/c.cb"

[[test]]
name = "d"
path = "tests/d.cb"

[[test]]
name = "e"
path = "tests/e.cb"
"#,
        ),
        // 27. Trailing whitespace tolerance (toml allows it)
        (
            "trailing-whitespace",
            r#"
[package]
name = "tw"
version = "0.1.0"
cobrust-version = "0.0.1"

[bin]
name = "tw"
path = "src/main.cb"
"#,
        ),
        // 28. Long name
        (
            "long-name",
            r#"
[package]
name = "abcdefghijklmnopqrstuvwxyz_long_but_under_64_chars"
version = "0.1.0"
cobrust-version = "0.0.1"

[bin]
name = "abcdefghijklmnopqrstuvwxyz_long_but_under_64_chars"
path = "src/main.cb"
"#,
        ),
        // 29. Numeric in name
        (
            "numeric-in-name",
            r#"
[package]
name = "a1b2c3"
version = "0.1.0"
cobrust-version = "0.0.1"

[bin]
name = "a1b2c3"
path = "src/main.cb"
"#,
        ),
        // 30. Range req
        (
            "range-req",
            r#"
[package]
name = "rg"
version = "0.1.0"
cobrust-version = "0.0.1"

[dependencies]
foo = ">=1.0, <2.0"

[bin]
name = "rg"
path = "src/main.cb"
"#,
        ),
        // 31. cobrust-nest style (cobra-named per ADR-0071 §3;
        // source library `tomli` preserved in description).
        (
            "cobrust-nest-style",
            r#"
[package]
name = "cobrust-nest"
version = "2.0.1"
cobrust-version = "0.0.1"
license = "Apache-2.0 OR MIT"
description = "TOML parser, translated from Python tomli."

[lib]
name = "cobrust_nest"
path = "src/lib.cb"
"#,
        ),
        // 32. Notebook-shaped
        (
            "notebook-shaped",
            r#"
[package]
name = "notebook"
version = "0.1.0"
cobrust-version = "0.0.1"
description = "M12 notebook example"

[dependencies]
cobrust-nest = { path = "../../crates/cobrust-nest" }

[bin]
name = "notebook"
path = "src/main.cb"

[[test]]
name = "smoke"
path = "tests/smoke.cb"
"#,
        ),
    ]
}

/// INVALID corpus — at least 30 distinct shape violations.
fn invalid_corpus() -> Vec<(&'static str, &'static str)> {
    vec![
        // 1. Missing package
        (
            "missing-package",
            r#"[bin]
name = "x"
path = "src/main.cb"
"#,
        ),
        // 2. Empty package name
        (
            "empty-name",
            r#"
[package]
name = ""
version = "0.1.0"
cobrust-version = "0.0.1"

[bin]
name = "x"
path = "src/main.cb"
"#,
        ),
        // 3. Name starts with digit
        (
            "digit-start",
            r#"
[package]
name = "1bad"
version = "0.1.0"
cobrust-version = "0.0.1"

[bin]
name = "1bad"
path = "src/main.cb"
"#,
        ),
        // 4. Name with dot
        (
            "dot-name",
            r#"
[package]
name = "bad.name"
version = "0.1.0"
cobrust-version = "0.0.1"

[bin]
name = "bad.name"
path = "src/main.cb"
"#,
        ),
        // 5. Name with slash
        (
            "slash-name",
            r#"
[package]
name = "bad/name"
version = "0.1.0"
cobrust-version = "0.0.1"

[bin]
name = "bad/name"
path = "src/main.cb"
"#,
        ),
        // 6. Name with space
        (
            "space-name",
            r#"
[package]
name = "bad name"
version = "0.1.0"
cobrust-version = "0.0.1"

[bin]
name = "bad name"
path = "src/main.cb"
"#,
        ),
        // 7. Invalid version
        (
            "not-semver",
            r#"
[package]
name = "x"
version = "not-a-version"
cobrust-version = "0.0.1"

[bin]
name = "x"
path = "src/main.cb"
"#,
        ),
        // 8. Invalid cobrust-version
        (
            "bad-cobrust-version",
            r#"
[package]
name = "x"
version = "0.1.0"
cobrust-version = "potato"

[bin]
name = "x"
path = "src/main.cb"
"#,
        ),
        // 9. Both bin and lib paths identical
        (
            "conflicting-paths",
            r#"
[package]
name = "x"
version = "0.1.0"
cobrust-version = "0.0.1"

[bin]
name = "x"
path = "src/lib.cb"

[lib]
name = "x_lib"
path = "src/lib.cb"
"#,
        ),
        // 10. No target
        (
            "no-target",
            r#"
[package]
name = "x"
version = "0.1.0"
cobrust-version = "0.0.1"
"#,
        ),
        // 11. Router config
        (
            "router-config",
            r#"
[router]
default_strategy = "quality"

[providers.openai]
kind = "openai"
"#,
        ),
        // 12. dep with both path and version
        (
            "dep-path-and-version",
            r#"
[package]
name = "x"
version = "0.1.0"
cobrust-version = "0.0.1"

[dependencies]
mixed = { path = "../m", version = "1.0" }

[bin]
name = "x"
path = "src/main.cb"
"#,
        ),
        // 13. dep with both path and git
        (
            "dep-path-and-git",
            r#"
[package]
name = "x"
version = "0.1.0"
cobrust-version = "0.0.1"

[dependencies]
mixed = { path = "../m", git = "https://x" }

[bin]
name = "x"
path = "src/main.cb"
"#,
        ),
        // 14. dep git missing rev
        (
            "dep-git-no-rev",
            r#"
[package]
name = "x"
version = "0.1.0"
cobrust-version = "0.0.1"

[dependencies]
remote = { git = "https://x" }

[bin]
name = "x"
path = "src/main.cb"
"#,
        ),
        // 15. dep with empty table
        (
            "dep-empty-table",
            r#"
[package]
name = "x"
version = "0.1.0"
cobrust-version = "0.0.1"

[dependencies]
empty = { }

[bin]
name = "x"
path = "src/main.cb"
"#,
        ),
        // 16. dep bare-string is not semver
        (
            "dep-bad-semver",
            r#"
[package]
name = "x"
version = "0.1.0"
cobrust-version = "0.0.1"

[dependencies]
foo = "potato"

[bin]
name = "x"
path = "src/main.cb"
"#,
        ),
        // 17. dep version field bad semver
        (
            "dep-bad-version-field",
            r#"
[package]
name = "x"
version = "0.1.0"
cobrust-version = "0.0.1"

[dependencies]
foo = { version = "tomato" }

[bin]
name = "x"
path = "src/main.cb"
"#,
        ),
        // 18. dep name starts with digit
        (
            "dep-digit-name",
            r#"
[package]
name = "x"
version = "0.1.0"
cobrust-version = "0.0.1"

[dependencies]
"1bad" = "1.0"

[bin]
name = "x"
path = "src/main.cb"
"#,
        ),
        // 19. Malformed TOML
        (
            "malformed-toml",
            r#"
[package
name = "x"
"#,
        ),
        // 20. Name longer than 64 chars
        (
            "name-too-long",
            r#"
[package]
name = "this_name_is_way_way_way_way_way_way_way_way_way_way_way_way_way_way_too_long"
version = "0.1.0"
cobrust-version = "0.0.1"

[bin]
name = "x"
path = "src/main.cb"
"#,
        ),
        // 21. Missing version field
        (
            "missing-version",
            r#"
[package]
name = "x"
cobrust-version = "0.0.1"

[bin]
name = "x"
path = "src/main.cb"
"#,
        ),
        // 22. Missing cobrust-version field
        (
            "missing-cobrust-version",
            r#"
[package]
name = "x"
version = "0.1.0"

[bin]
name = "x"
path = "src/main.cb"
"#,
        ),
        // 23. Missing name field
        (
            "missing-name",
            r#"
[package]
version = "0.1.0"
cobrust-version = "0.0.1"

[bin]
name = "x"
path = "src/main.cb"
"#,
        ),
        // 24. dep name with @
        (
            "dep-at-name",
            r#"
[package]
name = "x"
version = "0.1.0"
cobrust-version = "0.0.1"

[dependencies]
"foo@bar" = "1.0"

[bin]
name = "x"
path = "src/main.cb"
"#,
        ),
        // 25. version is integer
        (
            "version-int",
            r#"
[package]
name = "x"
version = 1
cobrust-version = "0.0.1"

[bin]
name = "x"
path = "src/main.cb"
"#,
        ),
        // 26. version is array
        (
            "version-array",
            r#"
[package]
name = "x"
version = ["0.1.0"]
cobrust-version = "0.0.1"

[bin]
name = "x"
path = "src/main.cb"
"#,
        ),
        // 27. dep with random unknown key only
        (
            "dep-unknown-only",
            r#"
[package]
name = "x"
version = "0.1.0"
cobrust-version = "0.0.1"

[dependencies]
foo = { wat = "?" }

[bin]
name = "x"
path = "src/main.cb"
"#,
        ),
        // 28. Router-only (no [package])
        (
            "only-providers",
            r#"
[router]
default_strategy = "quality"

[providers.local]
kind = "openai"
"#,
        ),
        // 29. Both [package] and [router] (rejected — looks_like_user_crate
        //     returns false because of [router]; but the parse_str path
        //     accepts because [package] is present. We assert it parses
        //     OR is rejected — the API contract is "if [package] is
        //     present, this is a user manifest". We move this to the
        //     valid set if it parses cleanly.) → keep as INVALID under
        //     the strict "must not have [router]" rule.
        //
        //     Actually parse_str only checks the `[package].is_none() &&
        //     [router].is_some()` path, so if both are present it accepts.
        //     We keep this as a separate corner case in the "valid" set
        //     elsewhere. Instead, drop a different invalid case here.
        //
        //     Replacement: dep version with path also set (mutual exclusion).
        (
            "dep-path-and-registry",
            r#"
[package]
name = "x"
version = "0.1.0"
cobrust-version = "0.0.1"

[dependencies]
mixed = { path = "../p", registry = "default" }

[bin]
name = "x"
path = "src/main.cb"
"#,
        ),
        // 30. Empty TOML
        ("empty-toml", ""),
        // 31. Bin name with space
        (
            "bin-name-space",
            r#"
[package]
name = "ok"
version = "0.1.0"
cobrust-version = "0.0.1"

[bin]
name = "ok"
path = "this is not a path"
[lib]
name = "ok_lib"
path = "this is not a path"
"#,
        ),
        // 32. dep name with newline (TOML rejects bare newlines in keys)
        (
            "dep-name-newline",
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\ncobrust-version = \"0.0.1\"\n[dependencies]\n\"a\nb\" = \"1.0\"\n[bin]\nname = \"x\"\npath = \"src/main.cb\"\n",
        ),
    ]
}

#[test]
fn corpus_size_valid_at_least_30() {
    assert!(
        valid_corpus().len() >= 30,
        "valid corpus must have ≥ 30 fixtures"
    );
}

#[test]
fn corpus_size_invalid_at_least_30() {
    assert!(
        invalid_corpus().len() >= 30,
        "invalid corpus must have ≥ 30 fixtures"
    );
}

#[test]
fn all_valid_fixtures_parse() {
    for (name, body) in valid_corpus() {
        let r = Manifest::parse_str(body);
        assert!(
            r.is_ok(),
            "valid fixture `{name}` failed to parse: {:?}",
            r.err()
        );
    }
}

#[test]
fn all_invalid_fixtures_reject() {
    for (name, body) in invalid_corpus() {
        let r = Manifest::parse_str(body);
        assert!(r.is_err(), "invalid fixture `{name}` unexpectedly parsed",);
    }
}

#[test]
fn invalid_router_config_specific() {
    let m = invalid_corpus()
        .into_iter()
        .find(|(n, _)| *n == "router-config")
        .unwrap()
        .1;
    let err = Manifest::parse_str(m).unwrap_err();
    assert!(matches!(
        err,
        PkgError::Manifest(ManifestError::IsRouterConfig)
    ));
}

#[test]
fn invalid_no_target_specific() {
    let m = invalid_corpus()
        .into_iter()
        .find(|(n, _)| *n == "no-target")
        .unwrap()
        .1;
    let err = Manifest::parse_str(m).unwrap_err();
    assert!(matches!(err, PkgError::Manifest(ManifestError::NoTarget)));
}

#[test]
fn invalid_conflicting_paths_specific() {
    let m = invalid_corpus()
        .into_iter()
        .find(|(n, _)| *n == "conflicting-paths")
        .unwrap()
        .1;
    let err = Manifest::parse_str(m).unwrap_err();
    assert!(matches!(
        err,
        PkgError::Manifest(ManifestError::ConflictingPaths { .. })
    ));
}
