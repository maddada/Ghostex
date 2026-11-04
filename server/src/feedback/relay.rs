//! Posting a reviewed feedback issue to the Ghostex feedback relay (`POST {relay}/v1/feedback`),
//! which stores the images and files the issue on GitHub.

use std::time::Duration;

use base64::Engine as _;
use serde_json::{json, Value};

use super::draft::{
    github_username, utf16_len, with_reporter, FeedbackClient, FEEDBACK_BODY_MAX_UNITS,
    FEEDBACK_TITLE_MAX_UNITS,
};

/// CDXC:Feedback 2026-10-04 WHY:
/// The relay is the only holder of the GitHub credentials, so every client posts through it, and
/// it is reached from gxserver rather than from each app: the desktop, the web page and later the
/// phone all ask gxserver, which keeps the relay address and its app key in one place and spares
/// the web page a cross-origin request. `GHOSTEX_FEEDBACK_RELAY_URL` points a test at a local mock.
/// SEE-ALSO: the relay's own checkout (`ghostex-feedback-relay`, `src/validate.ts` for the limits
/// mirrored here), apps/desktop/src/app/window/feedback_modal/ (the dialog that drafts and sends),
/// server/src/ghostex_cli/feedback.rs (`ghostex feedback send`, the feedback agent's door).
const FEEDBACK_RELAY_URL: &str = "https://feedback.ghostex.dev";
const FEEDBACK_RELAY_URL_ENV: &str = "GHOSTEX_FEEDBACK_RELAY_URL";
/// The relay's `X-Ghostex-App-Key`. Not a secret: it ships inside every build and only lets the
/// relay turn away generic bots and cut off an old client by removing its key.
const FEEDBACK_RELAY_APP_KEY: &str = "gxfb_300d19442e7005ed9fdda519";
/// Five images of 5 MiB in base64 take about 35 MB; a slow uplink needs minutes for that.
const FEEDBACK_RELAY_TIMEOUT: Duration = Duration::from_secs(170);

pub(super) const FEEDBACK_MAX_IMAGES: usize = 5;
pub(super) const FEEDBACK_IMAGE_MAX_BYTES: usize = 5 * 1024 * 1024;

/// A reviewed issue, ready to post.
pub(super) struct FeedbackSubmission {
    title: String,
    body: String,
    images: Vec<FeedbackImage>,
    client: FeedbackClient,
    /// The issue was gathered and written by the feedback agent (`ghostex feedback send`).
    collected_by_agent: bool,
    /// The sender's GitHub username, sent as the relay's `reporter`.
    reporter: Option<String>,
}

pub(super) struct FeedbackImage {
    name: String,
    content_type: &'static str,
    data_base64: String,
}

impl FeedbackSubmission {
    /// Reads `{ title, body, images: [{ name, dataBase64 }], githubUsername?, reporter?,
    /// collectedByAgent? }` and checks it against the relay's limits, so a rejected send says what
    /// to fix without a round trip. `githubUsername` (`ghostex feedback send`) puts the reporter
    /// line on top of the body when it is not there yet; `reporter` (the pop-up, whose Review step
    /// already showed the line and let the user edit it) leaves the body as it is. Either becomes
    /// the relay's `reporter`.
    ///
    /// CDXC:Feedback 2026-10-16 DECISION:
    /// User: "Let the reporter's own line ping them". The relay keeps the mention of a first line
    /// that is exactly `Reported by @<reporter>` and neutralizes every other one, so every client
    /// sends `reporter` with the username. A relay from before that change ignores the field.
    /// SEE-ALSO: the relay checkout's src/issue.ts (`bodyWithReporter`) and src/validate.ts,
    /// apps/mobile/app/src/feedback/feedbackRelay.ts (the phone's copy of this request).
    pub(super) fn from_params(
        params: &serde_json::Map<String, Value>,
        client: FeedbackClient,
    ) -> Result<Self, String> {
        let text = |key: &str| {
            params
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string()
        };
        let title = text("title");
        let added = github_username(&text("githubUsername"))?;
        let body = with_reporter(&text("body"), added.as_deref());
        let reporter = match added {
            Some(name) => Some(name),
            None => github_username(&text("reporter"))?,
        };
        if title.is_empty() {
            return Err(String::from("Add a title for the issue."));
        }
        if utf16_len(&title) > FEEDBACK_TITLE_MAX_UNITS {
            return Err(format!(
                "The title can be at most {FEEDBACK_TITLE_MAX_UNITS} characters."
            ));
        }
        if body.is_empty() {
            return Err(String::from("Add a description for the issue."));
        }
        if utf16_len(&body) > FEEDBACK_BODY_MAX_UNITS {
            return Err(String::from(
                "The description can be at most 20,000 characters.",
            ));
        }
        let images = FeedbackImage::from_params(params)?;
        Ok(Self {
            title,
            body,
            images,
            client,
            collected_by_agent: params
                .get("collectedByAgent")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            reporter,
        })
    }

    /// The relay request body.
    fn payload(&self) -> Value {
        let mut payload = json!({
            "title": self.title,
            "body": self.body,
            "images": self.images.iter().map(|image| json!({
                "name": image.name,
                "contentType": image.content_type,
                "dataBase64": image.data_base64,
            })).collect::<Vec<_>>(),
            "client": self.client.to_json(),
            "collectedByAgent": self.collected_by_agent,
        });
        if let Some(reporter) = &self.reporter {
            payload["reporter"] = json!(reporter);
        }
        payload
    }

    /// What `send` would post and where, with each image's data replaced by its size, so a dry run
    /// shows the exact issue without filing it.
    pub(super) fn dry_run(&self) -> Value {
        let mut payload = self.payload();
        payload["images"] = Value::Array(
            self.images
                .iter()
                .map(|image| {
                    json!({
                        "name": image.name,
                        "contentType": image.content_type,
                        "bytes": image.data_base64.len() / 4 * 3
                            - image.data_base64.bytes().rev().take_while(|b| *b == b'=').count(),
                    })
                })
                .collect(),
        );
        json!({ "dryRun": true, "url": relay_feedback_url(), "payload": payload })
    }

    /// Posts the issue and returns its number and URL.
    pub(super) fn send(&self) -> Result<(u64, String), String> {
        let url = relay_feedback_url();
        let payload = self.payload();
        let response = crate::http_agent::answering_agent()
            .post(&url)
            .header("X-Ghostex-App-Key", FEEDBACK_RELAY_APP_KEY)
            .config()
            .timeout_global(Some(FEEDBACK_RELAY_TIMEOUT))
            .build()
            .send_json(payload);
        match response {
            Ok(response) if response.status().as_u16() >= 400 => {
                let status = response.status().as_u16();
                let retry_after = crate::http_agent::header(&response, "Retry-After")
                    .and_then(|value| value.trim().parse::<u64>().ok());
                let answer: Value = crate::http_agent::read_json(response).unwrap_or(Value::Null);
                Err(relay_error_message(status, &answer["error"], retry_after))
            }
            Ok(response) => {
                let answer: Value = crate::http_agent::read_json(response)
                    .map_err(|_| String::from("The feedback service sent an unreadable answer."))?;
                let number = answer["issueNumber"].as_u64();
                let url = answer["issueUrl"]
                    .as_str()
                    .filter(|url| url.starts_with("https://"));
                match (number, url) {
                    (Some(number), Some(url)) => Ok((number, url.to_string())),
                    _ => Err(String::from(
                        "The feedback service did not say which issue it created.",
                    )),
                }
            }
            Err(transport) => Err(format!(
                "Could not reach the feedback service. Check your internet connection and try again. ({transport})"
            )),
        }
    }
}

fn relay_feedback_url() -> String {
    let base = std::env::var(FEEDBACK_RELAY_URL_ENV)
        .ok()
        .filter(|url| !url.trim().is_empty())
        .unwrap_or_else(|| FEEDBACK_RELAY_URL.to_string());
    format!("{}/v1/feedback", base.trim().trim_end_matches('/'))
}

/// What the user reads when the relay refused the issue (its `{ error: { code, message } }`).
/// The relay's own sentence is kept where it says what to fix (a bad image); the rest are its
/// internals, so they get a sentence about what the user can do.
fn relay_error_message(status: u16, error: &Value, retry_after: Option<u64>) -> String {
    let code = error["code"].as_str().unwrap_or_default();
    let message = error["message"].as_str();
    let relay_said = |fallback: &str| {
        format!(
            "The feedback service refused the issue: {}",
            message.unwrap_or(fallback)
        )
    };
    match (status, code) {
        (429, _) | (_, "rate_limited") => rate_limit_message(
            error["limit"].as_str(),
            error["retryAfterSeconds"].as_u64().or(retry_after),
        ),
        (401, _) | (_, "unauthorized") => {
            "This version of Ghostex can no longer send feedback. Update Ghostex and try again."
                .to_string()
        }
        (413, _) | (_, "payload_too_large") => relay_said("an image is larger than 5 MB."),
        (415, _) | (_, "unsupported_media_type") => {
            relay_said("only PNG, JPEG and WebP images can be attached.")
        }
        (_, "invalid_json" | "invalid_request" | "invalid_image") | (400, _) => {
            relay_said("the issue was not in the expected shape.")
        }
        (502, _) | (_, "github_error") => github_refusal_message(error),
        (503, _) | (_, "not_configured") => {
            "The feedback service is not available yet. Please try again later.".to_string()
        }
        _ => format!("The feedback service had a problem (HTTP {status}). Please try again."),
    }
}

/// CDXC:Feedback 2026-10-12 WHY:
/// A refusal used to read only "GitHub did not accept the issue just now", which hid why GitHub said no
/// (the 2026-10-11 Windows report). The relay now retries GitHub's transient failures itself and sends
/// GitHub's own status and message (`githubStatus`, `githubMessage`), so a refusal that reaches the user
/// says what GitHub answered. A relay without those fields still gets the old sentence.
fn github_refusal_message(error: &Value) -> String {
    let status = error["githubStatus"].as_u64();
    let said = error["githubMessage"]
        .as_str()
        .map(str::trim)
        .filter(|said| !said.is_empty());
    match (status, said) {
        (Some(status), Some(said)) => {
            format!("GitHub did not accept the issue (HTTP {status}): {said}")
        }
        (Some(status), None) if status >= 500 => format!(
            "GitHub is having trouble right now (HTTP {status}). Please try again in a few minutes."
        ),
        (Some(status), None) => format!("GitHub did not accept the issue (HTTP {status})."),
        _ => "GitHub did not accept the issue just now. Please try again in a moment.".to_string(),
    }
}

/// CDXC:Feedback 2026-10-04 WHY:
/// The relay caps feedback per minute, per network per day and in total per day, and says which
/// cap was hit (`limit`). A daily cap's wait runs to midnight UTC, so it is told as "tomorrow",
/// never as a raw number of seconds; the per-minute caps (`burst`, `flood`) say how long to wait.
fn rate_limit_message(limit: Option<&str>, retry_after: Option<u64>) -> String {
    match limit {
        Some("daily_ip") => "You've sent the daily maximum of feedback from this network. You can send more tomorrow."
            .to_string(),
        Some("daily_global") => {
            "We've received a lot of feedback today. Please try again tomorrow.".to_string()
        }
        _ => match retry_after {
            Some(seconds) => format!(
                "Too many feedback submissions. Try again in {}.",
                wait_label(seconds)
            ),
            None => "Too many feedback submissions. Please try again in a minute.".to_string(),
        },
    }
}

/// `60 s`, `3 min`, `2 h`: a wait the user can read.
fn wait_label(seconds: u64) -> String {
    if seconds <= 120 {
        format!("{} s", seconds.max(1))
    } else if seconds < 3600 {
        format!("{} min", seconds.div_ceil(60))
    } else {
        format!("{} h", seconds.div_ceil(3600))
    }
}

impl FeedbackImage {
    /// The `images` list of a request: `[{ name, dataBase64 }]`, at most five.
    pub(super) fn from_params(
        params: &serde_json::Map<String, Value>,
    ) -> Result<Vec<Self>, String> {
        let raw_images = match params.get("images") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(images)) => images.clone(),
            Some(_) => return Err(String::from("images must be a list.")),
        };
        if raw_images.len() > FEEDBACK_MAX_IMAGES {
            return Err(format!("Attach at most {FEEDBACK_MAX_IMAGES} images."));
        }
        raw_images
            .iter()
            .enumerate()
            .map(|(index, image)| Self::from_param(index, image))
            .collect()
    }

    pub(super) fn name(&self) -> &str {
        &self.name
    }

    pub(super) fn extension(&self) -> &'static str {
        match self.content_type {
            "image/jpeg" => "jpg",
            "image/webp" => "webp",
            _ => "png",
        }
    }

    /// The image file's bytes; `from_param` already checked that they decode.
    pub(super) fn bytes(&self) -> Vec<u8> {
        base64::engine::general_purpose::STANDARD
            .decode(&self.data_base64)
            .unwrap_or_default()
    }

    /// The relay takes PNG, JPEG and WebP. The type is read from the bytes, not from the caller.
    fn from_param(index: usize, value: &Value) -> Result<Self, String> {
        let number = index + 1;
        let data_base64: String = value["dataBase64"]
            .as_str()
            .unwrap_or_default()
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&data_base64)
            .map_err(|_| format!("Image {number} is not valid base64."))?;
        if bytes.is_empty() {
            return Err(format!("Image {number} is empty."));
        }
        if bytes.len() > FEEDBACK_IMAGE_MAX_BYTES {
            return Err(format!("Image {number} is larger than 5 MB."));
        }
        let (content_type, extension) = sniff_image_type(&bytes)
            .ok_or_else(|| format!("Image {number} is not a PNG, JPEG or WebP image."))?;
        let name = value["name"]
            .as_str()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("screenshot-{number}.{extension}"));
        Ok(Self {
            name,
            content_type,
            data_base64,
        })
    }
}

fn sniff_image_type(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
        Some(("image/png", "png"))
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some(("image/jpeg", "jpg"))
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some(("image/webp", "webp"))
    } else {
        None
    }
}
