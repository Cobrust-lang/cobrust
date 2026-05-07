"""L0 differential-test harness for parse_iso (synthetic-LLM mode).

Reads inputs from stdin (one per line) and prints the oracle output
as JSON. The Rust pipeline wires this up at L0; the file is committed
verbatim for reproducibility.
"""

import json
import sys

# When invoked as `python -m corpus.dateutil.harness.h_parse_iso`,
# the corpus package is importable from the repo root.
sys.path.insert(0, ".")  # repo-relative import for vendored corpus
from corpus.dateutil.upstream.parser_core import parse_iso, ParserError


def main():
    out = []
    for line in sys.stdin:
        src = line.rstrip("\n")
        try:
            tup = parse_iso(src)
            out.append({"ok": True, "tuple": list(tup)})
        except ParserError as e:
            out.append({"ok": False, "err": str(e)})
    json.dump(out, sys.stdout)


if __name__ == "__main__":
    main()
