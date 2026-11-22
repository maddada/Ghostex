//! The issue a feedback message becomes, and the client line the relay stamps under it.

use std::sync::OnceLock;

use serde_json::{json, Value};

/// The relay's limits on the issue text (`LIMITS` in the relay's `src/validate.ts`), counted in
/// UTF-16 code units like the relay's JavaScript `length`.
pub(super) const FEEDBACK_TITLE_MAX_UNITS: usize = 120;
pub(super) const FEEDBACK_BODY_MAX_UNITS: usize = 20_000;
const FEEDBACK_VERSION_MAX_CHARS: usize = 64;
const FEEDBACK_OS_MAX_CHARS: usize = 100;

/// Who sent the feedback, as the relay's `client` object.
pub(super) struct FeedbackClient {
    pub(super) app: &'static str,
    pub(super) version: String,
    pub(super) os: String,
}

impl FeedbackClient {
    /// The client for `app` (`desktop`, `web` or `mobile`) on this machine. Every client ships in
    /// the same bundle as this gxserver, so the version is gxserver's own.
    pub(super) fn for_app(app: Option<&str>) -> Result<Self, String> {
        let app = match app.map(str::trim) {
            Some("desktop") => "desktop",
            Some("web") => "web",
            Some("mobile") => "mobile",
            _ => return Err("app must be desktop, web or mobile.".to_string()),
        };
        Ok(Self {
            app,
            version: feedback_version(),
            os: feedback_os().to_string(),
        })
    }

    pub(super) fn to_json(&self) -> Value {
        json!({ "app": self.app, "version": self.version, "os": self.os })
    }

    /// The line the relay writes under the issue (`buildIssueBody` in the relay's `src/issue.ts`),
    /// so the review step can show it before anything is posted.
    pub(super) fn footer(&self) -> String {
        let app = match self.app {
            "desktop" => "Desktop",
            "web" => "Web",
            _ => "Mobile",
        };
        format!(
            "Sent from Ghostex feedback · {app} {} · {}",
            self.version, self.os
        )
    }
}

/// The relay accepts `[0-9A-Za-z.+_-]` for the version.
fn feedback_version() -> String {
    let version: String = crate::telemetry::base::SERVER_MARKETING_VERSION
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || ".+_-".contains(*character))
        .take(FEEDBACK_VERSION_MAX_CHARS)
        .collect();
    if version.is_empty() {
        "unknown".to_string()
    } else {
        version
    }
}

/// `macOS 15.1 (arm64)`, `Windows 10.0.26300 (x86_64)`, `Linux 6.8.0-45-generic (x86_64)`, kept to
/// the characters the relay accepts. Read once: probing the version spawns a process.
fn feedback_os() -> &'static str {
    static OS: OnceLock<String> = OnceLock::new();
    OS.get_or_init(|| {
        let name = match std::env::consts::OS {
            "macos" => "macOS",
            "windows" => "Windows",
            "linux" => "Linux",
            other => other,
        };
        let version = crate::telemetry::base::read_raw_os_version()
            .map(|version| format!(" {}", version.trim()))
            .unwrap_or_default();
        let raw = format!("{name}{version} ({})", std::env::consts::ARCH);
        raw.chars()
            .filter(|character| {
                character.is_ascii_alphanumeric() || " .,()/+_-".contains(*character)
            })
            .take(FEEDBACK_OS_MAX_CHARS)
            .collect::<String>()
            .trim()
            .to_string()
    })
}

/// The issue title a message starts as: its first non-empty line, whitespace collapsed, cut at a
/// word boundary with an ellipsis when it does not fit the relay's title limit.
pub(super) fn draft_title(message: &str) -> String {
    let first_line = message
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    if utf16_len(&first_line) <= FEEDBACK_TITLE_MAX_UNITS {
        return first_line;
    }
    let mut cut = String::new();
    for word in first_line.split(' ') {
        let candidate = if cut.is_empty() {
            word.to_string()
        } else {
            format!("{cut} {word}")
        };
        if utf16_len(&candidate) + 1 > FEEDBACK_TITLE_MAX_UNITS {
            break;
        }
        cut = candidate;
    }
    if cut.is_empty() {
        // One word longer than the limit: cut it by characters.
        cut = first_line
            .chars()
            .scan(0, |units, character| {
                *units += character.len_utf16();
                (*units < FEEDBACK_TITLE_MAX_UNITS).then_some(character)
            })
            .collect();
    }
    format!("{cut}…")
}

/// The GitHub username in `raw`, without its `@`: `Ok(None)` when it is empty, `Err` when it is
/// not a GitHub username (letters, digits and single hyphens, no hyphen at either end, up to 39
/// characters). The pop-up checks the same shape (`github_username` in
/// apps/desktop/src/app/window/feedback_modal/state.rs).
pub(super) fn github_username(raw: &str) -> Result<Option<String>, String> {
    let name = raw.trim();
    let name = name.strip_prefix('@').unwrap_or(name);
    if name.is_empty() {
        return Ok(None);
    }
    let valid = name.len() <= 39
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--");
    if valid {
        Ok(Some(name.to_string()))
    } else {
        Err(format!("\"{name}\" is not a GitHub username."))
    }
}

/// The issue's first line, which mentions the reporter so GitHub notifies them about the issue.
pub(super) fn reporter_line(name: &str) -> String {
    format!("Reported by @{name}")
}

/// `body` with the reporter line on top, unless it is already there.
pub(super) fn with_reporter(body: &str, name: Option<&str>) -> String {
    let body = body.trim();
    match name {
        Some(name) if body.lines().next().map(str::trim) != Some(reporter_line(name).as_str()) => {
            format!("{}\n\n{body}", reporter_line(name))
        }
        _ => body.to_string(),
    }
}

pub(super) fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}
