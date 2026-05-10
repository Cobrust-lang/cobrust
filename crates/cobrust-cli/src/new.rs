//! `cobrust new <name>` — scaffold a Cobrust user crate per ADR-0024
//! §"Package config skeleton (M10)".
//!
//! Produces (T1.3 enhanced):
//!
//! - `<name>/cobrust.toml` — full `[package]` + `[bin]` + `[[test]]` manifest
//! - `<name>/src/main.cb` — canonical `print("hello, world")` hello-world
//! - `<name>/tests/smoke.cb` — smoke test skeleton
//! - `<name>/.gitignore` — ignores `target/` and lock files
//! - `<name>/README.md` — one-liner with link to github.com/Cobrust-lang/cobrust

use std::path::{Path, PathBuf};

use crate::exit_codes;

/// Run `cobrust new <name>`.
pub fn run(name: &str, parent_dir: Option<&Path>) -> u8 {
    if name.is_empty() {
        eprintln!("cobrust new: package name must be non-empty");
        return exit_codes::USER_ERROR;
    }

    if !is_valid_name(name) {
        eprintln!(
            "cobrust new: invalid package name `{name}` \
             (must match [a-zA-Z][a-zA-Z0-9_]*)"
        );
        return exit_codes::USER_ERROR;
    }

    let parent: PathBuf = match parent_dir {
        Some(p) => p.to_path_buf(),
        None => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    };
    let crate_dir = parent.join(name);

    if crate_dir.exists() {
        eprintln!(
            "cobrust new: directory {} already exists",
            crate_dir.display()
        );
        return exit_codes::USER_ERROR;
    }

    if let Err(e) = std::fs::create_dir_all(crate_dir.join("src")) {
        eprintln!("cobrust new: cannot create {}: {e}", crate_dir.display());
        return exit_codes::INTERNAL_PANIC;
    }

    // ADR-0026 schema: full [package] + [dependencies] (empty) + [bin]
    // + [[test]] row out of the box, so `cobrust build` / `cobrust test`
    // work immediately.
    let cobrust_toml = format!(
        "[package]\n\
         name = \"{name}\"\n\
         version = \"0.1.0\"\n\
         cobrust-version = \"0.0.1\"\n\
         description = \"A Cobrust package.\"\n\
         license = \"Apache-2.0 OR MIT\"\n\
         \n\
         [dependencies]\n\
         \n\
         [bin]\n\
         name = \"{name}\"\n\
         path = \"src/main.cb\"\n\
         \n\
         [[test]]\n\
         name = \"smoke\"\n\
         path = \"tests/smoke.cb\"\n",
    );
    if let Err(e) = std::fs::write(crate_dir.join("cobrust.toml"), cobrust_toml) {
        eprintln!("cobrust new: cannot write cobrust.toml: {e}");
        return exit_codes::INTERNAL_PANIC;
    }

    let main_cb = "fn main() -> i64:\n    print(\"hello, world\")\n    return 0\n";
    if let Err(e) = std::fs::write(crate_dir.join("src/main.cb"), main_cb) {
        eprintln!("cobrust new: cannot write src/main.cb: {e}");
        return exit_codes::INTERNAL_PANIC;
    }

    if let Err(e) = std::fs::create_dir_all(crate_dir.join("tests")) {
        eprintln!("cobrust new: cannot create tests/: {e}");
        return exit_codes::INTERNAL_PANIC;
    }
    let smoke_cb = "fn main() -> i64:\n    print(\"smoke ok\")\n    return 0\n";
    if let Err(e) = std::fs::write(crate_dir.join("tests/smoke.cb"), smoke_cb) {
        eprintln!("cobrust new: cannot write tests/smoke.cb: {e}");
        return exit_codes::INTERNAL_PANIC;
    }

    // T1.3: .gitignore so `git init` + `cobrust run` works without stray
    // build artifacts being staged accidentally.
    let gitignore = "/target/\n*.lock\n";
    if let Err(e) = std::fs::write(crate_dir.join(".gitignore"), gitignore) {
        eprintln!("cobrust new: cannot write .gitignore: {e}");
        return exit_codes::INTERNAL_PANIC;
    }

    // T1.3: README stub so the project is immediately shareable.
    let readme = format!(
        "# {name}\n\
         \n\
         A [Cobrust](https://github.com/Cobrust-lang/cobrust) package.\n\
         \n\
         ```bash\n\
         cobrust run src/main.cb\n\
         ```\n"
    );
    if let Err(e) = std::fs::write(crate_dir.join("README.md"), readme) {
        eprintln!("cobrust new: cannot write README.md: {e}");
        return exit_codes::INTERNAL_PANIC;
    }

    println!(
        "cobrust: created package `{name}` at {}\n\
         \n\
         Run your project:\n\
         \n\
         \tcd {name} && cobrust run src/main.cb",
        crate_dir.display()
    );
    exit_codes::SUCCESS
}

fn is_valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}
