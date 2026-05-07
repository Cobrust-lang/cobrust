# SPDX-License-Identifier: MIT
# SPDX-FileCopyrightText: 2021 Taneli Hukkinen
#
# This file is a representative subset of `tomli/_parser.py` rewritten
# for clarity at the M4 scope window. It is the *source* the Cobrust
# translator consumes; the translated Rust lives at
# `crates/cobrust-tomli/src/parser.rs`.
#
# The full upstream is at https://github.com/hukkin/tomli (MIT). This
# subset is included verbatim under the same MIT license; see
# `corpus/tomli/UPSTREAM_LICENSE`.
"""Tomli loads(): subset rewrite for Cobrust M4."""


class TomliError(ValueError):
    """All parse failures bubble up as this single class."""


class _State:
    """Mutable position cursor over the input string.

    `src` is the input. `pos` is the current byte offset. The parser is
    a recursive-descent reader; helpers advance `pos` and may raise
    TomliError on malformed input.
    """

    def __init__(self, src):
        self.src = src
        self.pos = 0

    def eof(self):
        return self.pos >= len(self.src)

    def peek(self):
        if self.eof():
            return ""
        return self.src[self.pos]

    def advance(self):
        ch = self.peek()
        self.pos += 1
        return ch

    def expect(self, ch):
        if self.peek() != ch:
            raise TomliError("expected " + repr(ch) + " at pos " + str(self.pos))
        self.pos += 1


def _skip_whitespace(state):
    """Skip spaces, tabs, line comments (#...), and newlines."""
    while not state.eof():
        ch = state.peek()
        if ch == " " or ch == "\t" or ch == "\n" or ch == "\r":
            state.pos += 1
        elif ch == "#":
            while not state.eof() and state.peek() != "\n":
                state.pos += 1
        else:
            return


def _parse_basic_string(state):
    """Parse a "double-quoted" string. Supports \\n, \\t, \\\\, \\\""""
    state.expect('"')
    out = ""
    while not state.eof():
        ch = state.advance()
        if ch == '"':
            return out
        if ch == "\\":
            if state.eof():
                raise TomliError("unterminated escape")
            esc = state.advance()
            if esc == "n":
                out += "\n"
            elif esc == "t":
                out += "\t"
            elif esc == "\\":
                out += "\\"
            elif esc == '"':
                out += '"'
            elif esc == "r":
                out += "\r"
            else:
                raise TomliError("bad escape \\" + esc)
        else:
            out += ch
    raise TomliError("unterminated string")


def _parse_literal_string(state):
    """Parse a 'single-quoted' literal string. No escapes."""
    state.expect("'")
    out = ""
    while not state.eof():
        ch = state.advance()
        if ch == "'":
            return out
        out += ch
    raise TomliError("unterminated literal string")


def _parse_int(state):
    """Parse a decimal integer. Optional leading '-' or '+'."""
    start = state.pos
    if state.peek() == "-" or state.peek() == "+":
        state.pos += 1
    digits_start = state.pos
    while not state.eof() and state.peek() >= "0" and state.peek() <= "9":
        state.pos += 1
    if state.pos == digits_start:
        raise TomliError("expected digit at pos " + str(start))
    return int(state.src[start:state.pos])


def _parse_bool(state):
    """Parse `true` or `false`."""
    if state.src[state.pos:state.pos + 4] == "true":
        state.pos += 4
        return True
    if state.src[state.pos:state.pos + 5] == "false":
        state.pos += 5
        return False
    raise TomliError("expected bool at pos " + str(state.pos))


def _parse_value(state):
    """Parse one TOML value: string / int / bool / array / inline table."""
    _skip_whitespace(state)
    ch = state.peek()
    if ch == '"':
        return _parse_basic_string(state)
    if ch == "'":
        return _parse_literal_string(state)
    if ch == "[":
        return _parse_array(state)
    if ch == "{":
        return _parse_inline_table(state)
    if ch == "t" or ch == "f":
        return _parse_bool(state)
    if ch == "-" or ch == "+" or (ch >= "0" and ch <= "9"):
        return _parse_int(state)
    raise TomliError("unexpected character " + repr(ch) + " at pos " + str(state.pos))


def _parse_array(state):
    """Parse `[v1, v2, ...]`."""
    state.expect("[")
    out = []
    _skip_whitespace(state)
    if state.peek() == "]":
        state.pos += 1
        return out
    while True:
        out.append(_parse_value(state))
        _skip_whitespace(state)
        ch = state.peek()
        if ch == ",":
            state.pos += 1
            _skip_whitespace(state)
            if state.peek() == "]":
                state.pos += 1
                return out
            continue
        if ch == "]":
            state.pos += 1
            return out
        raise TomliError("expected , or ] at pos " + str(state.pos))


def _parse_key(state):
    """Parse a bare key: ASCII letters, digits, '_', '-'."""
    _skip_whitespace(state)
    start = state.pos
    while not state.eof():
        ch = state.peek()
        if (
            (ch >= "a" and ch <= "z")
            or (ch >= "A" and ch <= "Z")
            or (ch >= "0" and ch <= "9")
            or ch == "_"
            or ch == "-"
        ):
            state.pos += 1
        else:
            break
    if state.pos == start:
        raise TomliError("expected key at pos " + str(start))
    return state.src[start:state.pos]


def _parse_inline_table(state):
    """Parse `{ a = 1, b = 2 }`."""
    state.expect("{")
    out = {}
    _skip_whitespace(state)
    if state.peek() == "}":
        state.pos += 1
        return out
    while True:
        key = _parse_key(state)
        _skip_whitespace(state)
        state.expect("=")
        value = _parse_value(state)
        out[key] = value
        _skip_whitespace(state)
        ch = state.peek()
        if ch == ",":
            state.pos += 1
            _skip_whitespace(state)
            if state.peek() == "}":
                state.pos += 1
                return out
            continue
        if ch == "}":
            state.pos += 1
            return out
        raise TomliError("expected , or } at pos " + str(state.pos))


def _parse_table_header(state):
    """Parse `[section.subsection]` and return the dotted-path components."""
    state.expect("[")
    parts = []
    parts.append(_parse_key(state))
    while not state.eof() and state.peek() == ".":
        state.pos += 1
        parts.append(_parse_key(state))
    _skip_whitespace(state)
    state.expect("]")
    return parts


def _parse_kv(state, dest):
    """Parse `key = value` and write into `dest` dict."""
    key = _parse_key(state)
    _skip_whitespace(state)
    state.expect("=")
    value = _parse_value(state)
    dest[key] = value


def loads(src):
    """Parse a TOML string into a Python dict.

    Subset semantics: see corpus/tomli/README.md for the M4 scope window.
    """
    state = _State(src)
    root = {}
    current = root
    while True:
        _skip_whitespace(state)
        if state.eof():
            return root
        if state.peek() == "[":
            parts = _parse_table_header(state)
            cursor = root
            for part in parts:
                if part not in cursor:
                    cursor[part] = {}
                cursor = cursor[part]
            current = cursor
            continue
        _parse_kv(state, current)
