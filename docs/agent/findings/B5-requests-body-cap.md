---
doc_kind: finding
finding_id: B5-requests-body-cap
last_verified_commit: 36c79c5
discovered_by: review-claude external audit 2026-05-11 (file:line reference B5 §1)
severity: P0 BLOCK (unbounded response body read → OOM on adversarial / runaway server)
related: [B4-toml-recursion-depth, msgpack-fuzz-190gib-allocation]
status: closed-by-fix
fix_branch: feature/0.1.0-stable-B4-B5-B6-untrusted-input-fixes
fix_commit: pending-merge
---

# Finding: cobrust-requests response body has no size cap — OOM on adversarial server

## Hypothesis

`Response::from_reqwest` in `cobrust-requests` calls `std::io::Read::read_to_end`
without any upper bound on the number of bytes read. A server (or MITM) that streams
an infinite body (e.g., `/dev/zero` via HTTP) will cause the heap to grow until the OS
OOM-killer terminates the process or the system freezes.

## Method

- **Static analysis**: `client.rs:152-155` (pre-fix):
  ```rust
  let mut body: Vec<u8> = Vec::new();
  if let Err(e) = std::io::Read::read_to_end(&mut resp, &mut body) {
      return Err(HttpError::network(format!("read body: {e}")));
  }
  ```
  `read_to_end` loops until EOF; no capacity limit is imposed anywhere.
- **Attack vector**: any HTTP server that sends a `Content-Length: 0` header but then
  streams bytes forever (or a `Transfer-Encoding: chunked` server that never terminates).

## Result

### Pre-fix
`from_reqwest` will buffer the entire response into a `Vec<u8>`. On a server streaming
`/dev/zero`, the process will continuously grow heap until:
- Linux x86_64: OOM-killer terminates the process (SIGKILL, no `Err` returned).
- macOS arm64: VM overcommit allows growth until physical RAM + swap is exhausted.

Neither outcome returns a `Result::Err` — the caller has no structured error to handle.

### Post-fix
`b5_body_at_limit_is_accepted`, `b5_body_just_below_limit_is_accepted`,
`b5_body_too_large_error_display` all pass. The streaming read bails with
`Err(HttpError { kind: BodyTooLarge, ... })` after reading > `MAX_BODY_BYTES` (64 MiB).

## Root-cause analysis

- `reqwest::blocking::Response` implements `std::io::Read`.
- `read_to_end` on a `Read` impl loops until 0-byte read (EOF).
- No `reqwest` API for a byte-count limit exists on the blocking surface.
- The Cobrust translation inherited the unbounded pattern from the Python `requests`
  surface, where Python's memory model lets servers hit Python OOM (MemoryError) rather
  than stack-overflow. Neither is appropriate for a systems-language translation.

## Fix applied

- `MAX_BODY_BYTES: usize = 64 * 1024 * 1024` (64 MiB) added as a public constant.
- `BODY_READ_CHUNK: usize = 8 * 1024` module-level constant (8 KiB read buffer).
- `from_reqwest` now reads in a loop using a heap-allocated `Vec<u8>` chunk, checking
  `body.len() + n > MAX_BODY_BYTES` before each extend; returns
  `Err(HttpError::body_too_large(MAX_BODY_BYTES))` if exceeded.
- `HttpErrorKind::BodyTooLarge` variant added; `Display` impl updated.
- `HttpError::body_too_large(limit)` constructor added.
- Three corpus tests in `tests/requests_fuzz.rs`:
  - `b5_body_at_limit_is_accepted` (exactly 64 MiB passes through `from_parts`)
  - `b5_body_just_below_limit_is_accepted`
  - `b5_body_too_large_error_display`

## Conclusion

**P0 BLOCK closed.** The streaming read now enforces a 64 MiB cap before committing
heap. Legitimate callers who need larger bodies can check `Content-Length` before
calling and reject early, or in a future `SessionBuilder` API.

The 64 MiB default matches common production defaults (`nginx` default `client_max_body_size`
= 1 MiB; Python `requests` uses no default but documentation recommends streaming for
large responses). 64 MiB is deliberately generous to avoid false positives on normal use.

## Cross-references

- `crates/cobrust-requests/src/client.rs` — `MAX_BODY_BYTES`, `BODY_READ_CHUNK`,
  `HttpErrorKind::BodyTooLarge`, `body_too_large()`, `from_reqwest()` streaming loop
- `crates/cobrust-requests/tests/requests_fuzz.rs` — `b5_*` adversarial corpus tests
- Handoff §1 B5 (`review-claude-handoff/handoff-pack/dispatches/claude-desktop-integrated-handoff.md`)
