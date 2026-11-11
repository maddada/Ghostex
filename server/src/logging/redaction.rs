use super::*;

pub(super) fn normalize_log_entry(entry: GxserverLogInput) -> Value {
    let mut object = Map::new();
    object.insert(
        "ts".to_string(),
        json!(Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
    );
    object.insert("level".to_string(), json!(entry.level));
    object.insert("event".to_string(), json!(sanitize_log_text(&entry.event)));
    if let Some(server_id) = entry.server_id {
        object.insert("serverId".to_string(), json!(server_id));
    }
    if let Some(request_id) = entry.request_id {
        object.insert("requestId".to_string(), json!(request_id));
    }
    if let Some(client) = entry.client {
        object.insert("client".to_string(), json!(sanitize_log_text(&client)));
    }
    if let Some(duration_ms) = entry.duration_ms {
        object.insert("durationMs".to_string(), json!(duration_ms));
    }
    if let Some(error) = entry.error {
        object.insert("error".to_string(), json!(sanitize_log_text(&error)));
    }
    if let Some(details) = entry.details {
        object.insert(
            "details".to_string(),
            sanitize_log_value("details", details),
        );
    }
    Value::Object(object)
}

fn sanitize_log_value(key: &str, value: Value) -> Value {
    let key = key.to_ascii_lowercase();
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) => value,
        Value::String(text) => sanitize_string_field(&key, &text),
        Value::Array(items) => {
            if is_environment_key(&key) || is_sensitive_collection_key(&key) {
                json!({ "count": items.len(), "redacted": true })
            } else {
                Value::Array(
                    items
                        .into_iter()
                        .map(|item| sanitize_log_value(&key, item))
                        .collect(),
                )
            }
        }
        Value::Object(object) => {
            if is_environment_key(&key) || is_sensitive_collection_key(&key) {
                json!({ "redacted": true })
            } else {
                Value::Object(
                    object
                        .into_iter()
                        .map(|(entry_key, entry_value)| {
                            let sanitized = sanitize_log_value(&entry_key, entry_value);
                            (entry_key, sanitized)
                        })
                        .collect(),
                )
            }
        }
    }
}

fn sanitize_string_field(key: &str, value: &str) -> Value {
    // A push service's failure, as it named it (CDXC:Notifications 2026-11-11 in push/expo.rs): its code is a name
    // like `InvalidCredentials`, and its words have every push token cut out here again, so the words the secret rule
    // below would hide ("Credentials", "token") stay readable.
    if key == PUSH_ERROR_CODE_KEY && is_safe_identifier(value) {
        return json!(value);
    }
    if key == PUSH_ERROR_MESSAGE_KEY {
        return json!(redact_paths(&redact_urls(&cut_push_tokens(value))));
    }
    if is_secret_key(key) {
        return json!("[redacted:secret]");
    }
    if is_environment_key(key) {
        return json!("[redacted]");
    }
    if is_identifier_key(key) && is_safe_identifier(value) {
        return json!(value);
    }
    if is_url_key(key) || looks_like_url(value) {
        return summarize_url(value);
    }
    if is_path_key(key) || looks_like_path(value) {
        return json!("[redacted:path]");
    }
    if is_sensitive_text_key(key) {
        return json!("[redacted]");
    }
    json!(sanitize_log_text(value))
}

/// The log detail keys of a push service's failure, lowercased as the sanitizer compares them.
const PUSH_ERROR_CODE_KEY: &str = "pusherrorcode";
const PUSH_ERROR_MESSAGE_KEY: &str = "pusherrormessage";
/// The longest run of token-like characters a push failure's words may keep.
const PUSH_TOKEN_RUN_MAX: usize = 24;
const PUSH_MESSAGE_CHARS_MAX: usize = 300;

/// A push service's words on one line, at most 300 characters, with every push token cut out: a word that holds an
/// Expo push token (`ExponentPushToken[…]`, `ExpoPushToken[…]`) or a run of more than 24 letters, digits, `-` and
/// `_` (an APNs or FCM device token) becomes `[push token]`.
pub(crate) fn cut_push_tokens(text: &str) -> String {
    let mut out = String::new();
    for word in text.split_whitespace() {
        let mut run = 0usize;
        let mut longest = 0usize;
        for character in word.chars() {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                run += 1;
                longest = longest.max(run);
            } else {
                run = 0;
            }
        }
        let holds_token = word.contains("PushToken[") || longest > PUSH_TOKEN_RUN_MAX;
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(if holds_token { "[push token]" } else { word });
    }
    if out.chars().count() > PUSH_MESSAGE_CHARS_MAX {
        out = out.chars().take(PUSH_MESSAGE_CHARS_MAX - 1).collect();
        out.push('…');
    }
    out
}

fn sanitize_log_text(value: &str) -> String {
    let value = redact_json_string_fields(value);
    let value = redact_urls(&value);
    let value = redact_paths(&value);
    redact_secret_tokens(&value)
}

fn redact_json_string_fields(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut scan = 0;
    let mut copied_until = 0;
    while let Some(key_start) = find_from(value, '"', scan) {
        let Some(key_end) = find_unescaped_quote(value, key_start + 1) else {
            break;
        };
        let key = &value[key_start + 1..key_end];
        let mut cursor = skip_ascii_whitespace(value, key_end + 1);
        if value.as_bytes().get(cursor) != Some(&b':') {
            scan = key_end + 1;
            continue;
        }
        cursor = skip_ascii_whitespace(value, cursor + 1);
        if value.as_bytes().get(cursor) != Some(&b'"') {
            scan = cursor;
            continue;
        }
        let value_start = cursor + 1;
        let Some(value_end) = find_unescaped_quote(value, value_start) else {
            break;
        };
        let normalized_key = key.to_ascii_lowercase();
        if is_secret_key(&normalized_key)
            || is_url_key(&normalized_key)
            || is_path_key(&normalized_key)
            || is_sensitive_text_key(&normalized_key)
        {
            output.push_str(&value[copied_until..value_start]);
            output.push_str(redaction_for_key(&normalized_key));
            copied_until = value_end;
        }
        scan = value_end + 1;
    }
    output.push_str(&value[copied_until..]);
    output
}

fn redact_urls(value: &str) -> String {
    redact_matching_segments(value, &["http://", "https://"], "[redacted:url]", true)
}

fn redact_paths(value: &str) -> String {
    redact_matching_segments(
        value,
        &[
            "~/",
            "/Users/",
            "/Volumes/",
            "/private/",
            "/tmp/",
            "/var/folders/",
        ],
        "[redacted:path]",
        false,
    )
}

fn redact_matching_segments(
    value: &str,
    prefixes: &[&str],
    replacement: &str,
    case_insensitive: bool,
) -> String {
    let mut output = String::with_capacity(value.len());
    let mut cursor = 0;
    while cursor < value.len() {
        let next = prefixes
            .iter()
            .filter_map(|prefix| {
                find_prefix(&value[cursor..], prefix, case_insensitive)
                    .map(|index| (cursor + index, *prefix))
            })
            .min_by_key(|(index, _)| *index);
        let Some((start, prefix)) = next else {
            output.push_str(&value[cursor..]);
            break;
        };
        output.push_str(&value[cursor..start]);
        output.push_str(replacement);
        cursor = segment_end(value, start + prefix.len());
    }
    output
}

fn find_prefix(value: &str, prefix: &str, case_insensitive: bool) -> Option<usize> {
    if case_insensitive {
        value.to_ascii_lowercase().find(prefix)
    } else {
        value.find(prefix)
    }
}

fn segment_end(value: &str, start: usize) -> usize {
    for (offset, character) in value[start..].char_indices() {
        if character.is_whitespace()
            || matches!(character, '"' | '\'' | ')' | '(' | ']' | '[' | '}')
        {
            return start + offset;
        }
    }
    value.len()
}

fn redact_secret_tokens(value: &str) -> String {
    let mut sanitized = String::with_capacity(value.len());
    for word in value.split_whitespace() {
        let replacement = if contains_secret_marker(word) {
            "[redacted:secret]"
        } else {
            word
        };
        if !sanitized.is_empty() {
            sanitized.push(' ');
        }
        sanitized.push_str(replacement);
    }
    sanitized
}

fn find_from(value: &str, needle: char, start: usize) -> Option<usize> {
    value[start..].find(needle).map(|index| start + index)
}

fn find_unescaped_quote(value: &str, start: usize) -> Option<usize> {
    let mut escaped = false;
    for (offset, byte) in value.as_bytes()[start..].iter().enumerate() {
        if escaped {
            escaped = false;
        } else if *byte == b'\\' {
            escaped = true;
        } else if *byte == b'"' {
            return Some(start + offset);
        }
    }
    None
}

fn skip_ascii_whitespace(value: &str, mut cursor: usize) -> usize {
    while value
        .as_bytes()
        .get(cursor)
        .map(|byte| byte.is_ascii_whitespace())
        .unwrap_or(false)
    {
        cursor += 1;
    }
    cursor
}

fn redaction_for_key(key: &str) -> &'static str {
    if is_secret_key(key) {
        "[redacted:secret]"
    } else if is_url_key(key) {
        "[redacted:url]"
    } else if is_path_key(key) {
        "[redacted:path]"
    } else {
        "[redacted]"
    }
}

fn summarize_url(value: &str) -> Value {
    match url::Url::parse(value) {
        Ok(url) => json!({
            "host": url.host_str().unwrap_or_default(),
            "protocol": url.scheme(),
            "redacted": true,
            "type": "url",
        }),
        Err(_) => json!({ "redacted": true, "type": "url" }),
    }
}

fn is_identifier_key(key: &str) -> bool {
    key == "id"
        || key.ends_with("id")
        || key.ends_with("ids")
        || key.ends_with("ref")
        || key.ends_with("refs")
}

fn is_safe_identifier(value: &str) -> bool {
    value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

fn is_secret_key(key: &str) -> bool {
    key.contains("token")
        || key.contains("bearer")
        || key.contains("secret")
        || key.contains("credential")
        || key.contains("password")
        || key.contains("cookie")
        || key.contains("authorization")
        || key.contains("auth")
}

fn is_url_key(key: &str) -> bool {
    key == "url" || key.ends_with("url") || key.contains("uri") || key == "href" || key == "origin"
}

fn is_path_key(key: &str) -> bool {
    key == "path"
        || key == "cwd"
        || key.ends_with("path")
        || key.ends_with("dir")
        || key.ends_with("directory")
        || key.ends_with("root")
        || key.ends_with("file")
        || key.ends_with("filename")
        || key.contains("workspace")
}

fn is_sensitive_text_key(key: &str) -> bool {
    key == "title"
        || key.ends_with("title")
        || key == "name"
        || key.ends_with("name")
        || key == "message"
        || key == "details"
        || key.ends_with("details")
        || key == "input"
        || key == "text"
        || key.ends_with("text")
        || key == "comment"
        || key == "description"
        || key == "label"
        || key == "prompt"
        || key.ends_with("prompt")
        || key == "prompts"
        || key.ends_with("prompts")
        || key == "preview"
        || key.ends_with("preview")
        || key == "command"
        || key.ends_with("command")
        || key == "stdout"
        || key == "stderr"
        || key == "body"
        || key.ends_with("body")
}

fn is_sensitive_collection_key(key: &str) -> bool {
    key == "args" || key.ends_with("args") || key == "arguments" || key.ends_with("arguments")
}

fn is_environment_key(key: &str) -> bool {
    key == "env"
        || key == "envvars"
        || key == "envvariables"
        || key == "environment"
        || key == "environmentvariables"
        || key.ends_with("env")
        || key.ends_with("envvars")
        || key.ends_with("envvariables")
        || key.ends_with("environment")
        || key.ends_with("environmentvariables")
}

fn looks_like_url(value: &str) -> bool {
    value
        .get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("http://"))
        || value
            .get(..8)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"))
}

fn looks_like_path(value: &str) -> bool {
    value.starts_with("~/")
        || value.starts_with("/Users/")
        || value.starts_with("/Volumes/")
        || value.starts_with("/private/")
        || value.starts_with("/tmp/")
        || value.starts_with("/var/folders/")
}

fn contains_secret_marker(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("bearer")
        || lower.contains("token")
        || lower.contains("authorization")
        || lower.contains("password")
        || lower.contains("secret")
        || lower.contains("credential")
}
