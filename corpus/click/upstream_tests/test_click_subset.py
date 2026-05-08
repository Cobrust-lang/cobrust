"""Upstream-derived test subset for the click M-batch oracle."""

import sys
import os

HERE = os.path.dirname(os.path.abspath(__file__))
SHIPPED = os.path.join(HERE, "..", "upstream")
sys.path.insert(0, SHIPPED)

from click_subset import ClickError, argument, command, option  # type: ignore


def echo():
    return (
        command("echo")
        .set_about("emit")
        .add_option(option("--message", short="-m", type="str", default="hello"))
        .add_option(option("--times", type="int", default="1"))
    )


def test_default_resolves_when_omitted():
    r = echo().run(["echo"])
    assert r["options"]["message"] == "hello"
    assert r["options"]["times"] == "1"


def test_explicit_overrides_default():
    r = echo().run(["echo", "--message", "world", "--times", "3"])
    assert r["options"]["message"] == "world"
    assert r["options"]["times"] == "3"


def test_short_option_recognised():
    r = echo().run(["echo", "-m", "yo"])
    assert r["options"]["message"] == "yo"


def test_int_validates():
    try:
        echo().run(["echo", "--times", "not-int"])
    except ClickError as e:
        assert e.kind == "invalid value"
        return
    raise AssertionError("must raise")


def test_unknown_option_is_usage():
    try:
        echo().run(["echo", "--mystery", "x"])
    except ClickError as e:
        assert e.kind == "usage"
        return
    raise AssertionError("must raise")


if __name__ == "__main__":
    failures = []
    for name, fn in list(globals().items()):
        if name.startswith("test_") and callable(fn):
            try:
                fn()
                print("PASS", name)
            except Exception as e:
                failures.append((name, str(e)))
                print("FAIL", name, e)
    sys.exit(1 if failures else 0)
