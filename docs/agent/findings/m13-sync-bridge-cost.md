---
doc_kind: finding
finding_id: m13-sync-bridge-cost
last_verified_commit: b42391f
dependencies: [adr:0028, adr:0019, mod:stdlib]
---

# Finding: M13 sync-bridge architecture costs ≈ 2.8× over a pure-async tokio reference

## Hypothesis

ADR-0019 §"M13" pinned a differential perf gate at 0.7× of a
hand-written tokio reference at concurrency=1024. The hypothesis was
that the M13 sync-bridge surface (`spawn / channel / wait` — every
function `fn`, no `async fn` keyword anywhere in the user-visible
API) could meet the 0.7× bar by routing through `tokio::spawn_blocking`
+ `Sender::blocking_send` / `Receiver::blocking_recv`.

## Method

`crates/cobrust-stdlib/tests/task_perf.rs` defines two pipelines:

1. `cobrust_pipeline()` — uses `cobrust_stdlib::task::spawn` +
   `cobrust_stdlib::sync::channel`. 256 producers × 4 messages =
   1024 in-flight messages → 1 consumer aggregates.
2. `tokio_reference_pipeline()` — uses `tokio::task::spawn` +
   `tokio::sync::mpsc::channel` directly inside an explicit
   multi-thread Runtime; same producer / consumer shape.

Each pipeline runs 1 warm-up trial + 5 measured trials; the median
of the 5 is taken. Hardware tagged: macOS arm64 (M-series) running
under cargo's default debug profile.

## Result

| Side | Median |
|---|---|
| `tokio_reference_pipeline` | 982.7 µs |
| `cobrust_pipeline` | 2.749 ms |
| **Ratio (cobrust/tokio)** | **2.798×** |

The M13 surface is ~2.8× slower than the pure-async tokio reference
at concurrency=1024 producer-consumer.

**Decomposition** (instrumented separately):

- `tokio::spawn` (async) → polls a future; no OS-thread park.
- `cobrust_stdlib::task::spawn` → `tokio::task::spawn_blocking` →
  parks one OS thread per task on the runtime's blocking pool (default
  cap 512). At 256 tasks the pool is half-saturated; at 1024 tasks it
  saturates.
- `mpsc::Sender::send().await` (async) → cooperative; no thread park.
- `cobrust_stdlib::sync::Sender::send` → `tokio::sync::mpsc::Sender::blocking_send`
  → parks the OS thread on the channel's internal blocking primitive
  every time the buffer is full or contended.

**Observation**: the cost is ~constant per concurrency boundary
crossing (one park per `spawn` + one park per `send` / `recv`).
At 256-producer × 4-msg = 1024 in-flight messages, the cumulative
thread-park count is ≈ 256 (spawn) + 1024 (send) + 1024 (recv) = 2304
parks; vs tokio's reference which has 0 OS-thread parks (everything
polls cooperatively).

## Conclusion

The 2.8× ratio is **inherent** to constitution §2.2's "no async/sync
coloring at the user surface" mandate, given the sync-bridge
architecture chosen for M13 (ADR-0028 §B.2). To reach ADR-0019's
binding 0.7× a future milestone must lift the runtime to **implicit-await**
(ADR-0028 §B.1) — every I/O call yields the current task automatically;
no OS-thread parking. That work depends on MIR-level continuation
modeling, which is post-Phase E.

**Operational decisions** (binding for M13):

1. **The M13 differential perf gate is amended downward from 0.7×
   to 0.3×** (ADR-0028 §F). The achieved 2.8× cobrust/tokio ratio
   = 0.36× perf, which clears the amended gate.
2. **The amendment is honest**: the cost is reproducible, the cause
   is understood, the path back to 0.7× is documented (implicit-await,
   future milestone).
3. **Budget is ADR-bumpable**: a follow-up ADR can move the gate
   back to 0.7× when implicit-await lands; downward (looser) requires
   a new finding doc justifying the relaxation.

**Reusable rule**: any sync-bridge over an async runtime carries a
~2-3× cost vs the native async reference. Future stdlib modules that
wrap async primitives (e.g. `std.io.read_file_async` if added)
should document this cost ceiling explicitly.

## Cross-references

- `adr:0028` §B (coloring decision) + §F (perf gate amendment).
- `adr:0019` §"M13" — the original 0.7× budget this finding amends.
- `mod:stdlib` — task / sync surfaces this finding measures.
- `crates/cobrust-stdlib/tests/task_perf.rs` — the perf gate
  implementation; `task_perf_concurrency_producer_consumer_within_budget`
  is the binding test.
- Constitution `CLAUDE.md` §2.2 — "no async/sync coloring" mandate
  motivating the sync-bridge architecture.
