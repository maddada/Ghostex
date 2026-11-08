//! Text that looks like a secret (an API key, a token, a `password:` value, a private key block)
//! is replaced with `«redacted N chars»` before it is written to a bot's memory, the shared
//! preferences or its daily log, so a key pasted into a chat never sits in a plain file every
//! chat of the bot reads.
//!
//! CDXC:GBots 2026-11-09 WHY:
//! Ghostex's log redaction (server/src/logging/redaction.rs) blanks every word containing "token" or "secret", which would eat ordinary notes ("the token budget is tight"). Memory matches only the shapes of real keys (known prefixes with a long body, JWTs, private key blocks) and the value after a `password:`/`api_key=`-style label.

/// Prefixes of keys and tokens that are secret whatever follows; the whole word is redacted when
/// it is at least [`PREFIXED_MIN_CHARS`] long.
const SECRET_PREFIXES: [&str; 24] = [
    "sk-",
    "sk_live_",
    "sk_test_",
    "rk_live_",
    "pk_live_",
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "github_pat_",
    "glpat-",
    "xoxb-",
    "xoxp-",
    "xoxa-",
    "xoxr-",
    "xapp-",
    "AIza",
    "gxd_",
    "gxb_",
    "whsec_",
    "hf_",
    "npm_",
    "pypi-",
];
const PREFIXED_MIN_CHARS: usize = 20;
/// Labels whose value is a secret: `password: hunter22`, `api_key=…`, `"token": "…"`.
const SECRET_LABELS: [&str; 12] = [
    "password",
    "passwd",
    "passphrase",
    "api_key",
    "api-key",
    "apikey",
    "access_token",
    "auth_token",
    "client_secret",
    "secret_key",
    "private_key",
    "token",
];

fn marker(chars: usize) -> String {
    format!("«redacted {chars} chars»")
}

fn is_word_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | '/' | '+' | '=')
}

fn is_secret_word(word: &str) -> bool {
    let word = word.trim_end_matches(['.', '=']);
    let length = word.chars().count();
    if SECRET_PREFIXES
        .iter()
        .any(|prefix| word.starts_with(prefix) && length >= PREFIXED_MIN_CHARS)
    {
        return true;
    }
    // AWS access key ids.
    if (word.starts_with("AKIA") || word.starts_with("ASIA"))
        && length == 20
        && word
            .chars()
            .all(|character| character.is_ascii_uppercase() || character.is_ascii_digit())
    {
        return true;
    }
    // A JSON web token: three base64url parts, the first a JSON object.
    word.starts_with("eyJ") && word.matches('.').count() == 2 && length >= 30
}

/// Private key blocks, whole.
fn redact_key_blocks(text: &str, count: &mut usize) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("-----BEGIN ") {
        let head_end = rest[start..].find('\n').map(|end| start + end);
        let is_private = head_end
            .map(|end| rest[start..end].contains("PRIVATE KEY"))
            .unwrap_or(false);
        // The `-----` that closes the `-----END … PRIVATE KEY-----` line.
        let end = rest[start..].find("-----END ").and_then(|end| {
            let after = start + end + "-----END ".len();
            rest[after..].find("-----").map(|tail| after + tail)
        });
        match (is_private, end) {
            (true, Some(end)) => {
                let block_end = end + "-----".len();
                out.push_str(&rest[..start]);
                out.push_str(&marker(rest[start..block_end].chars().count()));
                *count += 1;
                rest = &rest[block_end..];
            }
            _ => {
                out.push_str(&rest[..start + "-----BEGIN ".len()]);
                rest = &rest[start + "-----BEGIN ".len()..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Words shaped like keys.
fn redact_words(text: &str, count: &mut usize) -> String {
    let mut out = String::with_capacity(text.len());
    let mut word = String::new();
    let mut flush = |word: &mut String, out: &mut String| {
        if is_secret_word(word) {
            out.push_str(&marker(word.chars().count()));
            *count += 1;
        } else {
            out.push_str(word);
        }
        word.clear();
    };
    for character in text.chars() {
        if is_word_char(character) {
            word.push(character);
        } else {
            flush(&mut word, &mut out);
            out.push(character);
        }
    }
    flush(&mut word, &mut out);
    out
}

/// The values after secret labels.
fn redact_labelled(text: &str, count: &mut usize) -> String {
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    let mut search = 0;
    while search < lower.len() {
        let found = SECRET_LABELS
            .iter()
            .filter_map(|label| lower[search..].find(label).map(|at| (search + at, *label)))
            .min_by_key(|(at, _)| *at);
        let Some((at, label)) = found else {
            break;
        };
        let before_ok = at == 0
            || !lower[..at]
                .chars()
                .next_back()
                .is_some_and(|character| character.is_ascii_alphanumeric() || character == '_');
        let mut cursor = at + label.len();
        search = cursor;
        if !before_ok {
            continue;
        }
        let bytes = lower.as_bytes();
        // `"token": "…"`: skip a closing quote after the label.
        if cursor < bytes.len() && matches!(bytes[cursor], b'"' | b'\'') {
            cursor += 1;
        }
        while cursor < bytes.len() && bytes[cursor] == b' ' {
            cursor += 1;
        }
        if cursor >= bytes.len() || !matches!(bytes[cursor], b':' | b'=') {
            continue;
        }
        cursor += 1;
        while cursor < bytes.len() && bytes[cursor] == b' ' {
            cursor += 1;
        }
        let quote =
            (cursor < bytes.len() && matches!(bytes[cursor], b'"' | b'\'')).then(|| bytes[cursor]);
        if quote.is_some() {
            cursor += 1;
        }
        let value_start = cursor;
        while cursor < bytes.len() {
            let byte = bytes[cursor];
            let ends = match quote {
                Some(quote) => byte == quote || byte == b'\n',
                None => byte.is_ascii_whitespace() || matches!(byte, b',' | b';' | b'"' | b'\''),
            };
            if ends {
                break;
            }
            cursor += 1;
        }
        let value = &text[value_start..cursor];
        if value.chars().count() >= 4 && !value.starts_with('«') {
            out.push_str(&text[copied..value_start]);
            out.push_str(&marker(value.chars().count()));
            *count += 1;
            copied = cursor;
        }
        search = cursor.max(search);
    }
    out.push_str(&text[copied..]);
    out
}

/// `text` with every secret-looking part replaced, and how many were.
pub(crate) fn redact(text: &str) -> (String, usize) {
    let mut count = 0;
    let text = redact_key_blocks(text, &mut count);
    let text = redact_labelled(&text, &mut count);
    let text = redact_words(&text, &mut count);
    (text, count)
}
