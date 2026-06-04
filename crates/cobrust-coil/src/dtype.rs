// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: numpy 2.0.2
// oracle: cpython 3.11 (module: numpy)
// scope: M7.0 dtype tier per ADR-0013 §3 + M7.6 Complex variants per ADR-0021 §3.
// see PROVENANCE.toml for the full manifest.

//! Closed dtype enum for the M7.0 ndarray foundation (ADR-0013 §3) +
//! M7.6 Complex widening (ADR-0021 §3).
//!
//! Maps Python dtype strings (long form + type-char form) to the
//! Rust types that back `Array` variants. The enum was closed at five
//! variants for M7.0; M7.6 widens to seven by adding `Complex64` and
//! `Complex128` (per ADR-0021 §3). Further widening is an explicit
//! ADR bump (M7.7+ may add `Int8` / `UInt32` / `Float16` etc.).

use crate::error::{NumpyError, NumpyErrorKind};

/// Closed enum of dtypes the cobrust-coil crate supports.
///
/// Per ADR-0013 §3 the M7.0 closed set was `Int32 / Int64 / Float32 /
/// Float64 / Bool`. M7.6 (per ADR-0021 §3) widens to seven by adding
/// `Complex64` and `Complex128`. Any unrecognised Python dtype string
/// raises `NumpyError::UnsupportedDtype`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dtype {
    /// `numpy.int32` — exact 32-bit signed integer (`i4` shorthand).
    Int32,
    /// `numpy.int64` — exact 64-bit signed integer (`i8` shorthand).
    /// M7.0 default integer dtype on 64-bit hosts (matches upstream
    /// numpy's default integer dtype).
    Int64,
    /// `numpy.float32` — single-precision IEEE 754 (`f4` shorthand).
    Float32,
    /// `numpy.float64` — double-precision IEEE 754 (`f8` shorthand).
    /// M7.0 default float dtype (matches upstream numpy).
    Float64,
    /// `numpy.bool_` — 1-byte boolean (`?` shorthand). Note: this is
    /// numpy's 1-byte form, not Rust's bit-packed `bool` of `bitvec`.
    Bool,
    /// `numpy.complex64` — `(f32, f32)` IEEE 754 real + imaginary
    /// (`c8` shorthand). Per ADR-0021 §3. Storage is
    /// `num_complex::Complex<f32>`; total size 8 bytes.
    Complex64,
    /// `numpy.complex128` — `(f64, f64)` IEEE 754 real + imaginary
    /// (`c16` shorthand). Per ADR-0021 §3. Storage is
    /// `num_complex::Complex<f64>`; total size 16 bytes.
    Complex128,
}

impl Dtype {
    /// Parse a Python dtype string (long form or type-char form) to a
    /// `Dtype`.
    ///
    /// # Errors
    /// Returns `NumpyError::UnsupportedDtype` for any string outside
    /// the seven-variant closed set per ADR-0013 §3 + ADR-0021 §3.
    pub fn from_python_string(s: &str) -> Result<Self, NumpyError> {
        match s {
            "int32" | "i4" => Ok(Self::Int32),
            "int64" | "i8" => Ok(Self::Int64),
            "float32" | "f4" => Ok(Self::Float32),
            "float64" | "f8" => Ok(Self::Float64),
            "bool" | "?" => Ok(Self::Bool),
            "complex64" | "c8" => Ok(Self::Complex64),
            "complex128" | "c16" => Ok(Self::Complex128),
            other => Err(NumpyError {
                kind: NumpyErrorKind::UnsupportedDtype,
                message: format!(
                    "unsupported dtype string: {other:?}; cobrust-coil supports \
                     int32 / int64 / float32 / float64 / bool / complex64 / complex128 \
                     per ADR-0013 + ADR-0021"
                ),
            }),
        }
    }

    /// Map a `Dtype` back to its canonical Python long-form string.
    /// Used by `Array::repr` and `Array::to_json`.
    #[must_use]
    pub fn to_python_string(self) -> &'static str {
        match self {
            Self::Int32 => "int32",
            Self::Int64 => "int64",
            Self::Float32 => "float32",
            Self::Float64 => "float64",
            Self::Bool => "bool",
            Self::Complex64 => "complex64",
            Self::Complex128 => "complex128",
        }
    }

    /// Map a `Dtype` to its canonical Rust variant name (the same
    /// name the `Array` enum uses). Used by the `to_json` serialiser
    /// so the L0 differential gate can compare bytewise against the
    /// Python reference.
    #[must_use]
    pub fn to_rust_variant_name(self) -> &'static str {
        match self {
            Self::Int32 => "Int32",
            Self::Int64 => "Int64",
            Self::Float32 => "Float32",
            Self::Float64 => "Float64",
            Self::Bool => "Bool",
            Self::Complex64 => "Complex64",
            Self::Complex128 => "Complex128",
        }
    }

    /// Bytes per element for this dtype.
    ///
    /// Per ADR-0021 §3: `Complex64` is 8 bytes (two `f32`),
    /// `Complex128` is 16 bytes (two `f64`).
    #[must_use]
    pub fn item_size(self) -> usize {
        match self {
            Self::Int32 | Self::Float32 => 4,
            Self::Int64 | Self::Float64 | Self::Complex64 => 8,
            Self::Bool => 1,
            Self::Complex128 => 16,
        }
    }

    /// Returns `true` when this dtype is a complex variant
    /// (`Complex64 / Complex128`). Used by ufunc/linalg routing per
    /// ADR-0021 §5 + §6 to decide between real and complex code paths.
    #[must_use]
    pub fn is_complex(self) -> bool {
        matches!(self, Self::Complex64 | Self::Complex128)
    }

    /// Returns `true` when this dtype is float-or-complex (i.e. not
    /// integer or bool). Convenience for unary-math routing.
    #[must_use]
    pub fn is_floating(self) -> bool {
        matches!(
            self,
            Self::Float32 | Self::Float64 | Self::Complex64 | Self::Complex128
        )
    }
}

impl core::fmt::Display for Dtype {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.to_python_string())
    }
}

// ---- Stream W item 6: iinfo / finfo (numpy `_core/getlimits.py`) ------------
//
// `iinfo`: `@py_compat(strict)` — integer bounds are exact.
// `finfo`: `@py_compat(numerical(rtol=1e-15))` — IEEE 754 limit
// constants (`eps`, `tiny`, `max`, ...) are platform-stable but treated
// as numerical per constitution §2.4.
//
// numpy's `iinfo`/`finfo` accept the full named-scalar-type space
// (`int8`, `uint16`, `float16`, ...), not just the `Dtype` tier the
// `Array` tagged-union can hold. We therefore expose dedicated
// `IntKind` / `FloatKind` enums spanning numpy's standard integer /
// float types so `np.iinfo(np.int8)` works even though `Array` cannot
// store an `int8`.

/// numpy named integer scalar types accepted by `iinfo`.
///
/// Covers the signed (`int8/16/32/64`) and unsigned (`uint8/16/32/64`)
/// families per numpy 2.0.2 `getlimits.py`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IntKind {
    /// `numpy.int8` — 8-bit signed.
    Int8,
    /// `numpy.int16` — 16-bit signed.
    Int16,
    /// `numpy.int32` — 32-bit signed.
    Int32,
    /// `numpy.int64` — 64-bit signed.
    Int64,
    /// `numpy.uint8` — 8-bit unsigned.
    UInt8,
    /// `numpy.uint16` — 16-bit unsigned.
    UInt16,
    /// `numpy.uint32` — 32-bit unsigned.
    UInt32,
    /// `numpy.uint64` — 64-bit unsigned.
    UInt64,
}

/// numpy named float scalar types accepted by `finfo`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FloatKind {
    /// `numpy.float32` — single precision IEEE 754.
    Float32,
    /// `numpy.float64` — double precision IEEE 754.
    Float64,
}

/// `numpy.iinfo(int_type)`-equivalent. Machine limits for an integer
/// scalar type. Mirrors numpy's `iinfo` object attributes (`bits`,
/// `min`, `max`).
///
/// `@py_compat(strict)` — values are exact (e.g. `iinfo(int8).max ==
/// 127`).
///
/// All bounds are exposed as `i128` so the full `uint64` range
/// (`0 ..= 18446744073709551615`) and the `int64` minimum
/// (`-9223372036854775808`) both fit losslessly in one signed type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IntInfo {
    /// The integer kind this info describes.
    pub kind: IntKind,
    /// Bit width of the type (`8 / 16 / 32 / 64`).
    pub bits: u32,
    /// Smallest representable value (`0` for unsigned types).
    pub min: i128,
    /// Largest representable value.
    pub max: i128,
}

impl IntInfo {
    /// Construct the machine limits for `kind`. Total function — every
    /// `IntKind` has well-defined limits.
    #[must_use]
    pub fn new(kind: IntKind) -> Self {
        let (bits, min, max): (u32, i128, i128) = match kind {
            IntKind::Int8 => (8, i128::from(i8::MIN), i128::from(i8::MAX)),
            IntKind::Int16 => (16, i128::from(i16::MIN), i128::from(i16::MAX)),
            IntKind::Int32 => (32, i128::from(i32::MIN), i128::from(i32::MAX)),
            IntKind::Int64 => (64, i128::from(i64::MIN), i128::from(i64::MAX)),
            IntKind::UInt8 => (8, 0, i128::from(u8::MAX)),
            IntKind::UInt16 => (16, 0, i128::from(u16::MAX)),
            IntKind::UInt32 => (32, 0, i128::from(u32::MAX)),
            IntKind::UInt64 => (64, 0, i128::from(u64::MAX)),
        };
        Self {
            kind,
            bits,
            min,
            max,
        }
    }
}

/// `numpy.finfo(float_type)`-equivalent. Machine limits for a floating
/// scalar type. Mirrors numpy's `finfo` object attributes.
///
/// `@py_compat(numerical(rtol=1e-15))`. The fields match numpy 2.0.2
/// `finfo(float32)` / `finfo(float64)` exactly (captured from the
/// oracle).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FloatInfo {
    /// The float kind this info describes.
    pub kind: FloatKind,
    /// Total bit width (`32 / 64`).
    pub bits: u32,
    /// `eps`: difference between 1.0 and the next representable value.
    pub eps: f64,
    /// `epsneg`: difference between 1.0 and the next *smaller* value.
    pub epsneg: f64,
    /// `max`: largest finite representable value.
    pub max: f64,
    /// `min`: most negative finite representable value (`-max`).
    pub min: f64,
    /// `tiny` (a.k.a. `smallest_normal`): smallest positive normal.
    pub tiny: f64,
    /// `resolution`: approximate decimal resolution (`10 ** -precision`).
    pub resolution: f64,
    /// `nmant`: number of bits in the mantissa.
    pub nmant: u32,
    /// `nexp`: number of bits in the exponent.
    pub nexp: u32,
    /// `precision`: approximate number of decimal digits.
    pub precision: u32,
}

impl FloatInfo {
    /// Construct the machine limits for `kind`. Constants captured from
    /// numpy 2.0.2 `finfo`. Total function.
    #[must_use]
    pub fn new(kind: FloatKind) -> Self {
        match kind {
            FloatKind::Float32 => Self {
                kind,
                bits: 32,
                eps: f64::from(f32::EPSILON),
                epsneg: 5.960_464_477_539_063e-8,
                max: f64::from(f32::MAX),
                min: f64::from(-f32::MAX),
                tiny: f64::from(f32::MIN_POSITIVE),
                resolution: 1e-6,
                nmant: 23,
                nexp: 8,
                precision: 6,
            },
            FloatKind::Float64 => Self {
                kind,
                bits: 64,
                eps: f64::EPSILON,
                epsneg: 1.110_223_024_625_156_5e-16,
                max: f64::MAX,
                min: -f64::MAX,
                tiny: f64::MIN_POSITIVE,
                resolution: 1e-15,
                nmant: 52,
                nexp: 11,
                precision: 15,
            },
        }
    }
}

/// `numpy.iinfo(type)`-equivalent over the named-int-type string
/// (`"int8"`, `"uint32"`, ...). Convenience wrapper that parses the
/// numpy type name then builds the `IntInfo`.
///
/// # Errors
/// `NumpyError::UnsupportedDtype` if `name` is not a recognised integer
/// scalar-type name.
pub fn iinfo(name: &str) -> Result<IntInfo, NumpyError> {
    let kind = match name {
        "int8" | "i1" => IntKind::Int8,
        "int16" | "i2" => IntKind::Int16,
        "int32" | "i4" => IntKind::Int32,
        "int64" | "i8" => IntKind::Int64,
        "uint8" | "u1" => IntKind::UInt8,
        "uint16" | "u2" => IntKind::UInt16,
        "uint32" | "u4" => IntKind::UInt32,
        "uint64" | "u8" => IntKind::UInt64,
        other => {
            return Err(NumpyError {
                kind: NumpyErrorKind::UnsupportedDtype,
                message: format!(
                    "iinfo: {other:?} is not an integer type; expected one of \
                     int8 / int16 / int32 / int64 / uint8 / uint16 / uint32 / uint64"
                ),
            });
        }
    };
    Ok(IntInfo::new(kind))
}

/// `numpy.finfo(type)`-equivalent over the named-float-type string
/// (`"float32"`, `"float64"`).
///
/// # Errors
/// `NumpyError::UnsupportedDtype` if `name` is not a recognised float
/// scalar-type name.
pub fn finfo(name: &str) -> Result<FloatInfo, NumpyError> {
    let kind = match name {
        "float32" | "f4" => FloatKind::Float32,
        "float64" | "f8" => FloatKind::Float64,
        other => {
            return Err(NumpyError {
                kind: NumpyErrorKind::UnsupportedDtype,
                message: format!(
                    "finfo: {other:?} is not a float type; expected one of \
                     float32 / float64"
                ),
            });
        }
    };
    Ok(FloatInfo::new(kind))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)]
    #![allow(clippy::unwrap_used)]
    use super::*;

    // Oracle: numpy 2.0.2.

    #[test]
    fn iinfo_int8_bounds() {
        let ii = IntInfo::new(IntKind::Int8);
        assert_eq!(ii.bits, 8);
        assert_eq!(ii.min, -128);
        assert_eq!(ii.max, 127);
    }

    #[test]
    fn iinfo_full_family_matches_numpy() {
        // numpy 2.0.2 oracle values.
        assert_eq!(IntInfo::new(IntKind::Int16).max, 32767);
        assert_eq!(IntInfo::new(IntKind::Int16).min, -32768);
        assert_eq!(IntInfo::new(IntKind::Int32).max, 2_147_483_647);
        assert_eq!(IntInfo::new(IntKind::Int32).min, -2_147_483_648);
        assert_eq!(IntInfo::new(IntKind::Int64).max, 9_223_372_036_854_775_807);
        assert_eq!(IntInfo::new(IntKind::Int64).min, -9_223_372_036_854_775_808);
        assert_eq!(IntInfo::new(IntKind::UInt8).min, 0);
        assert_eq!(IntInfo::new(IntKind::UInt8).max, 255);
        assert_eq!(IntInfo::new(IntKind::UInt64).min, 0);
        assert_eq!(
            IntInfo::new(IntKind::UInt64).max,
            18_446_744_073_709_551_615_i128
        );
    }

    #[test]
    fn iinfo_string_lookup() {
        assert_eq!(iinfo("int32").unwrap().max, 2_147_483_647);
        assert_eq!(iinfo("uint8").unwrap().max, 255);
        assert_eq!(
            iinfo("foo").unwrap_err().kind,
            NumpyErrorKind::UnsupportedDtype
        );
        // float name rejected by iinfo
        assert_eq!(
            iinfo("float64").unwrap_err().kind,
            NumpyErrorKind::UnsupportedDtype
        );
    }

    #[test]
    fn finfo_float64_eps_and_limits() {
        // numpy: finfo(float64).eps == 2.220446049250313e-16
        let fi = FloatInfo::new(FloatKind::Float64);
        assert_eq!(fi.bits, 64);
        assert_eq!(fi.eps, 2.220_446_049_250_313e-16);
        assert_eq!(fi.max, 1.797_693_134_862_315_7e308);
        assert_eq!(fi.min, -1.797_693_134_862_315_7e308);
        assert_eq!(fi.tiny, 2.225_073_858_507_201_4e-308);
        assert_eq!(fi.epsneg, 1.110_223_024_625_156_5e-16);
        assert_eq!(fi.nmant, 52);
        assert_eq!(fi.nexp, 11);
        assert_eq!(fi.precision, 15);
        assert_eq!(fi.resolution, 1e-15);
    }

    #[test]
    fn finfo_float32_eps_and_limits() {
        // numpy: finfo(float32).eps == 1.1920929e-07
        let fi = FloatInfo::new(FloatKind::Float32);
        assert_eq!(fi.bits, 32);
        assert!((fi.eps - 1.192_092_9e-7).abs() < 1e-13);
        assert!((fi.max - 3.402_823_5e38).abs() / fi.max < 1e-6);
        assert!((fi.tiny - 1.175_494_4e-38).abs() / fi.tiny < 1e-6);
        assert_eq!(fi.nmant, 23);
        assert_eq!(fi.nexp, 8);
        assert_eq!(fi.precision, 6);
        assert_eq!(fi.resolution, 1e-6);
    }

    #[test]
    fn finfo_string_lookup() {
        assert!((finfo("float32").unwrap().eps - 1.192_092_9e-7).abs() < 1e-13);
        assert_eq!(finfo("float64").unwrap().bits, 64);
        assert_eq!(
            finfo("int32").unwrap_err().kind,
            NumpyErrorKind::UnsupportedDtype
        );
    }

    /// `from_python_string` parses the dtype names `coil.astype(a, dtype)`
    /// accepts at the C-ABI boundary — the long form AND the type-char
    /// shorthand. This is the parse layer the `__cobrust_coil_astype`
    /// shim relies on; an UNKNOWN string is an `Err` (the shim turns that
    /// into a clean `coil_panic`, NOT a silent wrong cast).
    #[test]
    fn from_python_string_parses_supported_dtypes() {
        assert_eq!(Dtype::from_python_string("int32").unwrap(), Dtype::Int32);
        assert_eq!(Dtype::from_python_string("int64").unwrap(), Dtype::Int64);
        assert_eq!(
            Dtype::from_python_string("float32").unwrap(),
            Dtype::Float32
        );
        assert_eq!(
            Dtype::from_python_string("float64").unwrap(),
            Dtype::Float64
        );
        assert_eq!(Dtype::from_python_string("bool").unwrap(), Dtype::Bool);
        // type-char shorthands.
        assert_eq!(Dtype::from_python_string("i8").unwrap(), Dtype::Int64);
        assert_eq!(Dtype::from_python_string("f8").unwrap(), Dtype::Float64);
        assert_eq!(Dtype::from_python_string("?").unwrap(), Dtype::Bool);
    }

    /// An unsupported / garbage dtype string is an `Err`, not a panic and
    /// not a silent fallback — the property the astype shim's
    /// unknown-dtype trap depends on.
    #[test]
    fn from_python_string_rejects_unknown() {
        assert_eq!(
            Dtype::from_python_string("garbage").unwrap_err().kind,
            NumpyErrorKind::UnsupportedDtype
        );
        assert_eq!(
            Dtype::from_python_string("int8").unwrap_err().kind,
            NumpyErrorKind::UnsupportedDtype,
            "int8 is NOT in coil's closed Array dtype set"
        );
        assert!(Dtype::from_python_string("").is_err());
    }
}
