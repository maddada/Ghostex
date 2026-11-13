//! The pure helpers a host asks the core directly (the phone's `MobileChatCore::query`): they
//! read state and return an answer, and change nothing.
//!
//! Serialized straight from the typed answer, never through a [`Value`]: a detour through
//! `serde_json::Value` would re-order an object's keys.

use serde_json::Value;

use crate::composer::keys::ComposerKeyEvent;
use crate::composer::queries::{
    composer_key_intent, composer_references, insert_answer_attachments, reference_menu,
    remove_chat_reference, send_blocked_toast, transcript_menu,
};
use crate::composer::references::ComposerEdit;
use crate::state::{ChatContext, ChatState};

/// Answers one pure helper, serialized as JSON text.
///
/// `None` means the arguments were not the shape the helper takes, which is a refusal rather than
/// an empty answer.
pub fn answer_query(
    state: &ChatState,
    context: &ChatContext,
    query: Query,
    arguments: &[Value],
) -> Option<String> {
    let first = arguments.first();
    Some(match query {
        Query::ComposerReferences => to_json(&composer_references(first.and_then(Value::as_str)?)),
        Query::ComposerKeyIntent => {
            let event: ComposerKeyEvent = serde_json::from_value(first?.clone()).ok()?;
            let platform = arguments.get(1).and_then(Value::as_str);
            to_json(&composer_key_intent(&event, platform))
        }
        Query::ReferenceMenu => to_json(&reference_menu(first.and_then(Value::as_str)?)),
        Query::TranscriptMenu => {
            let request = first?;
            let href = request.get("href").and_then(Value::as_str);
            let selection = request
                .get("selection")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let question_active = request.get("questionActive") == Some(&Value::Bool(true));
            to_json(&transcript_menu(href, selection, question_active))
        }
        Query::TranscriptReply => {
            let request = first?;
            let field = |name: &str| request.get(name).and_then(Value::as_str);
            to_json(&crate::composer::transcript_menu::transcript_reply_draft(
                field("draft").unwrap_or_default(),
                field("quote")?,
                field("reply").unwrap_or_default(),
            ))
        }
        Query::SendBlockedToast => {
            // The renderer passes the reason it already has; when it passes none, the helper asks
            // the composer for the current one.
            let reason = match first.and_then(Value::as_str) {
                Some(reason) => reason.to_string(),
                None => crate::composer::document::send_blocked(state, context)?,
            };
            to_json(&send_blocked_toast(&reason))
        }
        Query::InsertAnswerAttachments => to_json(&answer_attachments_edit(arguments)?),
        Query::AttachmentReferencePaths => to_json(
            &crate::composer::attachment_moves::attachment_reference_paths(
                first.and_then(Value::as_str)?,
            ),
        ),
        Query::RetargetAttachmentReferences => {
            let moved: Vec<(String, String)> =
                serde_json::from_value(arguments.get(1)?.clone()).ok()?;
            to_json(
                &crate::composer::attachment_moves::retarget_attachment_references(
                    first.and_then(Value::as_str)?,
                    &moved,
                ),
            )
        }
        Query::RemoveChatReference => to_json(&chat_reference_removal(arguments)?),
        Query::RenderVisual => render_visual(arguments)?,
        // `null` when the chat has no agent message by that id.
        Query::SpeakableSegments => to_json(&crate::read_aloud::message_segments(
            state,
            first.and_then(Value::as_str)?,
        )),
        Query::SpeakableMarkdown => to_json(&crate::read_aloud::speakable_segments(
            first.and_then(Value::as_str)?,
        )),
        Query::TranscriptPictures => to_json(&crate::transcript::pictures::picture_gallery(
            state.transcript_view.items.iter(),
            first?,
        )),
        // `null` for a key the card does not know.
        Query::ToolAppLogo => to_json(&crate::transcript::tool_apps::tool_app_svg(
            first.and_then(Value::as_str)?,
        )),
    })
}

/// `renderVisual(source, width, theme, fontFamily, progress?)`: one ```visual block laid out at
/// `width` for the phone, which paints the returned SVG (the GPUI renderers paint the same scene
/// directly). `theme` is the theme object (or its JSON text) `Theme::from_json` reads; `progress`
/// (0 to 1) asks for the chart that far through drawing in, and its absence for the finished one.
fn render_visual(arguments: &[Value]) -> Option<String> {
    let source = arguments.first()?.as_str()?;
    let width = arguments.get(1)?.as_f64()? as f32;
    let theme = match arguments.get(2) {
        Some(Value::String(text)) => text.clone(),
        Some(value) => value.to_string(),
        None => "{}".to_string(),
    };
    let font_family = arguments
        .get(3)
        .and_then(Value::as_str)
        .unwrap_or("system-ui, sans-serif");
    let progress = arguments.get(4).and_then(Value::as_f64).map(|p| p as f32);
    Some(ghostex_gx_visual::render_json_at(
        source,
        width,
        &theme,
        font_family,
        progress,
    ))
}

/// `insertAnswerAttachments(current, paths, original, start, end, others?)`, offsets in UTF-16 code
/// units; `others` (a question form's other boxes) is optional.
pub fn answer_attachments_edit(arguments: &[Value]) -> Option<ComposerEdit> {
    let paths: Vec<&str> = arguments
        .get(1)?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    Some(insert_answer_attachments(
        arguments.first()?.as_str()?,
        &paths,
        arguments.get(2)?.as_str()?,
        usize_argument(arguments, 3)?,
        usize_argument(arguments, 4)?,
        arguments.get(5).and_then(Value::as_str).unwrap_or_default(),
    ))
}

/// `removeChatReference(current, start, end)`, offsets in UTF-16 code units.
pub fn chat_reference_removal(arguments: &[Value]) -> Option<ComposerEdit> {
    Some(remove_chat_reference(
        arguments.first()?.as_str()?,
        usize_argument(arguments, 1)?,
        usize_argument(arguments, 2)?,
    ))
}

fn usize_argument(arguments: &[Value], index: usize) -> Option<usize> {
    arguments.get(index)?.as_u64().map(|value| value as usize)
}

fn to_json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

/// The pure helpers. They read state and return an answer; they change nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Query {
    /// The reference pills a draft's text carries.
    ComposerReferences,
    /// What a keystroke in the composer means.
    ComposerKeyIntent,
    /// The context menu for a reference pill.
    ReferenceMenu,
    /// The context menu for a transcript selection.
    TranscriptMenu,
    /// `transcriptReply({draft, quote, reply})`: the chat box after the Reply popover adds a reply.
    TranscriptReply,
    /// The toast text for a send the composer refused.
    SendBlockedToast,
    /// A question answer with the references for the files a paste uploaded.
    InsertAnswerAttachments,
    /// `attachmentReferencePaths(text)`: the paths of the draft's attachment references
    /// (composer/attachment_moves.rs), for the phone's copy to the session's computer at send.
    AttachmentReferencePaths,
    /// `retargetAttachmentReferences(text, [[from, to], …])`: the draft with those references
    /// pointing at the copies.
    RetargetAttachmentReferences,
    /// A draft without the reference an attachment pill stands for.
    RemoveChatReference,
    /// One ```visual block drawn for the phone: its scene as SVG, tooltip regions, or a page card.
    RenderVisual,
    /// `speakableSegments(messageId)`: what Read aloud says for one agent message.
    SpeakableSegments,
    /// `speakableMarkdown(markdown)`: the same for any Markdown (the voice agent, tests).
    SpeakableMarkdown,
    /// `transcriptPictures(picture)`: every picture the transcript shows, opened on this one
    /// (`{images, index}`), for the picture viewer's next and previous.
    TranscriptPictures,
    /// `toolAppLogo(key)`: an MCP app's logo SVG for a tool-call card's icon
    /// (transcript/tool_apps.rs), so the phone's logos ship with the core.
    ToolAppLogo,
}

impl Query {
    /// The wire spelling, which is the method name a host passes.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ComposerReferences => "composerReferences",
            Self::ComposerKeyIntent => "composerKeyIntent",
            Self::ReferenceMenu => "referenceMenu",
            Self::TranscriptMenu => "transcriptMenu",
            Self::TranscriptReply => "transcriptReply",
            Self::SendBlockedToast => "sendBlockedToast",
            Self::InsertAnswerAttachments => "insertAnswerAttachments",
            Self::AttachmentReferencePaths => "attachmentReferencePaths",
            Self::RetargetAttachmentReferences => "retargetAttachmentReferences",
            Self::RemoveChatReference => "removeChatReference",
            Self::RenderVisual => "renderVisual",
            Self::SpeakableSegments => "speakableSegments",
            Self::SpeakableMarkdown => "speakableMarkdown",
            Self::TranscriptPictures => "transcriptPictures",
            Self::ToolAppLogo => "toolAppLogo",
        }
    }

    /// Maps a method name to a helper, or `None` when it is not one of them.
    pub fn from_wire(value: &str) -> Option<Self> {
        Some(match value {
            "composerReferences" => Self::ComposerReferences,
            "composerKeyIntent" => Self::ComposerKeyIntent,
            "referenceMenu" => Self::ReferenceMenu,
            "transcriptMenu" => Self::TranscriptMenu,
            "transcriptReply" => Self::TranscriptReply,
            "sendBlockedToast" => Self::SendBlockedToast,
            "insertAnswerAttachments" => Self::InsertAnswerAttachments,
            "attachmentReferencePaths" => Self::AttachmentReferencePaths,
            "retargetAttachmentReferences" => Self::RetargetAttachmentReferences,
            "removeChatReference" => Self::RemoveChatReference,
            "renderVisual" => Self::RenderVisual,
            "speakableSegments" => Self::SpeakableSegments,
            "speakableMarkdown" => Self::SpeakableMarkdown,
            "transcriptPictures" => Self::TranscriptPictures,
            "toolAppLogo" => Self::ToolAppLogo,
            _ => return None,
        })
    }
}
