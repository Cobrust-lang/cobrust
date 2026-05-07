//! Deterministic-id computation per ADR-0007 §3.
//!
//! ```text
//! deterministic_id = blake3(
//!     source_sha256_hex                 || b"\n" ||
//!     toolchain_string                  || b"\n" ||
//!     sorted_join(router_decision_ids, "\n")
//! )
//! ```
//!
//! Identical inputs ⇒ identical id; this is the constitution §2.4
//! ("Deterministic build IDs") promise made concrete.

/// Compute a deterministic id over the inputs that drive a translation.
///
/// `source_sha256_hex` is the **full** 64-hex digest of the upstream
/// source archive. `toolchain_string` is the active rustc/cargo
/// version label (we use `env!("CARGO_VERSION")`-style at the caller).
/// `router_decision_ids` is unordered; we sort to keep the id stable
/// across iteration orders.
#[must_use]
pub fn deterministic_id(
    source_sha256_hex: &str,
    toolchain_string: &str,
    router_decision_ids: &[String],
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(source_sha256_hex.as_bytes());
    hasher.update(b"\n");
    hasher.update(toolchain_string.as_bytes());
    hasher.update(b"\n");
    let mut sorted: Vec<&str> = router_decision_ids.iter().map(String::as_str).collect();
    sorted.sort_unstable();
    let joined = sorted.join("\n");
    hasher.update(joined.as_bytes());
    format!("blake3:{}", hasher.finalize().to_hex())
}

/// Compute the SHA-256 of a file's bytes, returning the lowercase hex
/// digest.
///
/// # Errors
/// I/O errors bubble up.
pub fn sha256_file(path: &std::path::Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(hex::encode(hasher.finalize()))
}

/// SHA-256 of a string, lowercase hex.
#[must_use]
pub fn sha256_str(s: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn id_is_deterministic_across_calls() {
        let ids = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let id1 = deterministic_id("abc", "rustc 1.94.1", &ids);
        let id2 = deterministic_id("abc", "rustc 1.94.1", &ids);
        assert_eq!(id1, id2);
    }

    #[test]
    fn id_invariant_to_router_id_order() {
        let asc = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let desc = vec!["c".to_string(), "b".to_string(), "a".to_string()];
        assert_eq!(
            deterministic_id("abc", "rustc 1.94.1", &asc),
            deterministic_id("abc", "rustc 1.94.1", &desc)
        );
    }

    #[test]
    fn id_changes_with_source_sha() {
        let ids = vec!["a".to_string()];
        let id1 = deterministic_id("abc", "rustc", &ids);
        let id2 = deterministic_id("def", "rustc", &ids);
        assert_ne!(id1, id2);
    }

    #[test]
    fn id_changes_with_toolchain() {
        let ids = vec!["a".to_string()];
        let id1 = deterministic_id("abc", "rustc 1.94", &ids);
        let id2 = deterministic_id("abc", "rustc 1.95", &ids);
        assert_ne!(id1, id2);
    }

    #[test]
    fn id_changes_with_decision_ids() {
        let id1 = deterministic_id("abc", "r", &["a".into()]);
        let id2 = deterministic_id("abc", "r", &["b".into()]);
        assert_ne!(id1, id2);
    }

    #[test]
    fn id_format_is_blake3_prefixed() {
        let id = deterministic_id("abc", "r", &["a".into()]);
        assert!(id.starts_with("blake3:"));
        assert_eq!(id.len(), "blake3:".len() + 64);
    }

    #[test]
    fn sha256_str_is_deterministic_lowercase_hex() {
        let h = sha256_str("hello");
        assert_eq!(
            h,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        assert!(
            h.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase())
        );
    }
}
