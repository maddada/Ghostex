//! Whether chat box text looks like it holds a key or token, so Send asks once more before it goes
//! into the chat and the text is kept out of the saved draft. Passwords are not recognised: they
//! look like ordinary words.
//!
//! CDXC:Secrets 2026-11-19 DECISION:
//! User: "keep the chat box usable while a secret card is open. Typed text goes to the chat as normal; the secret is only ever taken from the card's own field. If the worry is someone pasting a secret into the chat box by mistake, a warning when the text looks like a key (e.g. lin_api_…, sk-…) would cover it without blocking." The chat box no longer locks while a private secret card is open; instead text that looks like a key (a known key prefix, a private key block, or a long random-looking token) shows a quiet warning above the chat box at the first Send, and only a second Send of the same text posts it. Ordinary text is never held, and word-shaped tokens (branch names, identifiers, ARNs, paths) are ordinary text.
//! SEE-ALSO: apps/desktop/src/app/native_chat/secret_send_warning.rs (desktop and web), apps/mobile/app/src/chat/rust/secretLike.ts (the phone's copy of this rule, which must decide at Send without waiting on the core), tests/fixtures/secret_like.json (the strings both copies are checked against), `draft_sync.rs` (no draft save while this matches).

/// One known key or token prefix and what follows it in a real key.
struct Prefix {
    prefix: &'static str,
    /// The shortest body, so a word such as `sk-learn` is not one.
    body: usize,
    /// Whether the body may hold `-` and `_` (`npm_` and `hf_` bodies are letters and digits
    /// only, so `npm_package_…` variables are not keys).
    separators: bool,
    /// Whether ordinary words can follow the prefix (`sk-learn-…`, `npm_package_…`), so a body
    /// made of words is not a key. A prefix only keys carry skips that test: a random body
    /// sometimes looks word-shaped, and about one key in ten was missed with it.
    words: bool,
}

const fn prefix(prefix: &'static str, body: usize, separators: bool, words: bool) -> Prefix {
    Prefix {
        prefix,
        body,
        separators,
        words,
    }
}

/// Known key and token prefixes. `gxs_`, `gxo_` and `gxd_` are Ghostex's own session, owner and
/// device tokens (server/src/security/session_tokens.rs, server/src/auth.rs: 32 random bytes in
/// URL-safe base64).
const PREFIXES: &[Prefix] = &[
    prefix("sk-ant-", 20, true, false),
    prefix("sk-proj-", 20, true, false),
    prefix("sk-", 20, true, true),
    prefix("sk_live_", 16, true, false),
    prefix("sk_test_", 16, true, false),
    prefix("rk_live_", 16, true, false),
    prefix("lin_api_", 20, true, false),
    prefix("lin_oauth_", 20, true, false),
    prefix("ghp_", 20, true, false),
    prefix("gho_", 20, true, false),
    prefix("ghu_", 20, true, false),
    prefix("ghs_", 20, true, false),
    prefix("ghr_", 20, true, false),
    prefix("github_pat_", 20, true, false),
    prefix("glpat-", 16, true, false),
    prefix("xoxb-", 10, true, false),
    prefix("xoxp-", 10, true, false),
    prefix("xoxa-", 10, true, false),
    prefix("xoxr-", 10, true, false),
    prefix("xoxs-", 10, true, false),
    prefix("xapp-", 10, true, false),
    prefix("npm_", 30, false, true),
    prefix("hf_", 30, false, false),
    prefix("AIza", 30, true, false),
    prefix("gxs_", 20, true, false),
    prefix("gxo_", 20, true, false),
    prefix("gxd_", 20, true, false),
];

/// Whether `text` looks like it holds a key or token.
pub fn looks_like_secret(text: &str) -> bool {
    if text.contains("PRIVATE KEY-----") {
        return true;
    }
    tokens(text).any(|token| known_prefix(token) || aws_key_id(token) || random_token(token))
}

/// The words of `text` (`tokens`), each with whether it is a piece of a path written with spaces in
/// it. Such a piece is not judged as what is left of a key (`random_fragment`, 16 characters); a
/// whole random token (`random_token`, 32) still counts in it, as do a key's prefix and an AWS key
/// id.
///
/// A path starts at a word that starts with `/` or `~` (a word that does is never judged random
/// either) and goes on through each word one space after the last that has a `/` and a folder
/// named as a word is (`Support/ClipBook`), and ends at a file name with its extension
/// (`Chrome_19-33-36@2x.png`).
///
/// CDXC:Secrets 2026-11-20 WHY:
/// The user, of a draft holding a picture pasted from the ClipBook clipboard app ("it's wrong, it
/// doesn't hold a key or token, pls fix"): its `[Image #1](/Users/…/Application Support/ClipBook
/// Rust/attachments/04170605…4d1c/2026-11-19_Google Chrome_19-33-36@2x.png)` reference split at its
/// spaces, and the piece `Rust/attachments/04170605…/2026-11-19_Google` read as a random token: the
/// 64-character content hash made the long run and `Rust` and `Google` the upper case. The chat
/// box then held its Send and kept the draft off other devices. The Send warning is fixed by the
/// long run needing both cases (`longest_random_run`), so a whole random token still counts in a
/// path's piece: a security review found that exempting it there let a base64 key typed one space
/// after a path or link (`cat /tmp/notes <key>`) go unwarned. Only the draft's 16-character
/// fragment check skips the piece, as `2026-11-19_Google` alone is long and mixed enough to be
/// one. Every reference Ghostex writes for an attachment starts with `/` or `~` (or a drive,
/// whose `\` no key holds), so this covers them and a path the user pastes alike.
fn judged_tokens(text: &str) -> impl Iterator<Item = (&str, bool)> {
    let mut in_path = false;
    let mut previous_end = 0;
    tokens(text).map(move |token| {
        // `tokens` yields slices of `text`, so a token's place is its offset in it.
        let start = token.as_ptr() as usize - text.as_ptr() as usize;
        let joined = in_path && &text[previous_end..start] == " ";
        previous_end = start + token.len();
        if token.starts_with('/') || token.starts_with('~') {
            in_path = true;
            return (token, false);
        }
        if joined && token.contains('/') && token.split('/').any(word_shaped) {
            return (token, true);
        }
        in_path = false;
        (token, joined && file_name(token))
    })
}

/// A file name ending in an extension of one to five letters and digits (`2x.png`, `notes.md`).
fn file_name(token: &str) -> bool {
    token.rsplit_once('.').is_some_and(|(stem, extension)| {
        !stem.is_empty()
            && (1..=5).contains(&extension.len())
            && extension.chars().all(|c| c.is_ascii_alphanumeric())
            && extension.chars().any(|c| c.is_ascii_alphabetic())
    })
}

/// The words of `text`, split at whitespace (U+FEFF and U+0085 included, as the phone's copy
/// splits), quotes, brackets and the `=`/`:` of `KEY=value`.
fn tokens(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                '\u{feff}'
                    | '\u{85}'
                    | '"'
                    | '\''
                    | '`'
                    | '='
                    | ':'
                    | ','
                    | ';'
                    | '('
                    | ')'
                    | '['
                    | ']'
                    | '{'
                    | '}'
                    | '<'
                    | '>'
            )
    })
    .filter(|token| !token.is_empty())
}

fn known_prefix(token: &str) -> bool {
    PREFIXES.iter().any(|known| {
        token.strip_prefix(known.prefix).is_some_and(|rest| {
            let run = body_run(rest, known.separators);
            run.len() >= known.body && mixed(run) && !(known.words && word_shaped(run))
        })
    })
}

/// The key characters `rest` starts with: letters and digits, and `-` and `_` when `separators`.
fn body_run(rest: &str, separators: bool) -> &str {
    let len = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || (separators && matches!(c, '_' | '-'))))
        .unwrap_or(rest.len());
    &rest[..len]
}

/// What a saved draft holds in place of anything that could be part of a key.
pub const MASK: &str = "[key removed]";

/// Whether `text` holds anything that could be part of a key: a key, or what is left of one while
/// it is deleted a character at a time (`key_fragment`). A draft is not saved to gxserver while
/// this holds (draft_sync.rs).
pub fn has_key_fragment(text: &str) -> bool {
    text.contains("PRIVATE KEY")
        || judged_tokens(text).any(|(token, path)| key_fragment(token, path))
}

/// `text` with everything that could be part of a key replaced by [`MASK`], or `None` when nothing
/// is: what this client stores as the draft, so a long message survives a restart without the key.
/// A private key block keeps its `BEGIN` and `END` lines and loses everything between them.
pub fn mask_key_fragments(text: &str) -> Option<String> {
    let (text, blocks) = mask_private_key_blocks(text);
    let mut masked = String::with_capacity(text.len());
    let mut copied = 0;
    let mut any = blocks;
    for (token, _) in judged_tokens(&text).filter(|(token, path)| key_fragment(token, *path)) {
        // `tokens` yields slices of `text`, so a token's place is its offset in it.
        let start = token.as_ptr() as usize - text.as_ptr() as usize;
        masked.push_str(&text[copied..start]);
        masked.push_str(MASK);
        copied = start + token.len();
        any = true;
    }
    any.then(|| {
        masked.push_str(&text[copied..]);
        masked
    })
}

/// `text` with the body of every private key block (from the end of its `-----BEGIN … PRIVATE
/// KEY-----` line to the start of its `-----END` line, or to the end of the text while the block
/// is still being pasted) replaced by [`MASK`], and whether there was one.
fn mask_private_key_blocks(text: &str) -> (String, bool) {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut found = false;
    while let Some(begin) = rest.find("-----BEGIN") {
        let header_end = rest[begin..]
            .find('\n')
            .map_or(rest.len(), |offset| begin + offset);
        if !rest[begin..header_end].contains("PRIVATE KEY") {
            out.push_str(&rest[..header_end]);
            rest = &rest[header_end..];
            continue;
        }
        found = true;
        out.push_str(&rest[..header_end]);
        let body = &rest[header_end..];
        match body.find("-----END") {
            Some(end) => {
                out.push_str("\n");
                out.push_str(MASK);
                out.push_str("\n");
                rest = &body[end..];
            }
            None => {
                out.push_str("\n");
                out.push_str(MASK);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    (out, found)
}

/// A key, or a piece of one left while it is deleted: a known prefix with at least 8 mixed key
/// characters after it (a body of words is not one, after a prefix ordinary words can follow),
/// `AKIA`/`ASIA` with at least 8 capitals or digits, or a random run of 16 characters or more.
/// Ordinary words, links and paths stay out (`not_a_fragment` in the fixture); a piece of a path
/// with spaces in it (`path`, `judged_tokens`) is not judged as a random fragment.
fn key_fragment(token: &str, path: bool) -> bool {
    if known_prefix(token) || aws_key_id(token) || random_token(token) {
        return true;
    }
    let prefixed = PREFIXES.iter().any(|known| {
        token.strip_prefix(known.prefix).is_some_and(|rest| {
            let run = body_run(rest, known.separators);
            run.len() >= 8 && mixed(run) && !(known.words && word_shaped(run))
        })
    });
    let aws = ["AKIA", "ASIA"].iter().any(|prefix| {
        token.strip_prefix(prefix).is_some_and(|rest| {
            rest.chars()
                .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                .count()
                >= 8
        })
    });
    prefixed || aws || (!path && random_fragment(token))
}

/// What is left of a random token being deleted: 16 characters or more of key characters with
/// upper case, lower case and a digit, and not made of words. Links and paths are not: like
/// `random_token`, nothing with `//` or starting with `/` or `~`, and a run of 16 or more between
/// its `/` and `.` separators (`longest_random_run`).
fn random_fragment(token: &str) -> bool {
    let token = token.trim_end_matches(['.', '!', '?']);
    let longest_run = longest_random_run(token);
    token.len() >= 16
        && !token.contains("//")
        && !token.starts_with('/')
        && !token.starts_with('~')
        && longest_run >= 16
        && token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+' | '/' | '.'))
        && token.chars().any(|c| c.is_ascii_uppercase())
        && token.chars().any(|c| c.is_ascii_lowercase())
        && token.chars().any(|c| c.is_ascii_digit())
        && !word_shaped_from(token, 3)
}

/// Digits, or both upper and lower case: what a random body has and a word does not.
fn mixed(run: &str) -> bool {
    run.chars().any(|c| c.is_ascii_digit())
        || (run.chars().any(|c| c.is_ascii_uppercase())
            && run.chars().any(|c| c.is_ascii_lowercase()))
}

/// An AWS access key id: `AKIA` or `ASIA` and 16 upper-case letters or digits.
fn aws_key_id(token: &str) -> bool {
    ["AKIA", "ASIA"].iter().any(|prefix| {
        token.strip_prefix(prefix).is_some_and(|rest| {
            rest.len() >= 16
                && rest
                    .chars()
                    .take(16)
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        })
    })
}

/// A long run of letters, digits and `_-+/.` with upper case, lower case and digits mixed, little
/// repetition, and not made of words. Hex (commit ids, hashes, UUIDs) has one case only, even
/// inside a path, and paths have a `/` between words, so neither counts.
fn random_token(token: &str) -> bool {
    let token = token.trim_end_matches(['.', '!', '?']);
    if token.chars().count() < 32
        || token.contains("//")
        || token.starts_with('/')
        || token.starts_with('~')
        || !token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+' | '/' | '.'))
    {
        return false;
    }
    let upper = token.chars().filter(char::is_ascii_uppercase).count();
    let lower = token.chars().filter(char::is_ascii_lowercase).count();
    let digits = token.chars().filter(char::is_ascii_digit).count();
    // A path or dotted name: words between separators, not one random run.
    let longest_run = longest_random_run(token);
    upper >= 2
        && lower >= 2
        && digits >= 1
        && longest_run >= 24
        && entropy(token) >= 4.0
        && !word_shaped(token)
}

/// The length of the longest run of `token` between its `/` and `.` separators that has both upper
/// and lower case, as a random run has. Hex has one case only, so a content hash or commit id in a
/// path (`attachments/04170605…/Shot.png`) is not the long run, whatever case the folder names
/// around it are written in (CDXC:Secrets 2026-11-20 at `judged_tokens`).
fn longest_random_run(token: &str) -> usize {
    token
        .split(['/', '.'])
        .filter(|run| {
            run.chars().any(|c| c.is_ascii_uppercase())
                && run.chars().any(|c| c.is_ascii_lowercase())
        })
        .map(str::len)
        .max()
        .unwrap_or_default()
}

/// Whether at least half of `token`'s letters and digits sit in words: runs of four or more
/// letters written as a word is (`some`, `Branch`), split where camelCase starts a new word.
/// Random keys have few such runs; branch names, identifiers and ARNs are mostly words.
fn word_shaped(token: &str) -> bool {
    word_shaped_from(token, 4)
}

/// `word_shaped` counting words of `shortest` letters or more: a piece of a key is judged with
/// three-letter words too (`fix-tab-key-bug`), as only drafts hang on it.
fn word_shaped_from(token: &str, shortest: usize) -> bool {
    let alphanumeric = token.chars().filter(char::is_ascii_alphanumeric).count();
    if alphanumeric == 0 {
        return false;
    }
    let in_words: usize = token
        .split(|c: char| !c.is_ascii_alphabetic())
        .flat_map(camel_words)
        .filter(|word| word.len() >= shortest && word_cased(word))
        .map(str::len)
        .sum();
    in_words * 2 >= alphanumeric
}

/// A run of ASCII letters split where a new camelCase word starts: before an upper-case letter
/// that follows a lower-case one, and before the last of several upper-case letters when a
/// lower-case one follows it (`HTTPServer` is `HTTP` and `Server`).
fn camel_words(run: &str) -> Vec<&str> {
    let bytes = run.as_bytes();
    let mut words = Vec::new();
    let mut start = 0;
    for i in 1..bytes.len() {
        let (before, at) = (bytes[i - 1], bytes[i]);
        let after = bytes.get(i + 1).copied();
        let lower_to_upper = before.is_ascii_lowercase() && at.is_ascii_uppercase();
        let acronym_end = before.is_ascii_uppercase()
            && at.is_ascii_uppercase()
            && after.is_some_and(|c| c.is_ascii_lowercase());
        if lower_to_upper || acronym_end {
            words.push(&run[start..i]);
            start = i;
        }
    }
    if start < run.len() {
        words.push(&run[start..]);
    }
    words
}

/// Written as a word is: all lower case, or one capital and then lower case; and pronounceable,
/// with a vowel and no four consonants in a row, which a run of random letters often is not.
fn word_cased(word: &str) -> bool {
    let mut chars = word.chars();
    let first = chars.next().is_some_and(|c| c.is_ascii_alphabetic());
    if !(first && chars.all(|c| c.is_ascii_lowercase())) {
        return false;
    }
    let vowel = |c: char| matches!(c.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u' | 'y');
    let mut consonants = 0;
    for c in word.chars() {
        consonants = if vowel(c) { 0 } else { consonants + 1 };
        if consonants >= 4 {
            return false;
        }
    }
    word.chars().any(vowel)
}

/// Shannon entropy of `token`'s characters, in bits per character.
fn entropy(token: &str) -> f64 {
    let mut counts = std::collections::HashMap::new();
    for c in token.chars() {
        *counts.entry(c).or_insert(0usize) += 1;
    }
    let len = token.chars().count() as f64;
    counts
        .values()
        .map(|count| {
            let p = *count as f64 / len;
            -p * p.log2()
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    /// The strings both copies of the rule are checked against; the phone's copy reads the same
    /// file (`tests/secret_like_phone_copy.test.ts`). A `{}` in a string splits a token so no
    /// whole one is written in the file.
    const FIXTURE: &str = include_str!("../../tests/fixtures/secret_like.json");

    fn fixture() -> Value {
        serde_json::from_str(FIXTURE).expect("fixture is JSON")
    }

    #[test]
    fn fixture_strings() {
        let fixture = fixture();
        for (key, expected) in [
            ("flagged", true),
            ("ordinary", false),
            ("not_a_fragment", false),
        ] {
            for text in fixture[key].as_array().expect("a list") {
                let text = text.as_str().expect("a string").replace("{}", "");
                assert_eq!(super::looks_like_secret(&text), expected, "{key}: {text:?}");
            }
        }
        // Draft text that is not even a piece of a key is saved as written.
        for key in ["ordinary", "not_a_fragment"] {
            for text in fixture[key].as_array().expect("a list") {
                let text = text.as_str().expect("a string").replace("{}", "");
                assert!(
                    !super::has_key_fragment(&text),
                    "{key} is a fragment: {text:?}"
                );
                assert_eq!(
                    super::mask_key_fragments(&text),
                    None,
                    "{key} masked: {text:?}"
                );
            }
        }
    }

    fn xorshift(state: &mut u32) -> u32 {
        let mut x = *state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        *state = x;
        x
    }

    /// Random keys of each real format, built at test time (`generated` in the fixture): every
    /// one is flagged.
    #[test]
    fn generated_keys() {
        let fixture = fixture();
        let generated = &fixture["generated"];
        let mut state = generated["seed"].as_u64().expect("a seed") as u32;
        let count = generated["count"].as_u64().expect("a count");
        for format in generated["formats"].as_array().expect("formats") {
            for _ in 0..count {
                let mut key = String::new();
                for part in format.as_array().expect("parts") {
                    match part {
                        Value::String(literal) => key.push_str(&literal.replace("{}", "")),
                        part => {
                            let alphabet: Vec<char> = generated["alphabets"]
                                [part[0].as_str().expect("an alphabet")]
                            .as_str()
                            .expect("letters")
                            .chars()
                            .collect();
                            for _ in 0..part[1].as_u64().expect("a length") {
                                let index = xorshift(&mut state) as usize % alphabet.len();
                                key.push(alphabet[index]);
                            }
                        }
                    }
                }
                assert!(
                    super::looks_like_secret(&key),
                    "missed a generated key: {key}"
                );
            }
        }
    }
}
