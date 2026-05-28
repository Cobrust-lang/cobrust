# `import hood` — click-style CLI commands from Cobrust (callback marshalling second proof)

> Status: ADR-0073 second proof. After `pit` (Flask) showed the
> callback chain crossing `fn(Request) -> Response` through the C ABI,
> `hood` (the cobra-rebrand of Python's `click`) is the SEVENTH
> ecosystem module — and the SECOND to cross a callback. The shape
> here is `fn() -> i64` (no positional args; i64 return is the user's
> exit-code intent), proving the chain generalizes off the proven
> trampoline pattern.

## Example first

```python
import hood

fn handle_greet() -> i64:
    print("hello from hood")
    return 0

fn main() -> i64:
    let cmd = hood.Command("greet", "Print a friendly greeting")
    let _ = cmd.handler(handle_greet)
    let _ = cmd.run()
    return 0
```

Build and run:

```bash
cobrust build prog.cb -o prog
./prog
# hello from hood
```

## What you get (first proof surface)

- **`hood.Command(name, help) -> Command`** — construct a click-style
  command with `name` (the CLI verb) and `help` (the about-text shown
  in help output). Both are bare strings.
- **`Command.handler(fn)`** — bind a top-level `fn` as the command's
  callback. The handler MUST be a top-level
  `fn handler() -> i64: …`. Returns `i64` (zero sentinel); the
  canonical form is `let _ = cmd.handler(...)`.
- **`Command.run() -> i64`** — invoke the bound callback. Returns
  `0` when the callback ran; `-1` if no handler was registered.
  `fn main() -> i64: return cmd.run()` is the natural shape for a
  hood-only program.

## Why this design?

- **One callback ABI shape across pit + hood**: every handler crosses
  as `extern "C" fn(*mut u8) -> *mut u8` (ADR-0073 §5.1). hood's
  no-arg / i64-return shape uses a null pointer placeholder and
  discards the returned pointer — the handler's side effect (e.g.
  `print(...)`) IS the user's intent for the first proof.
- **Compile-time-catch callback shape (§2.5 binding)**: same gate as
  pit — rejects everything but a top-level `fn` NAME. No lambdas, no
  fn-typed locals, no call-results, no parenthesized forms. The
  diagnostic prints the fix the LLM should apply (Direction B).
- **Abort-on-panic across the C boundary**: the trampoline wraps the
  callback in `catch_unwind` and aborts on panic, with a structured
  stderr message (ADR-0073 §3 Q5).
- **Drop discipline (§2 D6)**: the `Command` handle is `.cb`-owned;
  scope-exit runs `__cobrust_hood_command_drop`. The boxed click
  builder + the registered closure are freed together exactly once.

## Today's limits

- **No closures / no lambdas as handlers**: must be a top-level `fn`.
- **No decorator sugar**: `@cmd.handler` is ADR-0074 (next sprint;
  the click-decorator-stack desugar tracks this).
- **No clap arg / option wiring through `.cb`**: today's
  `Command.handler(fn)` registers a single bare callback. The clap-side
  option / argument builders in `cobrust-hood/src/decorators.rs` work
  from Rust but aren't surfaced through the `.cb` ecosystem manifest
  yet — a paired follow-up wires `cmd.option(name, help)` +
  `cmd.argument(name)` once the manifest grows multi-method handle
  builders.
- **`Command.run()` doesn't surface the handler's i64 return**: the
  click-style `fn() -> i64` callback's return-value is currently
  discarded at the trampoline boundary; `cmd.run()` returns `0` on
  success / `-1` if no handler is bound. A future shape passes the
  handler's i64 through a structured-return ABI extension.
