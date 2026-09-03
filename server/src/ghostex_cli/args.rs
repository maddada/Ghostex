use std::collections::{BTreeMap, HashSet};

use serde_json::Value;

/*
CDXC:Cli 2026-07-13:
Faithful port of the Node CLI's parseArgs/multiValueFlag. Flag values keep the
JS shape: a flag with no following value (or followed by another --flag) is
boolean true; otherwise it captures the next argument as a string. Short flag
clusters (-ab) set each letter to true. `--` ends flag parsing.
*/

#[derive(Clone, Debug, PartialEq)]
pub enum FlagValue {
    Bool(bool),
    Text(String),
}

impl FlagValue {
    pub fn as_json(&self) -> Value {
        match self {
            FlagValue::Bool(value) => Value::Bool(*value),
            FlagValue::Text(value) => Value::String(value.clone()),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Flags(pub BTreeMap<String, FlagValue>);

impl Flags {
    pub fn contains(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }

    /// JS truthiness of `flags.key`: absent → false; boolean true → true;
    /// string → true unless empty.
    pub fn truthy(&self, key: &str) -> bool {
        match self.0.get(key) {
            None => false,
            Some(FlagValue::Bool(value)) => *value,
            Some(FlagValue::Text(value)) => !value.is_empty(),
        }
    }

    /// String(flags.key) when present, regardless of type.
    pub fn text(&self, key: &str) -> Option<String> {
        self.0.get(key).map(|value| match value {
            FlagValue::Bool(flag) => flag.to_string(),
            FlagValue::Text(text) => text.clone(),
        })
    }

    /// Only a captured string value (not boolean flags).
    pub fn string_value(&self, key: &str) -> Option<&str> {
        match self.0.get(key) {
            Some(FlagValue::Text(text)) => Some(text.as_str()),
            _ => None,
        }
    }

    /// Number(flags.key ?? default): JS Number coercion, NaN → None.
    pub fn number(&self, key: &str) -> Option<f64> {
        match self.0.get(key)? {
            FlagValue::Bool(true) => Some(1.0),
            FlagValue::Bool(false) => Some(0.0),
            FlagValue::Text(text) => js_number(text),
        }
    }

    pub fn insert_bool(&mut self, key: &str, value: bool) {
        self.0.insert(key.to_string(), FlagValue::Bool(value));
    }

    pub fn insert_text(&mut self, key: &str, value: &str) {
        self.0
            .insert(key.to_string(), FlagValue::Text(value.to_string()));
    }
}

/// JS Number("...") semantics for the subset the CLI uses: trimmed decimal
/// integers/floats; empty string → 0; otherwise None (NaN).
pub fn js_number(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Some(0.0);
    }
    trimmed.parse::<f64>().ok()
}

#[derive(Clone, Debug, Default)]
pub struct ParsedArgs {
    pub flags: Flags,
    pub rest: Vec<String>,
}

pub fn parse_args(args: &[String]) -> ParsedArgs {
    let mut flags = Flags::default();
    let mut rest: Vec<String> = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if arg == "--" {
            rest.extend(args.iter().skip(index + 1).cloned());
            break;
        }
        if arg.starts_with('-') && !arg.starts_with("--") && arg.len() > 1 {
            for short_flag in arg[1..].chars() {
                flags
                    .0
                    .insert(short_flag.to_string(), FlagValue::Bool(true));
            }
            index += 1;
            continue;
        }
        if !arg.starts_with("--") {
            rest.push(arg.to_string());
            index += 1;
            continue;
        }
        let body = &arg[2..];
        if let Some(equals_index) = body.find('=') {
            flags.0.insert(
                to_camel_case(&body[..equals_index]),
                FlagValue::Text(body[equals_index + 1..].to_string()),
            );
            index += 1;
            continue;
        }
        let key = to_camel_case(body);
        let next = args.get(index + 1);
        match next {
            Some(next) if !next.starts_with("--") => {
                flags.0.insert(key, FlagValue::Text(next.clone()));
                index += 2;
            }
            _ => {
                flags.0.insert(key, FlagValue::Bool(true));
                index += 1;
            }
        }
    }
    ParsedArgs { flags, rest }
}

pub fn multi_value_flag(args: &[String], names: &[&str]) -> Vec<String> {
    let normalized_names: HashSet<String> = names
        .iter()
        .map(|name| {
            name.chars()
                .flat_map(|letter| {
                    if letter.is_ascii_uppercase() {
                        vec!['-', letter.to_ascii_lowercase()]
                    } else {
                        vec![letter]
                    }
                })
                .collect::<String>()
        })
        .collect();
    let mut values: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if !arg.starts_with("--") {
            index += 1;
            continue;
        }
        let body = &arg[2..];
        let (name, inline_value) = match body.find('=') {
            Some(equals_index) => (&body[..equals_index], Some(&body[equals_index + 1..])),
            None => (body, None),
        };
        if !normalized_names.contains(name) {
            index += 1;
            continue;
        }
        if let Some(inline_value) = inline_value {
            for value in inline_value.split(',') {
                let value = value.trim();
                if !value.is_empty() && seen.insert(value.to_string()) {
                    values.push(value.to_string());
                }
            }
            index += 1;
            continue;
        }
        index += 1;
        while index < args.len() && !args[index].starts_with("--") {
            for value in args[index].split(',') {
                let value = value.trim();
                if !value.is_empty() && seen.insert(value.to_string()) {
                    values.push(value.to_string());
                }
            }
            index += 1;
        }
    }
    values
}

pub fn to_camel_case(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '-' {
            if let Some(next) = chars.peek() {
                if next.is_ascii_lowercase() {
                    let next = chars.next().expect("peeked char");
                    output.push(next.to_ascii_uppercase());
                    continue;
                }
            }
            output.push(character);
            continue;
        }
        output.push(character);
    }
    output
}

pub fn parse_boolean(value: &FlagValue) -> bool {
    match value {
        FlagValue::Bool(flag) => *flag,
        FlagValue::Text(text) => text == "true" || text == "1" || text == "yes",
    }
}

pub fn parse_json_value(value: &str) -> Option<Value> {
    serde_json::from_str(value).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn parse_args_matches_node_cli_shapes() {
        let parsed = parse_args(&args(&[
            "one",
            "--json",
            "--project-id",
            "P1",
            "--title=My Title",
            "-ab",
            "--",
            "--not-a-flag",
        ]));
        assert_eq!(parsed.rest, vec!["one", "--not-a-flag"]);
        assert_eq!(parsed.flags.text("json"), Some("true".to_string()));
        assert_eq!(parsed.flags.string_value("projectId"), Some("P1"));
        assert_eq!(parsed.flags.string_value("title"), Some("My Title"));
        assert!(parsed.flags.truthy("a"));
        assert!(parsed.flags.truthy("b"));
    }

    #[test]
    fn parse_args_flag_consumes_next_non_flag() {
        let parsed = parse_args(&args(&["--lines", "40", "session-name"]));
        assert_eq!(parsed.flags.string_value("lines"), Some("40"));
        assert_eq!(parsed.rest, vec!["session-name"]);
    }

    #[test]
    fn multi_value_flag_collects_comma_and_repeat_values() {
        let values = multi_value_flag(
            &args(&["--agent=a,b", "--agent", "c", "d", "--other", "x"]),
            &["agent"],
        );
        assert_eq!(values, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn camel_case_matches_js_regex() {
        assert_eq!(to_camel_case("project-id"), "projectId");
        assert_eq!(to_camel_case("token-from-stdin"), "tokenFromStdin");
        assert_eq!(to_camel_case("a--b"), "a-B");
        assert_eq!(to_camel_case("trailing-"), "trailing-");
    }
}
