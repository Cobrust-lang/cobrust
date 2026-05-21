//! `cobrust-registry` — index-generation side of the Cobrust wheel registry.
//!
//! ADR-0065 §7.3: this crate provides the **generation** counterpart to
//! `cobrust-pkg`'s `registry_client` consumer. Given a tagged GitHub Release,
//! it fetches the release's asset list, parses wheel filenames matching the
//! `cobrust-<pkg>-<version>-<triple>-<cpu_level>.tar.gz` convention, and
//! emits a canonical `wheels.json` index per §3.4.
//!
//! ## Public API
//!
//! ```text
//! generator::fetch_release_assets(repo, version)  → Vec<ReleaseAsset>
//! generator::parse_wheel_asset(filename)           → Option<(triple, cpu_level)>
//! generator::generate_index(pkg, version, assets)  → Index
//! generator::write_index_json(index, path)         → Result<(), Error>
//! ```
//!
//! ## Wire format
//!
//! `wheels.json` shape (one entry per wheel variant, §3.4):
//!
//! ```json
//! {
//!   "name": "numpy-cb",
//!   "version": "0.1.0",
//!   "wheels": [
//!     {
//!       "triple": "x86_64-unknown-linux-gnu",
//!       "cpu_level": "v3",
//!       "sha256": "a1b2c3...",
//!       "url": "https://github.com/Cobrust-lang/cobrust/releases/download/v0.1.0/...",
//!       "size": 4194304
//!     }
//!   ]
//! }
//! ```

pub mod generator;

pub use generator::{Error, Index, ReleaseAsset, WheelEntry, generate_index, write_index_json};
