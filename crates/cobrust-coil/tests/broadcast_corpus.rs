//! M7.1 broadcasting corpus — table-driven test for shape rules.
//!
//! Per ADR-0014 §2: every numpy-documented broadcast case + edge cases.
//! Cite https://numpy.org/doc/stable/user/basics.broadcasting.html.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::format_push_string)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::float_cmp)]
#![allow(clippy::type_complexity)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::manual_repeat_n)]
#![allow(clippy::if_not_else)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::print_stderr)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_errors_doc)]

use coil::{NumpyErrorKind, broadcast_shape};

// Canonical numpy-documented broadcasting cases (table-driven).
struct BCase {
    a: &'static [usize],
    b: &'static [usize],
    expected: Result<&'static [usize], NumpyErrorKind>,
    note: &'static str,
}

const CASES: &[BCase] = &[
    // Equal shapes: passthrough.
    BCase {
        a: &[3, 4],
        b: &[3, 4],
        expected: Ok(&[3, 4]),
        note: "equal 2D shapes",
    },
    BCase {
        a: &[2, 3, 4],
        b: &[2, 3, 4],
        expected: Ok(&[2, 3, 4]),
        note: "equal 3D shapes",
    },
    // Scalar broadcasting.
    BCase {
        a: &[],
        b: &[3, 4],
        expected: Ok(&[3, 4]),
        note: "scalar broadcasts to 2D",
    },
    BCase {
        a: &[3, 4],
        b: &[],
        expected: Ok(&[3, 4]),
        note: "2D ext scalar",
    },
    BCase {
        a: &[],
        b: &[],
        expected: Ok(&[]),
        note: "scalar with scalar",
    },
    // Size-1 axis expansion.
    BCase {
        a: &[3, 1],
        b: &[1, 4],
        expected: Ok(&[3, 4]),
        note: "outer product via size-1 expand",
    },
    BCase {
        a: &[1, 4],
        b: &[3, 1],
        expected: Ok(&[3, 4]),
        note: "outer product, swapped",
    },
    BCase {
        a: &[5, 1],
        b: &[5, 6],
        expected: Ok(&[5, 6]),
        note: "size-1 axis expanded",
    },
    // Left-pad with 1s when ranks differ.
    BCase {
        a: &[4],
        b: &[3, 4],
        expected: Ok(&[3, 4]),
        note: "shorter on left padded",
    },
    BCase {
        a: &[3, 4],
        b: &[4],
        expected: Ok(&[3, 4]),
        note: "shorter on right padded",
    },
    BCase {
        a: &[4, 5],
        b: &[3, 4, 5],
        expected: Ok(&[3, 4, 5]),
        note: "rank 2 to rank 3",
    },
    // Higher-rank examples from numpy docs.
    BCase {
        a: &[8, 1, 6, 1],
        b: &[7, 1, 5],
        expected: Ok(&[8, 7, 6, 5]),
        note: "numpy docs mixed-rank example",
    },
    BCase {
        a: &[5, 4],
        b: &[1],
        expected: Ok(&[5, 4]),
        note: "size-1 vector vs matrix",
    },
    BCase {
        a: &[15, 3, 5],
        b: &[15, 1, 5],
        expected: Ok(&[15, 3, 5]),
        note: "numpy docs broadcasting example",
    },
    BCase {
        a: &[15, 3, 5],
        b: &[3, 5],
        expected: Ok(&[15, 3, 5]),
        note: "numpy docs broadcasting example",
    },
    // Errors.
    BCase {
        a: &[3],
        b: &[4],
        expected: Err(NumpyErrorKind::BroadcastShapeMismatch),
        note: "mismatch 3 vs 4",
    },
    BCase {
        a: &[2, 1],
        b: &[8, 4, 3],
        expected: Err(NumpyErrorKind::BroadcastShapeMismatch),
        note: "rank mismatch propagates",
    },
    BCase {
        a: &[3, 4],
        b: &[5, 4],
        expected: Err(NumpyErrorKind::BroadcastShapeMismatch),
        note: "outer dim mismatch",
    },
    BCase {
        a: &[3, 7, 2],
        b: &[3, 5, 2],
        expected: Err(NumpyErrorKind::BroadcastShapeMismatch),
        note: "middle dim mismatch",
    },
    BCase {
        a: &[2, 3, 4, 5],
        b: &[3, 4, 6],
        expected: Err(NumpyErrorKind::BroadcastShapeMismatch),
        note: "high rank inner mismatch",
    },
];

#[test]
fn all_documented_cases() {
    let mut failed = vec![];
    for case in CASES {
        let actual = broadcast_shape(case.a, case.b);
        match (case.expected, &actual) {
            (Ok(expected_shape), Ok(actual_shape)) => {
                if expected_shape.to_vec() != *actual_shape {
                    failed.push(format!(
                        "FAIL [{}]: a={:?}, b={:?}, expected={:?}, got={:?}",
                        case.note, case.a, case.b, expected_shape, actual_shape
                    ));
                }
            }
            (Err(expected_kind), Err(e)) => {
                if expected_kind != e.kind {
                    failed.push(format!(
                        "FAIL [{}]: a={:?}, b={:?}, expected_kind={:?}, got_kind={:?}",
                        case.note, case.a, case.b, expected_kind, e.kind
                    ));
                }
            }
            (Ok(expected_shape), Err(e)) => {
                failed.push(format!(
                    "FAIL [{}]: a={:?}, b={:?}, expected_ok={:?}, got_err={:?}",
                    case.note, case.a, case.b, expected_shape, e
                ));
            }
            (Err(expected_kind), Ok(actual_shape)) => {
                failed.push(format!(
                    "FAIL [{}]: a={:?}, b={:?}, expected_err={:?}, got_ok={:?}",
                    case.note, case.a, case.b, expected_kind, actual_shape
                ));
            }
        }
    }
    assert!(
        failed.is_empty(),
        "broadcast_shape table-driven failures:\n{}",
        failed.join("\n")
    );
}

#[test]
fn case_count_meets_documentation_floor() {
    // Per ADR-0014: every numpy-documented case + edge cases.
    assert!(
        CASES.len() >= 20,
        "broadcasting corpus must cover >= 20 cases (got {})",
        CASES.len()
    );
}
