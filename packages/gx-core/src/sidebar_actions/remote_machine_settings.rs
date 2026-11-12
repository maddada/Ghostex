//! `normalizeRemoteMachineSettings` (packages/shared/ghostex-settings/remote-machines.ts (deleted 2026-10-01)), the list
//! the sidebar store holds as `hud.settings.remoteMachines`.
//!
//! CDXC:RemoteMachines 2026-09-21 WHY:
//! A machine tab's Hide Machine posts the WHOLE machine list back as a settings patch, and the
//! list it posts is the NORMALIZED one the sidebar page holds, not the saved one: machines without
//! a name or a reachable host are dropped, a missing or duplicate id is renumbered, strings are
//! trimmed and cut, and fields outside the schema (a stray password included) are left behind.
//! The app merges the patch over the saved settings as it arrives, so the normalization IS part
//! of what gets written, and a port that posted the raw list would save a different document.
//! Every rule below is the TypeScript's, including JavaScript's `Number` for the port and UTF-16
//! lengths for the cuts.
//!
//! SEE-ALSO: packages/shared/ghostex-settings/remote-machines.ts (deleted 2026-10-01),
//! packages/gx-core/src/sidebar_actions/machine_disable.rs.

use serde_json::{Map, Value};

use crate::sidebar_view::text::{is_js_whitespace, js_trim, utf16_prefix};

/// `normalizeRemoteMachineSettings(candidate)`.
pub fn normalize_remote_machine_settings(candidate: Option<&Value>) -> Vec<Value> {
    let Some(items) = candidate.and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut seen_ids: Vec<String> = Vec::new();
    let mut normalized: Vec<Value> = Vec::new();
    for item in items {
        // `isRecord`: an object that is not an array.
        let Some(item) = item.as_object() else {
            continue;
        };
        let name = cut(&loose(item.get("name")), 80);
        let ssh_host = cut(&loose(item.get("sshHost")), 200);
        let easy_connect = easy_connect_address(item.get("easyConnectAddress"));
        let easy = item.get("transport").and_then(Value::as_str) == Some("easyConnect")
            && !easy_connect.is_empty();
        if name.is_empty() || (ssh_host.is_empty() && !easy) {
            continue;
        }
        let mut id = machine_id(item.get("id"));
        if id.as_ref().is_none_or(|id| seen_ids.contains(id)) {
            let mut next = format!("remote-{}", normalized.len() + 1);
            while seen_ids.contains(&next) {
                next = format!("remote-{}-{}", normalized.len() + 1, seen_ids.len() + 1);
            }
            id = Some(next);
        }
        let id = id.expect("assigned above");
        seen_ids.push(id.clone());
        let ssh_user = cut(&loose(item.get("sshUser")), 120);
        let ssh_identity_file = cut(&loose(item.get("sshIdentityFile")), 500);
        let ssh_port = ssh_port(item.get("sshPort"));
        let wsl_distribution = wsl_distribution(item.get("wslDistribution"));
        let mut machine = Map::new();
        machine.insert("id".to_string(), Value::String(id));
        machine.insert("name".to_string(), Value::String(name));
        // The pill's short name set in Settings (`machine_short_names.rs`); left out when unset.
        let short_name =
            crate::machine_short_names::clean_machine_short_name(&loose(item.get("shortName")));
        if !short_name.is_empty() {
            machine.insert("shortName".to_string(), Value::String(short_name));
        }
        // The computer's icon as this computer last read it from there (`computer_icons.rs`), kept
        // to draw while it is not connected; left out when unset or not one of the icons offered.
        if let Some(icon) = item
            .get("icon")
            .and_then(Value::as_str)
            .and_then(crate::computer_icons::computer_icon)
        {
            machine.insert("icon".to_string(), Value::String(icon.id.to_string()));
        }
        machine.insert("sshHost".to_string(), Value::String(ssh_host));
        if item.get("sshPasswordSaved") == Some(&Value::Bool(true)) {
            machine.insert("sshPasswordSaved".to_string(), Value::Bool(true));
        }
        // A machine that rides an SSH connection the user already signed in to (GitHub #496); off
        // unless set, like `agentAccess`, so only `true` is kept.
        if item.get("reuseSshConnection") == Some(&Value::Bool(true)) {
            machine.insert("reuseSshConnection".to_string(), Value::Bool(true));
        }
        if easy {
            machine.insert(
                "transport".to_string(),
                Value::String("easyConnect".to_string()),
            );
            machine.insert(
                "easyConnectAddress".to_string(),
                Value::String(easy_connect),
            );
        }
        if item.get("disabled") == Some(&Value::Bool(true)) {
            machine.insert("disabled".to_string(), Value::Bool(true));
        }
        // Agent access is on unless turned off, so only `false` is kept.
        if item.get("agentAccess") == Some(&Value::Bool(false)) {
            machine.insert("agentAccess".to_string(), Value::Bool(false));
        }
        if !ssh_identity_file.is_empty() {
            machine.insert(
                "sshIdentityFile".to_string(),
                Value::String(ssh_identity_file),
            );
        }
        if let Some(port) = ssh_port {
            machine.insert("sshPort".to_string(), Value::from(port));
        }
        if !ssh_user.is_empty() {
            machine.insert("sshUser".to_string(), Value::String(ssh_user));
        }
        if !wsl_distribution.is_empty() {
            machine.insert(
                "wslDistribution".to_string(),
                Value::String(wsl_distribution),
            );
        }
        normalized.push(Value::Object(machine));
    }
    normalized
}

/// `readLooseString`: a string, trimmed; anything else is empty.
fn loose(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .map(|value| js_trim(value).to_string())
        .unwrap_or_default()
}

/// `.slice(0, units)`. A surrogate pair cut in half is left out where JavaScript keeps a lone
/// surrogate, which a Rust string cannot hold (declared difference 41).
fn cut(value: &str, units: usize) -> String {
    utf16_prefix(value, units).to_string()
}

/// `normalizeRemoteMachineEasyConnectAddress`.
fn easy_connect_address(value: Option<&Value>) -> String {
    let address = cut(&loose(value), 4000);
    let accepted = address.chars().map(char::len_utf16).sum::<usize>() > 2
        && address.starts_with("tc")
        && !address.chars().any(is_js_whitespace);
    match accepted {
        true => address,
        false => String::new(),
    }
}

/// `normalizeRemoteMachineId`: `/^remote-[a-z0-9_-]+$/iu` over the trimmed, cut value.
fn machine_id(value: Option<&Value>) -> Option<String> {
    let id = cut(&loose(value), 80);
    // The `i` flag reaches the prefix too, so `REMOTE-1` is an id and is kept as written.
    let rest = id
        .get(..7)
        .filter(|prefix| prefix.eq_ignore_ascii_case("remote-"))
        .map(|_| &id[7..])?;
    (!rest.is_empty()
        && rest
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-')))
    .then_some(id)
}

/// `normalizeRemoteMachineWslDistribution`.
fn wsl_distribution(value: Option<&Value>) -> String {
    let distribution = cut(&loose(value), 120);
    let mut characters = distribution.chars();
    let valid = match characters.next() {
        Some(first) => {
            first.is_ascii_alphanumeric()
                && characters.all(|character| {
                    character.is_ascii_alphanumeric()
                        || matches!(character, '.' | '_' | '+' | '(' | ')' | ' ' | '-')
                })
        }
        None => false,
    };
    match valid {
        true => distribution,
        false => String::new(),
    }
}

/// `normalizeRemoteMachineSshPort`: JavaScript's `Number(input)`, kept when it is an integer from
/// 1 to 65535.
fn ssh_port(value: Option<&Value>) -> Option<u64> {
    let number = match value? {
        Value::Null => return None,
        Value::String(text) if text.is_empty() => return None,
        Value::Number(number) => number.as_f64()?,
        Value::Bool(flag) => f64::from(u8::from(*flag)),
        Value::String(text) => js_string_to_number(text),
        // `Number([])` is 0 and `Number([x])` is `Number(String(x))`; a longer array and every
        // object are NaN. All of them fail the range test below except a one-element array.
        Value::Array(items) => match items.as_slice() {
            [] => 0.0,
            [Value::Null] => 0.0,
            [Value::Number(number)] => number.as_f64()?,
            [Value::String(text)] => js_string_to_number(text),
            [Value::Bool(flag)] => js_string_to_number(if *flag { "true" } else { "false" }),
            _ => f64::NAN,
        },
        Value::Object(_) => f64::NAN,
    };
    (number.fract() == 0.0 && (1.0..=65535.0).contains(&number)).then_some(number as u64)
}

/// `Number(text)` for a string: whitespace trimmed, empty is 0, `0x`/`0o`/`0b` integers, and the
/// decimal literal grammar with an optional sign, `Infinity` included.
fn js_string_to_number(text: &str) -> f64 {
    let trimmed = js_trim(text);
    if trimmed.is_empty() {
        return 0.0;
    }
    for (prefix, radix) in [
        ("0x", 16),
        ("0X", 16),
        ("0o", 8),
        ("0O", 8),
        ("0b", 2),
        ("0B", 2),
    ] {
        if let Some(digits) = trimmed.strip_prefix(prefix) {
            if digits.is_empty() || !digits.chars().all(|digit| digit.is_digit(radix)) {
                return f64::NAN;
            }
            return digits.chars().fold(0.0, |total, digit| {
                total * f64::from(radix) + f64::from(digit.to_digit(radix).unwrap_or(0))
            });
        }
    }
    let unsigned = trimmed.trim_start_matches(['+', '-']);
    if trimmed.len() - unsigned.len() > 1 {
        return f64::NAN;
    }
    if unsigned == "Infinity" {
        return if trimmed.starts_with('-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    // The decimal grammar: digits, an optional fraction, an optional exponent, and at least one
    // digit before the exponent. Rust's parser also takes "inf" and "nan", which JavaScript does
    // not, so the characters are checked first.
    let mut mantissa_digits = 0;
    let mut chars = unsigned.chars().peekable();
    while chars.peek().is_some_and(char::is_ascii_digit) {
        chars.next();
        mantissa_digits += 1;
    }
    if chars.peek() == Some(&'.') {
        chars.next();
        while chars.peek().is_some_and(char::is_ascii_digit) {
            chars.next();
            mantissa_digits += 1;
        }
    }
    if mantissa_digits == 0 {
        return f64::NAN;
    }
    if matches!(chars.peek(), Some('e' | 'E')) {
        chars.next();
        if matches!(chars.peek(), Some('+' | '-')) {
            chars.next();
        }
        let mut exponent_digits = 0;
        while chars.peek().is_some_and(char::is_ascii_digit) {
            chars.next();
            exponent_digits += 1;
        }
        if exponent_digits == 0 {
            return f64::NAN;
        }
    }
    if chars.next().is_some() {
        return f64::NAN;
    }
    trimmed.parse::<f64>().unwrap_or(f64::NAN)
}
