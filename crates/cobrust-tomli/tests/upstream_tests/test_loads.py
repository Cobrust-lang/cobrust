# SPDX-License-Identifier: MIT
# Upstream-style pytest cases for the M4-scope loads() function.
#
# These cases run against:
#  1. The vendored Python source (sanity check it parses what we say it parses).
#  2. CPython's `tomllib` (the L3 oracle).
#  3. The translated Cobrust crate (`cobrust-tomli`) via its Rust API.
#
# The Rust harness (tests/tomli_downstream.rs in the cobrust-tomli
# crate) loads this file by name and replays each case against all
# three implementations, asserting equality.

CASES = [
    # name, input, expected dict
    ("empty",                  "",                                        {}),
    ("single_int",             "x = 1\n",                                  {"x": 1}),
    ("negative_int",           "x = -42\n",                                {"x": -42}),
    ("plus_int",               "x = +7\n",                                 {"x": 7}),
    ("two_keys",               "a = 1\nb = 2\n",                           {"a": 1, "b": 2}),
    ("bool_true",              "k = true\n",                               {"k": True}),
    ("bool_false",             "k = false\n",                              {"k": False}),
    ("basic_string",           'k = "hi"\n',                               {"k": "hi"}),
    ("basic_string_escape",    'k = "a\\nb"\n',                            {"k": "a\nb"}),
    ("literal_string",         "k = 'hi'\n",                               {"k": "hi"}),
    ("empty_array",            "k = []\n",                                 {"k": []}),
    ("int_array",              "k = [1, 2, 3]\n",                          {"k": [1, 2, 3]}),
    ("trailing_comma_array",   "k = [1, 2,]\n",                            {"k": [1, 2]}),
    ("inline_table",           "k = { a = 1, b = 2 }\n",                   {"k": {"a": 1, "b": 2}}),
    ("table_header",           "[s]\nx = 1\n",                             {"s": {"x": 1}}),
    ("nested_table_header",    "[a.b]\nx = 1\n",                           {"a": {"b": {"x": 1}}}),
    ("multiple_tables",        "[a]\nx = 1\n[b]\ny = 2\n",                 {"a": {"x": 1}, "b": {"y": 2}}),
    ("comment_line",           "# comment\nx = 1\n",                       {"x": 1}),
    ("inline_comment",         "x = 1 # tail comment\n",                   {"x": 1}),
    ("dashed_key",             "my-key = 1\n",                             {"my-key": 1}),
    ("underscore_key",         "my_key = 1\n",                             {"my_key": 1}),
    ("string_with_escape",     'k = "tab\\there"\n',                       {"k": "tab\there"}),
    ("array_of_strings",       'k = ["a", "b"]\n',                         {"k": ["a", "b"]}),
    ("array_of_bools",         "k = [true, false]\n",                      {"k": [True, False]}),
    ("nested_inline_table",    "k = { a = { b = 1 } }\n",                  {"k": {"a": {"b": 1}}}),
    ("whitespace_around_eq",   "x   =    1\n",                             {"x": 1}),
    ("crlf_line_endings",      "x = 1\r\ny = 2\r\n",                       {"x": 1, "y": 2}),
]


# ---- Negative cases ---------------------------------------------------------
# These must raise tomli/Cobrust errors (CPython tomllib also rejects them).

NEGATIVE_CASES = [
    ("unterminated_string",    'x = "abc\n'),
    ("bad_escape",             'x = "\\q"\n'),
    ("trailing_dot",           "[a.]\n"),
    ("unclosed_array",         "x = [1, 2\n"),
    ("bare_value",             "= 1\n"),
]
