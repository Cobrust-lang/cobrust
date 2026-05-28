# `import nest` — parse TOML from Cobrust

> Status: ADR-0072 second-module proof. After `den` showed the
> ecosystem-import chain end-to-end, `nest` (TOML, the rebrand of
> `tomli`) is the simplest cheap generalization onto that same chain —
> a pure string-in-string-out function with no handles to manage.

## Example first

```python
import nest

fn main() -> i64:
    let toml_input: str = "title = \"hello\"\n[server]\nport = 8080\n"
    let canonical_json: str = nest.loads_str(toml_input)
    print(canonical_json)
    return 0
```

Build and run it:

```bash
cobrust build prog.cb -o prog
./prog
# {"server":{"port":8080},"title":"hello"}
```

## What you get (second-module proof surface)

- **`nest.loads_str(toml) -> str`** — parse the TOML source in `toml`
  and return its canonical-JSON rendering. Just a string in, just a
  string out. On a parse error the returned string is a JSON sentinel
  of shape `{"err": "<message>"}` (a typed `Result` surface is a
  follow-up).

That's the whole surface for now: the smallest-useful free function
that exercises the chain top-to-bottom for a second module.

## Why this design?

- **It proves the chain isn't den-specific.** The wiring for `nest`
  reused every layer the `den` first proof landed — manifest,
  type-check, MIR retarget, codegen extern, the link locator — without
  changes. Only the manifest row, a codegen extern declaration, the
  new C-ABI shim, and a one-line addition to the symbol-prefix
  recognizer were needed.
- **No handles means no escape rules.** TOML→JSON canonicalization is a
  pure value transformation; there's nothing to keep alive across
  scopes, nothing to free explicitly. The compiler's existing string
  drop schedule already does the right thing.
- **Only what you import is linked.** A program that imports `nest`
  links `libnest.a`; a program that doesn't, doesn't. No bloat.

## Today's limits

- Wrap your code in `fn main() -> i64:` (bare top-level statements are
  a separate, not-yet-finished part of the toolchain).
- The returned string is a JSON-canonical rendering, not yet a typed
  Cobrust value tree — for now, downstream code parses it back with
  any JSON consumer (this matches today's `den.fetchall()` rendering
  shape).
- Parse-error reporting is a JSON-string sentinel (`{"err":"…"}`); a
  typed `Result[str, Error]` surface is a tracked follow-up.

These are tracked follow-ups, not dead ends — the wiring generalizes
to the rest of the ecosystem libraries from here.
