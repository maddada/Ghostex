//! JavaScript's number semantics, in one place for every family.
//!
//! Every label in the chat was built by a template string over a `number`, every stored stamp and
//! every wire count was read back with `JSON.parse`, and the renderers and the saved records still
//! expect exactly those results. None of that behaves the way the Rust spelling of the same line
//! does. The three rules that bite, and what they cost when a family writes the obvious Rust
//! instead:
//!
//! - **`String(value)`** is [`js_number`]. Rust's `Display` for `f64` never emits an exponent, so
//!   `1e21` printed as 22 digits where JavaScript writes `1e+21`, and `value as i64` saturated
//!   every integral double above `i64::MAX` to `9223372036854775807`.
//! - **Reading a JSON number** is [`js_number_of`]. `serde_json::Value::as_i64` answers `None` for
//!   a token written `3.0` or `3e0`, which JavaScript cannot tell apart from `3` at all, so a
//!   caller that fell back to a default took a branch the TypeScript never took.
//! - **`Math.round`** is [`js_round`]: halves go towards positive infinity, where `f64::round`
//!   sends them away from zero.
//!
//! Nothing here reads a clock, allocates an id, or touches the host. It is pure arithmetic and
//! formatting, shared by families a to f; `examples/js_number_check.rs` is its table test.

use serde_json::Value;

/// `Math.round`: halves go up, towards positive infinity.
///
/// `Math.round(-0.5)` is `-0` and `(-0.5f64).round()` is `-1`, which is the whole difference.
pub fn js_round(value: f64) -> f64 {
    if !value.is_finite() {
        return value;
    }
    let floor = value.floor();
    if value - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    }
}

/// `Number.prototype.toString()` with no radix, which is what `String(value)` and every template
/// string write.
///
/// The specification (ECMA-262, `Number::toString`) asks for the SHORTEST decimal digit string
/// that reads back as the same double, then places the point by the decimal exponent `n`:
///
/// | condition | form | example |
/// |---|---|---|
/// | `k <= n <= 21` | the digits, then `n - k` zeros | `1e20` -> `100000000000000000000` |
/// | `0 < n <= 21` | a point inside the digits | `123.456` |
/// | `-6 < n <= 0` | `0.`, then `-n` zeros, then the digits | `1e-6` -> `0.000001` |
/// | otherwise | exponent form, always signed | `1e21` -> `1e+21`, `5e-324` |
///
/// Rust's `{:e}` produces the same shortest digits (both sides implement "shortest that round
/// trips, closest on a tie") and is the one formatter that hands back the exponent as well, so the
/// digits are taken from there and only the placement is done here. `-0` prints `0`, which is what
/// `String(-0)` answers even though `-0 < 0` is false.
pub fn js_number(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }
    if value == 0.0 {
        return "0".to_string();
    }
    if value < 0.0 {
        return format!("-{}", js_number(-value));
    }
    if value.is_infinite() {
        return "Infinity".to_string();
    }
    let exponential = format!("{value:e}");
    let (mantissa, exponent) = exponential
        .split_once('e')
        .unwrap_or((exponential.as_str(), "0"));
    // `digits` is ASCII by construction, so every slice below is on a character boundary.
    let digits: String = mantissa
        .chars()
        .filter(|value| value.is_ascii_digit())
        .collect();
    let count = digits.len() as i32;
    let point = exponent.parse::<i32>().unwrap_or(0) + 1;
    if count <= point && point <= 21 {
        let mut out = digits;
        out.extend(std::iter::repeat_n('0', (point - count) as usize));
        return out;
    }
    if 0 < point && point <= 21 {
        let at = point as usize;
        return format!("{}.{}", &digits[..at], &digits[at..]);
    }
    if -6 < point && point <= 0 {
        return format!("0.{}{}", "0".repeat((-point) as usize), digits);
    }
    let exponent = point - 1;
    let sign = if exponent >= 0 { '+' } else { '-' };
    let magnitude = exponent.unsigned_abs();
    if count == 1 {
        format!("{digits}e{sign}{magnitude}")
    } else {
        format!("{}.{}e{sign}{magnitude}", &digits[..1], &digits[1..])
    }
}

/// A JSON number read the way JavaScript reads it: any numeric token is one double.
///
/// `Value::as_i64` and `as_u64` answer `None` for `3.0`, `3e0` and anything with a fraction, and
/// every caller that fell back to a default on that `None` took a branch `JSON.parse` cannot
/// produce: there are no integer tokens in JavaScript, only doubles. Use this, then apply the
/// same guard the TypeScript applied (`Number.isInteger`, `>= 0`, a length test).
pub fn js_number_of(value: Option<&Value>) -> Option<f64> {
    value?.as_f64()
}

/// A JSON number as a whole `i64`, refusing what JavaScript would not have used as one.
///
/// `Number.isSafeInteger` is the test, because a double above 2^53 has already lost the digits
/// that would tell two of them apart. The point is that the refusal is EXPLICIT: `value as i64`
/// saturates `1e300` to `i64::MAX` silently, which is how a stored `updatedAt` of `1e300` won
/// every freshness comparison for ever.
pub fn js_safe_integer(value: Option<&Value>) -> Option<i64> {
    let number = js_number_of(value)?;
    (number.fract() == 0.0 && number.abs() <= 9_007_199_254_740_991.0).then_some(number as i64)
}

/// `#[serde(with = "js_optional_number")]` for a stored or wire field that is a JavaScript number.
///
/// A `f64` field serde writes by itself comes out as `1700000000000.0`, which is not the record the
/// TypeScript brain saved and breaks the document's integers-stay-integers rule. This keeps the
/// double all the way through (no truncation, no `as i64` saturation) and still writes the bytes
/// `JSON.stringify` writes.
pub mod js_optional_number {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    /// Writes the number as `JSON.stringify` writes it, and `None` as `null`.
    pub fn serialize<S: Serializer>(value: &Option<f64>, serializer: S) -> Result<S::Ok, S::Error> {
        match value {
            Some(value) => super::js_number_value(*value).serialize(serializer),
            None => serializer.serialize_none(),
        }
    }

    /// Reads any JSON number, which is the only numeric form `JSON.parse` produces.
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<f64>, D::Error> {
        Option::<serde_json::Value>::deserialize(deserializer)
            .map(|value| super::js_number_of(value.as_ref()))
    }
}

/// A number as `JSON.stringify` writes it: an integral double has no decimal point, and `NaN` and
/// the infinities are `null`.
pub fn js_number_value(value: f64) -> Value {
    if !value.is_finite() {
        return Value::Null;
    }
    if value == value.trunc() && value.abs() <= 9_007_199_254_740_991.0 {
        return Value::from(value as i64);
    }
    serde_json::Number::from_f64(value).map_or(Value::Null, Value::Number)
}
