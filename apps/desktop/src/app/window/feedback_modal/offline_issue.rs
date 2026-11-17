//! "Open a GitHub issue in your browser": the Send Bug / Feedback path that works while gxserver
//! does not answer. The report is cleaned of personal data here, on this computer, and opened as a
//! prefilled new-issue page; nothing is posted until the user presses Create in the browser.
//!
//! CDXC:Feedback 2026-10-22 DECISION:
//! User, asked whether Send Bug / Feedback should offer a way to report that works without gxserver: "Yes, with personal data scrubbed". When gxserver gives no answer, the dialog offers "Open a GitHub issue in your browser" on maddada/Ghostex with the user's message and this computer's startup diagnostics filled in. Before the browser opens, user names, home and profile paths, computer names, saved remote machines, addresses, emails and tokens are replaced (the rules of the phone's crash-report scrub, apps/mobile/app/src/feedback/crashReportScrub.ts, plus the names this computer knows). A link longer than GitHub accepts has its diagnostics shortened, never the message, and says so; a message too long even alone goes to the clipboard and the page says to paste it. While gxserver answers, the dialog works as before.
//! SEE-ALSO: state.rs (`open_offline_issue`), apps/desktop/src/app/feedback_modal_lifecycle.rs (the diagnostics and the names to remove), apps/desktop/src/app/gx_store/rpc_types.rs (`unanswered`).

use regex::{Captures, Regex};
use std::sync::LazyLock;

/// The repository the browser opens a new issue on.
const NEW_ISSUE_URL: &str = "https://github.com/maddada/Ghostex/issues/new";
/// GitHub refuses longer new-issue links (HTTP 414), so the whole link stays under this.
const MAX_URL_LENGTH: usize = 8_000;
const MAX_TITLE_CHARS: usize = 80;

/// What this computer knows is private and must not reach the issue.
#[derive(Clone, Default)]
pub(crate) struct FeedbackScrubContext {
    /// Home and profile folders, replaced by `~` wherever they appear.
    pub(crate) home_dirs: Vec<String>,
    /// User names, computer names and saved remote machines' names, hosts and users.
    pub(crate) private_words: Vec<String>,
}

pub(super) struct OfflineIssue {
    pub(super) url: String,
    /// The whole report, when even the message alone was too long for a link.
    pub(super) clipboard: Option<String>,
}

/// The new-issue link for `message` (and `title`, else the message's first line), with the
/// diagnostics shortened until the link fits.
pub(super) fn offline_issue(
    title: &str,
    message: &str,
    diagnostics: Option<&str>,
    context: &FeedbackScrubContext,
) -> OfflineIssue {
    let message = scrub_personal_data(message.trim(), context);
    let title = scrub_personal_data(title.trim(), context);
    let title = if title.is_empty() {
        let first_line = message
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("");
        let mut title: String = first_line.trim().chars().take(MAX_TITLE_CHARS).collect();
        if first_line.trim().chars().count() > MAX_TITLE_CHARS {
            title.push('…');
        }
        if title.is_empty() {
            "Bug report".to_string()
        } else {
            title
        }
    } else {
        title
    };
    let diagnostics = diagnostics
        .map(|text| scrub_personal_data(text.trim(), context))
        .unwrap_or_default();
    let full_body = issue_body(&message, &diagnostics, 0);
    let url = issue_url(&title, &full_body);
    if url.len() <= MAX_URL_LENGTH {
        return OfflineIssue {
            url,
            clipboard: None,
        };
    }
    // The longest start of the diagnostics that still fits.
    let characters: Vec<char> = diagnostics.chars().collect();
    let (mut low, mut high) = (0, characters.len());
    while low < high {
        let middle = (low + high).div_ceil(2);
        let kept: String = characters[..middle].iter().collect();
        let body = issue_body(&message, &kept, characters.len() - middle);
        if issue_url(&title, &body).len() <= MAX_URL_LENGTH {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    let kept: String = characters[..low].iter().collect();
    let body = issue_body(&message, &kept, characters.len() - low);
    let url = issue_url(&title, &body);
    if url.len() <= MAX_URL_LENGTH {
        return OfflineIssue {
            url,
            clipboard: None,
        };
    }
    OfflineIssue {
        url: issue_url(
            &title,
            "This report was too long for a link, so Ghostex put it on your clipboard. Paste it here before you create the issue.",
        ),
        clipboard: Some(full_body),
    }
}

fn issue_body(message: &str, diagnostics: &str, left_out: usize) -> String {
    let mut body = format!(
        "{message}\n\n---\nSent from Send Bug / Feedback in Ghostex while its background service (gxserver) was not answering. User names, home folders, computer names, addresses and tokens were removed before this page opened.\n"
    );
    if !diagnostics.is_empty() || left_out > 0 {
        body.push_str("\n<details><summary>Startup diagnostics</summary>\n\n");
        if left_out > 0 {
            body.push_str(&format!(
                "The diagnostics were shortened to fit in a link: the last {left_out} characters were left out.\n\n"
            ));
        }
        body.push_str(&format!("```text\n{diagnostics}\n```\n\n</details>\n"));
    }
    body
}

fn issue_url(title: &str, body: &str) -> String {
    format!(
        "{NEW_ISSUE_URL}?title={}&body={}",
        encode_query_value(title),
        encode_query_value(body)
    )
}

fn encode_query_value(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("valid scrub pattern")
}

/// Replaces every match `keep_or_replace` returns text for; `None` keeps the match. It also gets
/// the text before and after the match, which stands in for the lookarounds `regex` lacks.
fn replace_checked(
    text: &str,
    pattern: &Regex,
    mut keep_or_replace: impl FnMut(&Captures, &str, &str) -> Option<String>,
) -> String {
    let mut result = String::with_capacity(text.len());
    let mut last = 0;
    for captures in pattern.captures_iter(text) {
        let whole = captures.get(0).expect("whole match");
        if let Some(replacement) =
            keep_or_replace(&captures, &text[..whole.start()], &text[whole.end()..])
        {
            result.push_str(&text[last..whole.start()]);
            result.push_str(&replacement);
            last = whole.end();
        }
    }
    result.push_str(&text[last..]);
    result
}

fn starts_with_word_char(text: &str) -> bool {
    text.chars()
        .next()
        .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

/// `.x`, `-x` or a word character right after a match: the match was part of a longer name.
fn continues_name(after: &str) -> bool {
    starts_with_word_char(after)
        || (after.starts_with('.') && after[1..].chars().next().is_some_and(char::is_alphanumeric))
}

/// File types a stack names after an `@`, which are not email domains.
const CODE_FILE_EXTENSIONS: [&str; 12] = [
    "bundle", "js", "jsx", "mjs", "ts", "tsx", "hbc", "kt", "java", "swift", "mm", "so",
];

const PLACEHOLDER_WORDS: [&str; 9] = [
    "private",
    "redacted",
    "token",
    "email",
    "host",
    "user",
    "app",
    "private key",
    "cut",
];

static PRIVATE_KEY: LazyLock<Regex> = LazyLock::new(|| {
    regex(r"(?s)-----BEGIN [A-Z ]*PRIVATE KEY-----.*?-----END [A-Z ]*PRIVATE KEY-----")
});
static JWT: LazyLock<Regex> =
    LazyLock::new(|| regex(r"\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\b"));
static VENDOR_TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    regex(
        r"\b(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|sk-[A-Za-z0-9_-]{20,}|xox[abprs]-[A-Za-z0-9-]{10,}|AKIA[0-9A-Z]{16})\b",
    )
});
static AUTH_SCHEME: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(?i)\b(Bearer|Basic)\s+[A-Za-z0-9._~+/=-]{8,}"));
static SECRET_FIELD: LazyLock<Regex> = LazyLock::new(|| {
    regex(
        r#"(?i)\b([A-Za-z]*[_-]?(?:token|key|secret|password|passwd|pwd|cookie|authorization|credential)s?)(["']?\s*[:=]\s*["']?)([^\s"'&,;}]+)"#,
    )
});
static URL_CREDENTIALS: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(?i)\b([a-z][a-z0-9+.-]*://)[^\s/@:]+(?::[^\s/@]*)?@"));
static UNIX_HOME: LazyLock<Regex> = LazyLock::new(|| regex(r#"/(?:Users|home)/[^/\s:'"`)]+"#));
static WINDOWS_HOME: LazyLock<Regex> =
    LazyLock::new(|| regex(r#"(?i)\b[A-Z]:[\\/]Users[\\/][^\\/\s:'"`)]+"#));
static EMAIL: LazyLock<Regex> = LazyLock::new(|| {
    regex(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.([A-Za-z]{2,})\b")
});
static LOCAL_HOST: LazyLock<Regex> = LazyLock::new(|| {
    regex(
        r"(?i)\b[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.(?:local|lan|home|internal|localdomain|ts\.net|beta\.tailscale\.net)\b",
    )
});
static IPV4: LazyLock<Regex> = LazyLock::new(|| {
    regex(r"\b(?:25[0-5]|2[0-4]\d|1?\d?\d)(?:\.(?:25[0-5]|2[0-4]\d|1?\d?\d)){3}\b")
});
static IPV6: LazyLock<Regex> = LazyLock::new(|| {
    regex(
        r"(?:[0-9A-Fa-f]{1,4}:){3,7}[0-9A-Fa-f]{1,4}|(?:[0-9A-Fa-f]{1,4}:){1,6}:(?:[0-9A-Fa-f]{1,4}(?::[0-9A-Fa-f]{1,4}){0,5})?",
    )
});
static USER_AT_PLACEHOLDER: LazyLock<Regex> =
    LazyLock::new(|| regex(r"\b[A-Za-z0-9._-]+@(<(?:host|ip|private)>)"));
static LONG_KEY: LazyLock<Regex> = LazyLock::new(|| regex(r"\b[A-Za-z0-9+/_-]{32,}={0,2}"));

/// `text` with the obvious personal data replaced by placeholders such as `<email>` and `~`.
pub(crate) fn scrub_personal_data(text: &str, context: &FeedbackScrubContext) -> String {
    let mut result = text.to_string();
    // This computer's own home folders first, in whatever form they were written.
    let mut homes: Vec<&str> = context
        .home_dirs
        .iter()
        .map(|home| home.trim().trim_end_matches(['/', '\\']))
        .filter(|home| home.len() > 3)
        .collect();
    homes.sort_by_key(|home| std::cmp::Reverse(home.len()));
    for home in homes {
        for form in [
            home.to_string(),
            home.replace('\\', "/"),
            home.replace('/', "\\"),
        ] {
            let pattern = regex(&format!("(?i){}", regex::escape(&form)));
            result = pattern.replace_all(&result, "~").into_owned();
        }
    }
    result = PRIVATE_KEY
        .replace_all(&result, "<private key>")
        .into_owned();
    result = JWT.replace_all(&result, "<token>").into_owned();
    result = VENDOR_TOKEN.replace_all(&result, "<token>").into_owned();
    result = AUTH_SCHEME.replace_all(&result, "$1 <token>").into_owned();
    result = SECRET_FIELD
        .replace_all(&result, "$1$2<redacted>")
        .into_owned();
    result = URL_CREDENTIALS
        .replace_all(&result, "$1<user>@")
        .into_owned();
    result = UNIX_HOME.replace_all(&result, "~").into_owned();
    result = WINDOWS_HOME.replace_all(&result, "~").into_owned();
    // Emails, but not a JavaScript stack's `function@file.bundle:1:2`.
    result = replace_checked(&result, &EMAIL, |captures, _, after| {
        let extension = captures[1].to_ascii_lowercase();
        let followed_by_line = after.starts_with(':')
            && after[1..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit());
        (!continues_name(after)
            && !followed_by_line
            && !CODE_FILE_EXTENSIONS.contains(&extension.as_str()))
        .then(|| "<email>".to_string())
    });
    // Local and Tailscale host names carry machine and tailnet names.
    result = replace_checked(&result, &LOCAL_HOST, |_, _, after| {
        (!continues_name(after)).then(|| "<host>".to_string())
    });
    // IPv4 addresses, except loopback and the unspecified address.
    result = replace_checked(&result, &IPV4, |captures, _, _| {
        let address = &captures[0];
        (!address.starts_with("127.") && address != "0.0.0.0").then(|| "<ip>".to_string())
    });
    // IPv6 addresses, never `HH:MM:SS` times (those have no hex digit group of 3+ colons) or `::1`.
    result = replace_checked(&result, &IPV6, |captures, before, after| {
        let address = &captures[0];
        let touches_word = |c: char| c.is_alphanumeric() || c == '_' || c == ':';
        let isolated = !before.chars().next_back().is_some_and(touches_word)
            && !after.chars().next().is_some_and(touches_word);
        let looks_like_time = !address.contains("::")
            && address
                .split(':')
                .all(|group| group.len() <= 2 && group.chars().all(|c| c.is_ascii_digit()));
        (isolated
            && address != "::1"
            && address.chars().any(|c| c.is_ascii_digit())
            && !looks_like_time)
            .then(|| "<ip>".to_string())
    });
    result = USER_AT_PLACEHOLDER
        .replace_all(&result, "<user>@$1")
        .into_owned();
    // Any other long, key-like run of letters and digits (32+ characters, letters and digits).
    result = replace_checked(&result, &LONG_KEY, |captures, _, _| {
        let run = &captures[0];
        let has_digit = run.chars().any(|c| c.is_ascii_digit());
        let has_letter = run.chars().any(|c| c.is_ascii_alphabetic());
        let short_hex = run.len() <= 40 && run.chars().all(|c| c.is_ascii_hexdigit());
        (has_digit && has_letter && !short_hex).then(|| "<redacted>".to_string())
    });
    // Then the names this computer knows are private, longest first.
    let mut words: Vec<String> = context
        .private_words
        .iter()
        .map(|word| word.trim().to_string())
        .filter(|word| {
            word.chars().count() >= 3 && !PLACEHOLDER_WORDS.contains(&word.to_lowercase().as_str())
        })
        .collect();
    words.sort_by_key(|word| std::cmp::Reverse(word.len()));
    words.dedup();
    for word in words {
        let pattern = regex(&format!("(?i){}", regex::escape(&word)));
        result = replace_checked(&result, &pattern, |_, before, after| {
            let touches = |c: char| c.is_ascii_alphanumeric() || c == '<' || c == '>';
            (!before.chars().next_back().is_some_and(touches)
                && !after.chars().next().is_some_and(touches))
            .then(|| "<private>".to_string())
        });
    }
    USER_AT_PLACEHOLDER
        .replace_all(&result, "<user>@$1")
        .into_owned()
}
