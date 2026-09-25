//! The table test for `crate::jsnum`: `String(value)`, `Math.round` and the two JSON readings.
//!
//! Every expectation below is what V8 prints for the same expression. The table was produced by
//! running the left column through `bun -e 'console.log(String(x))'` and pasting the answer, so a
//! change here is a change against the engine the saved chat data was written under, not against
//! a transcription of the specification.
//!
//! ```
//! cargo run --release --example js_number_check
//! ```
//!
//! Prints `differences 0` and exits zero when every row matches.

use ghostex_gx_chat_core::jsnum::{
    js_number, js_number_of, js_number_value, js_round, js_safe_integer,
};
use serde_json::json;

/// `String(value)`, one row per rule of `Number::toString`.
const NUMBERS: &[(f64, &str)] = &[
    // The plain cases every label uses.
    (0.0, "0"),
    (-0.0, "0"),
    (1.0, "1"),
    (-1.0, "-1"),
    (2.0, "2"),
    (42.0, "42"),
    (0.5, "0.5"),
    (-0.5, "-0.5"),
    (1.5, "1.5"),
    (123.456, "123.456"),
    (0.1, "0.1"),
    (1.0 / 3.0, "0.3333333333333333"),
    (100.0 / 3.0, "33.333333333333336"),
    // `k <= n <= 21`: digits then zeros, no exponent.
    (1e20, "100000000000000000000"),
    (1.2e20, "120000000000000000000"),
    (9_007_199_254_740_992.0, "9007199254740992"),
    // Above `i64::MAX`, which the old `value as i64` saturated to 9223372036854775807.
    (1e19, "10000000000000000000"),
    (1.8446744073709552e19, "18446744073709552000"),
    // `n > 21`: the exponent form, always with a sign.
    (1e21, "1e+21"),
    (1.5e21, "1.5e+21"),
    (1e300, "1e+300"),
    (1.7976931348623157e308, "1.7976931348623157e+308"),
    (-1e21, "-1e+21"),
    // `-6 < n <= 0`: the leading zeros stay.
    (1e-6, "0.000001"),
    (0.000001234, "0.000001234"),
    // `n <= -6`: the exponent form again.
    (1e-7, "1e-7"),
    (1.2e-7, "1.2e-7"),
    (5e-324, "5e-324"),
    (-1e-7, "-1e-7"),
    // The three that are not numbers.
    (f64::NAN, "NaN"),
    (f64::INFINITY, "Infinity"),
    (f64::NEG_INFINITY, "-Infinity"),
];

/// `Math.round`, where the halves go the other way from `f64::round`.
const ROUNDED: &[(f64, f64)] = &[
    (0.5, 1.0),
    (-0.5, 0.0),
    (-1.5, -1.0),
    (1.5, 2.0),
    (2.5, 3.0),
    (-2.5, -2.0),
    (0.4, 0.0),
    (-0.4, 0.0),
];

fn main() {
    let mut differences = 0;

    for (value, expected) in NUMBERS {
        let actual = js_number(*value);
        if actual != *expected {
            differences += 1;
            println!("js_number      expected {expected}, got {actual}");
        }
    }

    for (value, expected) in ROUNDED {
        let actual = js_round(*value);
        // `-0` and `0` are the same number and `Math.round(-0.5)` is `-0`; the string is what
        // matters, and `js_number` already answers "0" for both.
        if actual != *expected {
            differences += 1;
            println!("js_round       expected {expected}, got {actual}");
        }
    }

    // `JSON.parse` has no integer tokens: `3`, `3.0` and `3e0` are one double, and the old
    // `Value::as_i64` answered `None` for the last two.
    for token in ["3", "3.0", "3e0", "0.30000000000000004e1"] {
        let parsed = serde_json::from_str::<serde_json::Value>(token).unwrap_or(json!(null));
        let read = js_number_of(Some(&parsed));
        if read.map(|value| (value - 3.0).abs() < 1e-9) != Some(true) {
            differences += 1;
            println!("js_number_of   {token} did not read as 3");
        }
    }

    // `Number.isSafeInteger` is the guard, and the refusal is explicit rather than a saturation.
    for (token, expected) in [
        ("5", Some(5_i64)),
        ("5.0", Some(5)),
        ("-5", Some(-5)),
        ("5.5", None),
        ("1e300", None),
        ("9007199254740992", None),
        ("9007199254740991", Some(9_007_199_254_740_991)),
        ("\"5\"", None),
    ] {
        let parsed = serde_json::from_str::<serde_json::Value>(token).unwrap_or(json!(null));
        let read = js_safe_integer(Some(&parsed));
        if read != expected {
            differences += 1;
            println!("js_safe_integer {token} expected {expected:?}, got {read:?}");
        }
    }

    // `JSON.stringify(2)` is `2`, not `2.0`; `NaN` and the infinities are `null`.
    for (value, expected) in [
        (2.0, "2"),
        (-0.0, "0"),
        (2.5, "2.5"),
        (f64::NAN, "null"),
        (f64::INFINITY, "null"),
    ] {
        let actual = js_number_value(value).to_string();
        if actual != expected {
            differences += 1;
            println!("js_number_value expected {expected}, got {actual}");
        }
    }

    println!("rows            {}", NUMBERS.len() + ROUNDED.len());
    println!("differences     {differences}");
    std::process::exit(i32::from(differences > 0));
}
