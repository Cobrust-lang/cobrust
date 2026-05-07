"""L0 differential-test harness for relativedelta_add."""

import json
import sys

sys.path.insert(0, ".")
from corpus.dateutil.upstream.relativedelta_core import relativedelta_add


def main():
    cases = json.load(sys.stdin)
    out = []
    for c in cases:
        tup = relativedelta_add(*c)
        out.append(list(tup))
    json.dump(out, sys.stdout)


if __name__ == "__main__":
    main()
