"""L0 differential harness for `click.command(...).run(argv)`.

Drives the corpus oracle (`click_subset.command + option + argument`)
through a representative argv matrix and asserts that the Result
shape matches the cobrust-click Rust crate's expected output. The
Rust side is exercised at `crates/cobrust-click/tests/click_downstream.rs`.
"""

import sys
import os

HERE = os.path.dirname(os.path.abspath(__file__))
SHIPPED = os.path.join(HERE, "..", "upstream")
sys.path.insert(0, SHIPPED)

from click_subset import argument, command, option  # type: ignore


def echo():
    return (
        command("echo")
        .set_about("emit")
        .add_option(option("--message", short="-m", type="str", default="hello"))
        .add_option(option("--times", type="int", default="1"))
    )


def cli_with_args():
    return (
        command("cp")
        .add_argument(argument("src", type="str"))
        .add_argument(argument("dst", type="str"))
    )


def main():
    failures = []
    cases = [
        (echo(), ["echo"], {"message": "hello", "times": "1"}),
        (echo(), ["echo", "--message", "x"], {"message": "x", "times": "1"}),
        (echo(), ["echo", "-m", "y", "--times", "5"], {"message": "y", "times": "5"}),
        (cli_with_args(), ["cp", "/from", "/to"], {"src": "/from", "dst": "/to"}),
    ]
    for cmd, argv, expected in cases:
        result = cmd.run(argv)
        merged = {**result["options"], **result["arguments"]}
        for k, v in expected.items():
            if merged.get(k) != v:
                failures.append((argv, k, v, merged.get(k)))
    if failures:
        for f in failures:
            print("FAIL", f)
        sys.exit(1)
    print("PASS h_command")


if __name__ == "__main__":
    main()
