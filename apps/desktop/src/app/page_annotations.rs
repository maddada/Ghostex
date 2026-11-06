//! Annotate's Send: what a page's Agentation toolbar posts over the page bridge when the user
//! presses Send, delivered to an agent with its screenshots attached.
//!
//! CDXC:Browser 2026-11-07 DECISION:
//! User: "for g pls just fork agentation and add screenshot capturing but keep same tool and bundle it", after a Discord tester said Annotate needed manual copying and attached no screenshot ("for most other tools, they attach the screenshot or snippet of the place you highlighted"). Send goes where the Docs review's Send goes (the agent session last clicked in the sidebar of the active project, its chat box or its terminal, else the clipboard), with each screenshot saved into that session the way a pasted picture is (`/api/saveSessionChatImage`) and named in the text as `[Image #N](path)`, so the chat attaches it and a CLI agent can open the file. Nothing is sent to the agent until the user sends it.
//! SEE-ALSO: app/helpers/browser.rs (the Annotate script and its CDXC), app/docs_annotation_feedback.rs (the target and the delivery), app/browser_page_notifications.rs and app/storybook.rs (the bridges that hear Send).

use std::collections::HashMap;
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use serde_json::Value;

use crate::app::helpers::*;
use crate::*;

/// More than this many pictures in one Send are left out (the notes still go).
const MAX_SCREENSHOTS: usize = 20;
/// A picture larger than this (decoded) is left out.
const MAX_SCREENSHOT_BYTES: usize = 12 * 1024 * 1024;
const SAVE_TIMEOUT: Duration = Duration::from_secs(15);

/// One Send from the Annotate toolbar.
#[derive(Debug, PartialEq)]
pub(crate) struct PageAnnotation {
    pub(crate) markdown: String,
    /// Annotation id and PNG bytes, in the order the markdown names them.
    pub(crate) screenshots: Vec<(String, Vec<u8>)>,
}

/// Reads a page bridge message as an Annotate Send; `None` for anything else, including a
/// message without this run's token.
pub(crate) fn parse_page_annotation(message: &Value, token: &str) -> Option<PageAnnotation> {
    if message["type"] != "annotate" || message["token"].as_str() != Some(token) {
        return None;
    }
    page_annotation_from_fields(message)
}

/// The `markdown` and `screenshots` (`[{id, png}]`, PNG data URLs) of a Send, from the page bridge
/// or from the Files view's `sendPageAnnotation` request.
pub(crate) fn page_annotation_from_fields(message: &Value) -> Option<PageAnnotation> {
    let markdown = message["markdown"].as_str()?.trim().to_string();
    if markdown.is_empty() {
        return None;
    }
    let screenshots = message["screenshots"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|shot| {
            let id = shot["id"].as_str()?.to_string();
            let data = shot["png"]
                .as_str()?
                .strip_prefix("data:image/png;base64,")?;
            let bytes = BASE64_STANDARD.decode(data).ok()?;
            (bytes.len() <= MAX_SCREENSHOT_BYTES).then_some((id, bytes))
        })
        .take(MAX_SCREENSHOTS)
        .collect();
    Some(PageAnnotation {
        markdown,
        screenshots,
    })
}

/// The markdown with each screenshot reference pointing at its saved file. A line whose picture
/// was not saved is dropped, so the agent is never pointed at a file that does not exist.
pub(crate) fn page_annotation_markdown_with_paths(
    markdown: &str,
    saved: &HashMap<String, String>,
) -> String {
    let mut lines = Vec::new();
    for line in markdown.lines() {
        let Some(start) = line.find(ANNOTATE_SCREENSHOT_REFERENCE_PREFIX) else {
            lines.push(line.to_string());
            continue;
        };
        let after = &line[start + ANNOTATE_SCREENSHOT_REFERENCE_PREFIX.len()..];
        let Some(end) = after.find(')') else {
            continue;
        };
        let Some(path) = saved.get(&after[..end]) else {
            // A compact line carries the reference at its end; keep the note without it.
            let before = line[..start].trim_end();
            if let Some(kept) = before.strip_suffix(|c| c == '(').and_then(|rest| {
                rest.rfind("[Image #")
                    .map(|open| rest[..open].trim_end().to_string())
            }) {
                if !kept.is_empty() && !kept.starts_with("**Screenshot:**") {
                    lines.push(kept);
                }
            }
            continue;
        };
        lines.push(format!("{}{}{}", &line[..start], path, &after[end..]));
    }
    lines.join("\n")
}

impl GhostexGpuiApp {
    /// Takes a page bridge message if it is an Annotate Send (see the module comment); returns
    /// whether it was one.
    pub(crate) fn receive_page_annotation(
        &mut self,
        message: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if message["type"] != "annotate" {
            return false;
        }
        let Some(annotation) = parse_page_annotation(message, browser_annotate_token()) else {
            support_logs::append_for_scenario(
                support_logs::GpuiSupportLog::SidebarRefresh,
                "native.sidebar.refresh",
                "gpui.pageAnnotation.rejected",
                serde_json::json!({}),
            );
            return true;
        };
        self.deliver_page_annotation(annotation, cx);
        true
    }

    /// Saves the Send's screenshots into the target session and delivers its text there (see the
    /// module comment).
    pub(crate) fn deliver_page_annotation(
        &mut self,
        annotation: PageAnnotation,
        cx: &mut gpui::Context<Self>,
    ) {
        let rpc_target = self
            .docs_annotation_feedback_target()
            .and_then(|target| self.docs_annotation_feedback_rpc_target(target.shell_session_id));
        let (Some(rpc_target), false) = (rpc_target, annotation.screenshots.is_empty()) else {
            let content =
                page_annotation_markdown_with_paths(&annotation.markdown, &HashMap::new());
            self.deliver_docs_annotation_feedback(content, None, cx, |_, _, _| {});
            return;
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let PageAnnotation {
                markdown,
                screenshots,
            } = annotation;
            let count = screenshots.len();
            let saved = background
                .spawn(async move {
                    let mut saved = HashMap::new();
                    for (index, (id, bytes)) in screenshots.into_iter().enumerate() {
                        let result = rpc_target.rpc(
                            "/api/saveSessionChatImage",
                            serde_json::json!({
                                "base64Data": BASE64_STANDARD.encode(bytes),
                                "suggestedName": format!("annotation-{}.png", index + 1),
                            }),
                            SAVE_TIMEOUT,
                        );
                        if let Some(path) = result
                            .ok()
                            .and_then(|value| value["path"].as_str().map(str::to_string))
                        {
                            saved.insert(id, path);
                        }
                    }
                    saved
                })
                .await;
            support_logs::append_for_scenario(
                support_logs::GpuiSupportLog::SidebarRefresh,
                "native.sidebar.refresh",
                "gpui.pageAnnotation.screenshotsSaved",
                serde_json::json!({ "count": count, "saved": saved.len() }),
            );
            let content = page_annotation_markdown_with_paths(&markdown, &saved);
            let _ = this.update(cx, |this, cx| {
                this.deliver_docs_annotation_feedback(content, None, cx, |_, _, _| {});
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "run-token";

    fn png_data_url() -> String {
        format!(
            "data:image/png;base64,{}",
            BASE64_STANDARD.encode(b"\x89PNG fake")
        )
    }

    #[test]
    fn reads_a_send_with_its_screenshots() {
        let message = serde_json::json!({
            "type": "annotate",
            "token": TOKEN,
            "markdown": "## Page Feedback: /\n",
            "screenshots": [{"id": "1", "png": png_data_url()}, {"id": "2", "png": "data:text/plain;base64,AAAA"}],
        });
        let parsed = parse_page_annotation(&message, TOKEN).unwrap();
        assert_eq!(parsed.markdown, "## Page Feedback: /");
        assert_eq!(
            parsed.screenshots,
            vec![("1".to_string(), b"\x89PNG fake".to_vec())]
        );
    }

    #[test]
    fn refuses_another_token_and_other_messages() {
        let send = serde_json::json!({"type": "annotate", "token": "other", "markdown": "x"});
        assert!(parse_page_annotation(&send, TOKEN).is_none());
        let notification =
            serde_json::json!({"type": "notification", "token": TOKEN, "markdown": "x"});
        assert!(parse_page_annotation(&notification, TOKEN).is_none());
    }

    #[test]
    fn points_saved_screenshots_at_their_files_and_drops_the_rest() {
        let markdown = format!(
            "### 1. button \"Choose Team\"\n**Screenshot:** [Image #1]({p}111)\n**Feedback:** bolder\n### 2. card\n**Screenshot:** [Image #2]({p}222)\n**Feedback:** spacing\n3. **h2**: smaller [Image #3]({p}333)",
            p = ANNOTATE_SCREENSHOT_REFERENCE_PREFIX
        );
        let saved = HashMap::from([(
            "111".to_string(),
            "/home/me/shots/annotation-1.png".to_string(),
        )]);
        assert_eq!(
            page_annotation_markdown_with_paths(&markdown, &saved),
            "### 1. button \"Choose Team\"\n**Screenshot:** [Image #1](/home/me/shots/annotation-1.png)\n**Feedback:** bolder\n### 2. card\n**Feedback:** spacing\n3. **h2**: smaller"
        );
    }
}
