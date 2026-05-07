# SPDX-License-Identifier: BSD-3-Clause
#
# Placeholder for vendored upstream numpy ufunc tests. M7.1 uses the
# differential harness (corpus/numpy/M7.1/harness/h_ufunc.py) as the
# primary L2.behavior gate; this file is a stub for upstream-tests
# directory presence (constitution §4.2 + corpus directory layout per
# ADR-0014).
#
# The substantive M7.1 test suite lives at
# crates/cobrust-numpy/tests/ufunc_*.rs.

def test_placeholder():
    """Placeholder so pytest collects this directory."""
    assert True
