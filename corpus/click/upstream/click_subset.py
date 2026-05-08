"""Vendored subset of click 8.1.7 — the M-batch L0 oracle.

Per ADR-0022 §1, this file holds only the surface that the M-batch
translation covers. It is **not** the upstream `click` package
verbatim — it is a hand-pinned oracle subset that the L0 differential
harness drives against the cobrust translation.

The full upstream surface (groups, choices, autocompletion, prompts)
is documented in `corpus/click/README.md` as out of scope.
"""

from __future__ import annotations

import sys


class ClickError(Exception):
    """Single error class — mirrors the cobrust ClickError taxonomy."""

    def __init__(self, kind: str, message: str):
        self.kind = kind
        self.message = message
        super().__init__(f"click {kind}: {message}")


class _Param:
    def __init__(self, name, ptype="str", default=None, required=False, help=None):
        self.name = name
        self.ptype = ptype
        self.default = default
        self.required = required
        self.help = help


class OptionSpec(_Param):
    def __init__(self, long_, short=None, ptype="str", default=None, help=None, required=False):
        stripped = long_.lstrip("-")
        super().__init__(stripped, ptype, default, required, help)
        self.short = short


class ArgumentSpec(_Param):
    def __init__(self, name, ptype="str", optional=False):
        super().__init__(name, ptype, None, not optional, None)


class Command:
    def __init__(self, name):
        self.name = name
        self.about = None
        self.options = []
        self.arguments = []

    def set_about(self, about):
        self.about = about
        return self

    def add_option(self, opt):
        self.options.append(opt)
        return self

    def add_argument(self, arg):
        self.arguments.append(arg)
        return self

    def run(self, argv):
        result = {"options": {}, "arguments": {}}
        # Skip program name.
        rest = list(argv[1:])
        i = 0
        opts_by_name = {o.name: o for o in self.options}
        opts_by_short = {o.short.lstrip("-"): o for o in self.options if o.short}
        positional = []
        seen_options = set()
        while i < len(rest):
            tok = rest[i]
            if tok.startswith("--"):
                name = tok[2:]
                if name not in opts_by_name:
                    raise ClickError("usage", f"unknown option {tok}")
                opt = opts_by_name[name]
                if opt.ptype == "bool":
                    result["options"][opt.name] = "true"
                    seen_options.add(opt.name)
                else:
                    if i + 1 >= len(rest):
                        raise ClickError("missing option", f"{tok} requires a value")
                    val = rest[i + 1]
                    if opt.ptype == "int":
                        try:
                            int(val)
                        except ValueError:
                            raise ClickError("invalid value", f"--{opt.name} requires an int")
                    if opt.ptype == "float":
                        try:
                            float(val)
                        except ValueError:
                            raise ClickError("invalid value", f"--{opt.name} requires a float")
                    result["options"][opt.name] = val
                    seen_options.add(opt.name)
                    i += 1
            elif tok.startswith("-") and len(tok) > 1:
                short_name = tok[1:]
                if short_name not in opts_by_short:
                    raise ClickError("usage", f"unknown short option {tok}")
                opt = opts_by_short[short_name]
                if opt.ptype == "bool":
                    result["options"][opt.name] = "true"
                else:
                    if i + 1 >= len(rest):
                        raise ClickError("missing option", f"{tok} requires a value")
                    result["options"][opt.name] = rest[i + 1]
                    i += 1
                seen_options.add(opt.name)
            else:
                positional.append(tok)
            i += 1
        # Defaults for missing options.
        for o in self.options:
            if o.name in seen_options:
                continue
            if o.ptype == "bool":
                result["options"][o.name] = "false"
            elif o.default is not None:
                result["options"][o.name] = str(o.default)
            elif o.required:
                raise ClickError("missing option", f"--{o.name} is required")
        # Positional args.
        for j, arg in enumerate(self.arguments):
            if j < len(positional):
                val = positional[j]
                if arg.ptype == "int":
                    try:
                        int(val)
                    except ValueError:
                        raise ClickError("invalid value", f"argument {arg.name} requires an int")
                if arg.ptype == "float":
                    try:
                        float(val)
                    except ValueError:
                        raise ClickError("invalid value", f"argument {arg.name} requires a float")
                result["arguments"][arg.name] = val
            elif arg.required:
                raise ClickError("missing argument", arg.name)
        return result


def command(name):
    return Command(name)


def option(long_, short=None, type="str", default=None, help=None, required=False):
    return OptionSpec(long_, short=short, ptype=type, default=default, help=help, required=required)


def argument(name, type="str", optional=False):
    return ArgumentSpec(name, ptype=type, optional=optional)
