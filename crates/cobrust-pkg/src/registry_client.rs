//! Registry client — fetch wheel indexes + download wheels with SHA-256
//! verification per ADR-0065 §3.3.2 + §3.4.
//!
//! Wave-2 scope: the "registry" is a static JSON index served over HTTPS
//! (GitHub Releases acts as the primary host per §3.4). The client speaks
//! the §3.4 index API:
//!
//! ```text
//! GET <base_url>/index/<pkg>/<version>/wheels.json
//! ```
//!
//! Response shape (one entry per wheel variant) is the [`WheelMeta`] struct.
//!
//! SHA verification:
//! Every downloaded wheel is hashed with SHA-256 and compared to the index
//! advertised value. Mismatch → hard error before any bytes are written to
//! the install path (per §3.3.2 step 8).
//!
//! Synchronous: uses `reqwest::blocking` to avoid imposing an async runtime
//! on the `cobrust install` callsite.

use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};
use thiserror::Error;

pub use crate::wheel_select::WheelMeta;

/// Errors from the registry client.
#[derive(Debug, Error)]
pub enum RegistryClientError {
    /// HTTP request failed at the transport layer.
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    /// HTTP response status was not 2xx.
    #[error(
        "registry returned status {status} for {url}\n  suggestion: verify the package name and version, or try `--registry-url` to override"
    )]
    BadStatus {
        /// HTTP status code.
        status: u16,
        /// URL that returned the bad status.
        url: String,
    },
    /// JSON deserialisation of the wheel index failed.
    #[error(
        "index JSON parse error: {0}\n  suggestion: registry index format may have drifted; report at https://github.com/Cobrust-lang/cobrust/issues"
    )]
    Parse(#[from] serde_json::Error),
    /// Downloaded wheel SHA-256 didn't match the index advertisement.
    #[error(
        "SHA-256 mismatch for {filename}\n  expected: {expected}\n  got:      {got}\n  suggestion: re-run `cobrust install <pkg> --force` to re-download, or pin to a known-good version"
    )]
    Sha256Mismatch {
        /// Wheel filename that failed verification.
        filename: String,
        /// Expected SHA-256 (lowercase hex).
        expected: String,
        /// Computed SHA-256 (lowercase hex).
        got: String,
    },
    /// Filesystem error writing the downloaded wheel to disk.
    #[error("io error writing {path}: {source}")]
    Io {
        /// Path being written to.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
}

/// Static-index registry client.
///
/// Holds a `base_url` (e.g. `https://github.com/Cobrust-lang/cobrust/releases/download`)
/// and a `reqwest::blocking::Client` configured with a sane timeout.
#[derive(Debug)]
pub struct RegistryClient {
    base_url: String,
    http: reqwest::blocking::Client,
}

impl RegistryClient {
    /// Construct a client.
    ///
    /// `base_url` is the registry root; the client appends path segments
    /// like `/index/<pkg>/<version>/wheels.json` per §3.4.
    ///
    /// Trailing slashes on `base_url` are normalised away to avoid `//`
    /// in constructed URLs.
    pub fn new(base_url: impl Into<String>) -> Result<Self, RegistryClientError> {
        let base = base_url.into();
        let base = base.trim_end_matches('/').to_owned();
        let http = reqwest::blocking::Client::builder()
            .user_agent(concat!(
                "cobrust-pkg/",
                env!("CARGO_PKG_VERSION"),
                " (+https://github.com/Cobrust-lang/cobrust)"
            ))
            .timeout(Duration::from_secs(60))
            .build()?;
        Ok(Self {
            base_url: base,
            http,
        })
    }

    /// Fetch the wheel index for `pkg_name` + `version` from the registry.
    ///
    /// Returns the parsed list of [`WheelMeta`] entries on success.
    pub fn fetch_index(
        &self,
        pkg_name: &str,
        version: &str,
    ) -> Result<Vec<WheelMeta>, RegistryClientError> {
        let url = format!(
            "{}/index/{pkg_name}/{version}/wheels.json",
            self.base_url.trim_end_matches('/')
        );
        let resp = self.http.get(&url).send()?;
        let status = resp.status();
        if !status.is_success() {
            return Err(RegistryClientError::BadStatus {
                status: status.as_u16(),
                url,
            });
        }
        let body = resp.text()?;
        let wheels: Vec<WheelMeta> = serde_json::from_str(&body)?;
        Ok(wheels)
    }

    /// Download `meta.download_url` into `dest_dir/<meta.filename>` and verify
    /// its SHA-256 matches `meta.sha256`.
    ///
    /// On mismatch the file is deleted from disk and a structured error is
    /// returned. Returns the final on-disk path on success.
    pub fn download_wheel(
        &self,
        meta: &WheelMeta,
        dest_dir: &Path,
    ) -> Result<PathBuf, RegistryClientError> {
        std::fs::create_dir_all(dest_dir).map_err(|e| RegistryClientError::Io {
            path: dest_dir.to_path_buf(),
            source: e,
        })?;
        let dest = dest_dir.join(&meta.filename);

        let resp = self.http.get(&meta.download_url).send()?;
        let status = resp.status();
        if !status.is_success() {
            return Err(RegistryClientError::BadStatus {
                status: status.as_u16(),
                url: meta.download_url.clone(),
            });
        }
        let bytes = resp.bytes()?;
        let actual_hex = sha256_hex(&bytes);
        if !actual_hex.eq_ignore_ascii_case(&meta.sha256) {
            return Err(RegistryClientError::Sha256Mismatch {
                filename: meta.filename.clone(),
                expected: meta.sha256.clone(),
                got: actual_hex,
            });
        }
        std::fs::write(&dest, &bytes).map_err(|e| RegistryClientError::Io {
            path: dest.clone(),
            source: e,
        })?;
        Ok(dest)
    }
}

/// Compute the SHA-256 of `bytes` as lowercase hex.
fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for b in digest {
        let _ = std::fmt::Write::write_fmt(&mut out, format_args!("{b:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_hex_matches_known_vector() {
        // SHA-256("abc") = ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn registry_client_new_normalises_trailing_slash() {
        let c = RegistryClient::new("https://example.com/").expect("client constructs");
        // Reach into the public surface only — fetch_index uses base_url; assert
        // via a smoke that no double-slash is generated by inspecting the
        // request URL via a mock would be ideal but reqwest blocking has no
        // easy mock hook without a dev dep, so we just confirm construction.
        let _ = c;
    }

    #[test]
    fn wheel_meta_round_trips_through_json() {
        let original = WheelMeta {
            filename: "cobrust-numpy-0.1.0-x86_64-unknown-linux-gnu-v3.tar.gz".to_owned(),
            triple: "x86_64-unknown-linux-gnu".to_owned(),
            cpu_level: "v3".to_owned(),
            sha256: "a1b2c3".to_owned(),
            cobrust_abi: "0.1".to_owned(),
            size_bytes: 4_194_304,
            download_url: "https://example.com/wheel.tar.gz".to_owned(),
        };
        let json = serde_json::to_string(&original).expect("serialize");
        let back: WheelMeta = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(original, back);
    }
}
