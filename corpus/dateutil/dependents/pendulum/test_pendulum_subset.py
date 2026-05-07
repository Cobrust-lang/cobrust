"""Vendored subset of pendulum tests that touch dateutil.

Pinned upstream pendulum version 3.0.0. pendulum's primary use of
dateutil is the `tz` module (timezone resolution). The `tz` module is
**out of scope** for M5 + M6 — see ADR-0010 §5 ("Skipped { reason }").

This file exists so the L3 driver records "skipped (tz out of scope)"
rather than silently omitting pendulum from the dependent list. Per
ADR-0009 §5, every dependent must be either Pass / Skipped / Failed
with a reason in the manifest.
"""

import sys


def test_pendulum_tz_out_of_scope():
    """pendulum's tz integration depends on `dateutil.tz.gettz()`,
    which the cobrust M5/M6 scope window does not cover.

    Per ADR-0010 §5, this test is intentionally skip-flagged. The L3
    driver emits "SKIP <name>" (which the runner treats as a non-fail
    skip) rather than "PASS" or "FAIL".
    """
    print("SKIP", "pendulum_tz_out_of_scope: tz module deferred to M7+ per ADR-0010 §5")


if __name__ == "__main__":
    # Per ADR-0010 §5 / ADR-0009 §5 partial-coverage policy: emit a
    # SKIP line (which the L3 driver records as Skipped { reason }).
    # We exit 0 — a skip is not a failure.
    test_pendulum_tz_out_of_scope()
    sys.exit(0)
