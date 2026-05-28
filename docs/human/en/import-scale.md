# `import scale` — encode and decode msgpack from Cobrust

> Status: ADR-0072 fourth-module proof. After `den` (SQLite, handle
> pattern), `nest` (TOML, pure value pattern), and `strike` (HTTP,
> handle + free entry), `scale` (msgpack, the rebrand of
> `msgpack-python`) is the FOURTH module wired through the same
> ecosystem-import chain — a value-pattern generalization on top of
> nest's, with a JSON-in / hex-out round trip.

## Example first

```python
import scale

fn main() -> i64:
    let packed: str = scale.dumps_str("{\"key\":\"value\"}")
    let back: str = scale.loads_str(packed)
    print(back)
    return 0
```

Build and run it:

```bash
cobrust build prog.cb -o prog
./prog
# {"key":"value"}
```

## What you get (fourth-module proof surface)

- **`scale.dumps_str(json_input) -> str`** — parse `json_input` as JSON,
  msgpack-encode the value tree, and return the bytes as lowercase HEX
  in a Cobrust `str`. Printable on stdout, easy to round-trip back.
- **`scale.loads_str(packed) -> str`** — decode the HEX into msgpack
  bytes, unpack the value tree, and return the canonical-JSON rendering
  of the value. Same shape as `nest.loads_str` (a printable string of
  canonical JSON).

That's the whole surface for now: the smallest-useful value-pattern
round trip that exercises the chain top-to-bottom for a fourth module.

## What happens when the input is malformed?

The msgpack surface **never panics, never returns null**. On any error
(invalid JSON for `dumps_str`, non-hex / corrupted bytes / non-msgpack
input for `loads_str`) the returned `str` is the **empty-string
sentinel**. The idiomatic check is:

```python
let packed = scale.dumps_str(maybe_bad_json)
if str_len(packed) == 0:
    print("input was not valid JSON")
else:
    print(packed)
```

This matches Cobrust's runtime convention for all the value-pattern
shims — fail cleanly, never panic across the C-ABI boundary.

## Why this design?

- **It proves the chain handles a SECOND value-pattern module.** `nest`
  was the first value module; `scale` is the second. The wiring reused
  every layer the den/nest/strike proofs landed without changes. Only
  data was added (manifest row + codegen extern row + recognizer arm),
  plus the new shim crate.
- **HEX rendering keeps the surface str→str.** Msgpack's natural ABI
  is raw bytes, but plumbing a `*mut u8` bytes ABI through the chain
  is its own redesign (Q5 of ADR-0072 covered strings, not bytes). The
  first-proof shape borrows nest's proven str→str path and renders the
  msgpack bytes as printable HEX. A raw bytes ABI is a tracked
  follow-up.
- **Only what you import is linked.** A program that imports `scale`
  links `libscale.a`; a program that doesn't, doesn't. No bloat.

## Today's limits

- Wrap your code in `fn main() -> i64:` (bare top-level statements are
  a separate, not-yet-finished part of the toolchain).
- The wire format is HEX-of-msgpack, not raw bytes. Downstream tooling
  that needs to write the packed bytes to a binary file unhexes them
  manually for now. A raw `bytes` surface is a tracked follow-up.
- The canonical-JSON rendering on `loads_str` matches `nest.loads_str`'s
  shape; a typed Cobrust value tree is a tracked follow-up.
- The error path is the empty-string sentinel; a typed
  `Result[str, ScaleError]` surface is a tracked follow-up.

These are tracked follow-ups, not dead ends — the wiring generalizes
to the rest of the ecosystem libraries from here.
