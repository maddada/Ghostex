use super::{appearance::ChatAppearance, markdown_links, state::NativeChatView};
use gpui::Context;
use gpui_component::input::InlineReplacement;
use serde_json::{Value, json};
use std::{ops::Range, time::Duration};

/// `0.1rem`, the distance the chat stylesheet puts between a composer pill's left edge and its
/// icon. The composer's own `1em` is `14 * scale`, matching `Input::text_size` in `composer.rs`.
const ICON_INSET_REM: f32 = 0.1;
const ROOT_FONT_PX: f32 = 16.0;
/// The chat box's own text size, unscaled.
const COMPOSER_FONT_PX: f32 = 14.0;
const SESSION_ICON_PX: f32 = 12.0;

/// One markdown reference in the draft, resolved to the draft's byte offsets.
///
/// CDXC:SessionChat 2026-09-18 SEE-ALSO:
/// The rules live in `packages/gx-chat-core/src/composer/reference_pills.rs` and reach this
/// file through `nativeChat.composerReferences`; the input side is
/// `.dependencies/gpui-component/crates/base/src/input/base/inline_replacement.rs`.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ComposerReference {
    /// Byte range of `[label](path)` inside the draft.
    pub(super) range: Range<usize>,
    pub(super) kind: String,
    /// The reference's own label, `Image #1` for a pasted picture.
    pub(super) label: String,
    pub(super) path: String,
    /// The visible text, already padded for the icon and truncated to the shared label width.
    pub(super) pill: String,
    pub(super) revealed: bool,
}

/// Byte offset of every UTF-16 code-unit boundary in `draft`, so JS string indices land on chars.
fn utf16_boundaries(draft: &str) -> Vec<(usize, usize)> {
    let mut boundaries = Vec::with_capacity(draft.len() + 1);
    let mut utf16 = 0;
    for (byte, character) in draft.char_indices() {
        boundaries.push((utf16, byte));
        utf16 += character.len_utf16();
    }
    boundaries.push((utf16, draft.len()));
    boundaries
}

fn byte_offset(boundaries: &[(usize, usize)], utf16: usize) -> Option<usize> {
    boundaries
        .binary_search_by_key(&utf16, |(units, _)| *units)
        .ok()
        .map(|index| boundaries[index].1)
}

/// Decode the shared parser's answer for `draft`.
pub(super) fn parse(draft: &str, parsed: &Value) -> Vec<ComposerReference> {
    let boundaries = utf16_boundaries(draft);
    parsed
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|reference| {
            let start = byte_offset(&boundaries, reference["start"].as_u64()? as usize)?;
            let end = byte_offset(&boundaries, reference["end"].as_u64()? as usize)?;
            (start < end).then_some(ComposerReference {
                range: start..end,
                kind: reference["kind"].as_str()?.to_owned(),
                label: reference["label"].as_str()?.to_owned(),
                path: reference["path"].as_str()?.to_owned(),
                pill: reference["pill"].as_str()?.to_owned(),
                revealed: reference["revealed"] == true,
            })
        })
        .collect()
}

/// Turn the draft's references into the input's display projection.
///
/// CDXC:SessionChat 2026-11-18 DECISION:
/// User, of the Reply popover's picture pill ("pls in this reply the icon is too close to 'Image'
/// text"): the pill's room for its icon is spaces in the core's pill text, so it grows and shrinks
/// with the input's font, while the icon was always sized for the chat box's 14px. In the 13px
/// reply field the icon filled that room and touched the label. `font_px` is the unscaled size of
/// the input the pills are drawn in, and the icon follows it, which keeps the chat box's gap in
/// every field that draws pills.
pub(super) fn replacements(
    references: &[ComposerReference],
    appearance: &ChatAppearance,
    font_px: f32,
) -> Vec<InlineReplacement> {
    references
        .iter()
        .filter(|reference| !reference.revealed)
        .filter_map(|reference| {
            let color = markdown_links::composer_color(&reference.kind, appearance)?;
            // A mention of another session (gx-chat-core `session_mentions.rs`) leads with that
            // session's agent logo, as its pill does in the transcript.
            let mut icon_size = font_px * markdown_links::VISUAL.icon_em;
            let icon = if reference.kind == "session" {
                // An agent logo fills its whole box where the outline icons above keep a margin,
                // so at the full 1em it touched the title. It draws at the sent pill's 12px
                // (`session_pill_link`), which leaves the same gap before the title.
                icon_size = SESSION_ICON_PX;
                let known = crate::app::gx_chat::session_directory::find(&reference.path);
                let agent = known
                    .as_ref()
                    .map_or("", |session| session.agent_id.as_str());
                markdown_links::session_pill_icon(
                    ghostex_gx_chat_core::extras::agents::default_agent_by_id(Some(agent))
                        .map_or(agent, |(icon, _)| icon),
                    known.as_ref().is_some_and(|session| session.coordinator),
                    appearance,
                )
                .0
                .to_string()
            } else if reference.kind == "board" {
                // A board item leads with the Board pill's icon, as its pill does when sent.
                icon_size = SESSION_ICON_PX;
                markdown_links::BOARD_ICON.to_string()
            } else {
                format!("chat-references/{}.svg", reference.kind)
            };
            if reference.kind == "session" {
                return Some(session_replacement(reference, &icon, appearance));
            }
            Some(
                InlineReplacement::new(reference.range.clone(), reference.pill.clone())
                    .color(color)
                    .icon(
                        icon,
                        gpui::px(icon_size * appearance.scale),
                        gpui::px(ICON_INSET_REM * ROOT_FONT_PX * appearance.scale),
                    )
                    // No pill changes the cursor (see native_chat/cursor.rs): a reference inside
                    // the composer keeps the input's own text cursor, and clicking it still
                    // opens what it points at.
                    .pointer(false)
                    // CDXC:SessionChat 2026-09-20 DECISION:
                    // User: hovering a composer pill shows the path, URL, or whatever else it
                    // points at, because the pill hides the markdown destination. React shows the
                    // same thing from `session-chat-lexical-input.tsx`.
                    .tooltip(if reference.kind == "sideChat" {
                        "Side chat: sent to the agent as a side question (/btw). Click to remove."
                            .to_string()
                    } else if reference.kind == "comment" {
                        // A review comment's destination is its encoded payload, not a place.
                        ghostex_gx_chat_core::composer::review_comments::ReviewComment::from_href(
                            &reference.path,
                        )
                        .map(|comment| comment.tooltip())
                        .unwrap_or_default()
                    } else if reference.kind == "answer" {
                        // The question it answers, by its title (never its tags).
                        ghostex_gx_chat_core::questions::answer_pills::tooltip(&reference.label)
                    } else if reference.kind == "message" {
                        // One of the user's earlier messages (gx-chat-core `message_numbers.rs`).
                        format!(
                            "Message {}: its start is quoted for the agent",
                            reference.label
                        )
                    } else if reference.kind == "board" {
                        format!(
                            "{} on the board",
                            reference
                                .path
                                .strip_prefix("board:")
                                .unwrap_or(&reference.path)
                        )
                    } else {
                        reference.path.clone()
                    }),
            )
        })
        .collect()
}

/// A mention of another session in the chat box: the sent mention's soft chip (`session_pill_link`
/// in markdown_links.rs), a faint fill with rounded corners and no border, holding the computer's
/// icon for a session on another computer, the agent's logo in its colour and the title, cut with
/// an ellipsis past `SESSION_TITLE_MAX_CHARACTERS`; no state dot and no underline. The box stays
/// `SESSION_CHIP_MARGIN` inside the replacement on both sides, so it never touches the words beside
/// it. It stays one piece for the caret and Delete, as every replacement does. Hovering it shows the
/// session's hover card, which the card builder finds by the Global Ref the tooltip carries. The
/// user's decision is CDXC:SessionChat 2026-11-16 on `mention_card` in gx-chat-core. The input lays
/// the replacement out as text, so the room for the margin, padding and icons is no-break spaces
/// (about a quarter of the 14px line each) around the title.
fn session_replacement(
    reference: &ComposerReference,
    icon: &str,
    appearance: &ChatAppearance,
) -> InlineReplacement {
    let s = appearance.scale;
    let known = crate::app::gx_chat::session_directory::find(&reference.path);
    let (_, icon_color) = markdown_links::session_pill_icon(
        known.as_ref().map_or("", |session| {
            ghostex_gx_chat_core::extras::agents::default_agent_by_id(Some(&session.agent_id))
                .map_or(session.agent_id.as_str(), |(icon, _)| icon)
        }),
        known.as_ref().is_some_and(|session| session.coordinator),
        appearance,
    );
    let marks = super::session_card::card(&reference.path)
        .map(|card| super::session_card::marks(&card, appearance))
        .unwrap_or_default();
    let gap = 4.0;
    let margin = markdown_links::SESSION_CHIP_MARGIN;
    let padding = 5.0;
    let computer: f32 = marks
        .iter()
        .map(|mark| match mark {
            super::session_card::PillMark::Icon { size, .. } => f32::from(*size) / s + gap,
        })
        .sum();
    let spaces = |room: f32| "\u{a0}".repeat((room / NBSP_PX).ceil() as usize);
    let icon_inset = margin + padding + computer;
    // The logo, its gap to the title and the room before it, in place of the core's own icon room.
    let lead = spaces(icon_inset + SESSION_ICON_PX + gap);
    let text = format!(
        "{lead}{}{}",
        session_title(&reference.label),
        spaces(padding + margin)
    );
    InlineReplacement::new(reference.range.clone(), text)
        .color(appearance.foreground)
        .font_weight(gpui::FontWeight::MEDIUM)
        .icon(
            icon.to_string(),
            gpui::px(SESSION_ICON_PX * s),
            gpui::px(icon_inset * s),
        )
        .icon_color(icon_color)
        .frame(gpui_component::input::InlineReplacementFrame {
            border_color: gpui::transparent_black(),
            border_width: gpui::px(0.0),
            background: Some(appearance.foreground.opacity(0.08)),
            hover_background: Some(appearance.foreground.opacity(0.15)),
            radius: gpui::px(6.0 * s),
            inset_y: gpui::px(1.0 * s),
            inset_x: gpui::px(margin * s),
            padding_left: gpui::px(padding * s),
            padding_right: gpui::px(padding * s),
            gap: gpui::px(gap * s),
        })
        .leading(
            marks
                .into_iter()
                .map(super::session_card::PillMark::into_replacement_mark)
                .collect(),
        )
        .pointer(false)
        .tooltip(reference.path.clone())
}

/// About how wide one no-break space is in the 14px chat box, unscaled.
const NBSP_PX: f32 = 3.6;

/// The most of a session's title a chip in the chat box shows; a longer one ends in `…`.
const SESSION_TITLE_MAX_CHARACTERS: usize = 30;

/// A session's title as its chip shows it: cut to `SESSION_TITLE_MAX_CHARACTERS` with `…`, its
/// spaces no-break so the chip never wraps.
fn session_title(label: &str) -> String {
    let label = label.trim();
    let title: String = if label.chars().count() > SESSION_TITLE_MAX_CHARACTERS {
        let cut: String = label
            .chars()
            .take(SESSION_TITLE_MAX_CHARACTERS - 1)
            .collect();
        format!("{}\u{2026}", cut.trim_end())
    } else {
        label.to_string()
    };
    title.replace(' ', "\u{a0}")
}

/// The draft with `·` inserted before the reference's destination, and the caret that follows it.
///
/// CDXC:SessionChat 2026-09-18 DECISION:
/// User (2026-09-09, React composer): a double click expands a pill back into its editable
/// markdown source. The marker is what suppresses the pill, so the source stays visible for as
/// long as it is there; the shared parser skips any label that ends with it.
pub(super) fn revealed(draft: &str, reference: &ComposerReference) -> Option<(String, usize)> {
    let source = draft.get(reference.range.clone())?;
    let label_end = source.find("](")?;
    let split = reference.range.start + label_end;
    let mut next = String::with_capacity(draft.len() + 2);
    next.push_str(&draft[..split]);
    next.push('\u{b7}');
    next.push_str(&draft[split..]);
    let caret = next[..split + '\u{b7}'.len_utf8()].encode_utf16().count();
    Some((next, caret))
}

impl NativeChatView {
    /// Keep the composer input's reference pills in step with the draft it is about to paint.
    ///
    /// CDXC:SessionChat 2026-10-07 WHY:
    /// The pills and the image tiles above the input are parsed here, in the paint, by the core's own pure parser (`composer_references` in packages/gx-chat-core/src/composer/queries.rs, the same rule the runtime's `composerReferences` query answers with). Supersedes asking the chat runtime thread with a 40ms timeout and keeping the last answer when it timed out: right after Enter that thread is busy with the send, so the sent message's thumbnails stayed above the emptied input for about a second.
    pub(super) fn sync_composer_references(
        &mut self,
        appearance: &ChatAppearance,
        cx: &mut Context<Self>,
    ) {
        if self.composer_reference_draft.as_deref() != Some(self.draft.as_str()) {
            let draft = self.draft.clone();
            let parsed = serde_json::to_value(
                ghostex_gx_chat_core::composer::queries::composer_references(&draft),
            )
            .unwrap_or(Value::Null);
            self.composer_references = parse(&draft, &parsed);
            self.composer_reference_draft = Some(draft);
            // Editing the draft moves every reference, so a click that has not opened yet no
            // longer refers to what the user pressed.
            self.cancel_composer_reference_open();
            self.composer_image_hover = None;
        }
        let replacements = replacements(&self.composer_references, appearance, COMPOSER_FONT_PX);
        if let Some(input) = self.input.clone() {
            input.update(cx, |input, cx| {
                input.set_inline_replacements(replacements, cx)
            });
        }
    }

    /// Handle a press on a composer reference pill. Returns whether one was hit.
    pub(super) fn click_composer_reference(
        &mut self,
        event: &gpui::MouseDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        // React dropped a pending open on any press inside the composer, so a press that is not on
        // the same pill can never be followed by the earlier one opening a view.
        self.cancel_composer_reference_open();
        let Some(input) = self.input.clone() else {
            return false;
        };
        let Some(range) = input.read(cx).inline_replacement_at(event.position) else {
            return false;
        };
        let Some(reference) = self
            .composer_references
            .iter()
            .find(|reference| reference.range == range)
            .cloned()
        else {
            return false;
        };
        // The Side Chat pill has nothing to open; a click takes it off, as the tooltip says.
        if reference.kind == "sideChat" {
            let draft = self.draft.clone();
            self.invoke(json!({"type":"toggleSideChat","text":draft}), cx);
            return true;
        }
        // A review comment pill has nothing to open; it goes to the agent when the message is sent.
        if reference.kind == "comment" {
            return true;
        }
        if event.click_count >= 2 {
            if let Some((draft, caret)) = revealed(&self.draft, &reference) {
                self.insert_prompt(&draft, cx);
                self.input_caret = Some(caret);
            }
            return true;
        }
        // CDXC:SessionChat 2026-09-19 DECISION:
        // User: a click on a composer pill does exactly what it does in the React composer
        // (`use-session-chat-reference-interactions.ts`). An image pill opens the picture preview
        // (user, 2026-09-09); any other pill whose destination is a local path opens through the
        // transcript's Code/Docs flow (user, 2026-09-16); a web link does nothing. Opening waits
        // out the double-click window so the view cannot swallow the second click that expands
        // the reference source.
        let image = reference.kind == "image";
        let click = self.composer_reference_click;
        self.composer_reference_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(500))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.composer_reference_click != click {
                    return;
                }
                this.composer_reference_task = None;
                // React's `pill.isConnected`: the reference has to still be in the draft.
                if !this.composer_references.contains(&reference) {
                    return;
                }
                if image {
                    let (images, index) = this.composer_image_gallery(&reference.path);
                    this.open_image_viewer(images, index, cx);
                } else {
                    this.invoke(
                        json!({"type":"openComposerReference","href":reference.path}),
                        cx,
                    );
                }
            });
        }));
        true
    }

    /// Drop a pill open that is still waiting out the double-click window.
    pub(super) fn cancel_composer_reference_open(&mut self) {
        self.composer_reference_click += 1;
        self.composer_reference_task = None;
    }

    /// Track the image pill under the pointer, which outlines its thumbnail.
    pub(super) fn hover_composer_reference(
        &mut self,
        position: Option<gpui::Point<gpui::Pixels>>,
        cx: &mut Context<Self>,
    ) {
        let hovered = position
            .zip(self.input.as_ref())
            .and_then(|(position, input)| input.read(cx).inline_replacement_at(position))
            .and_then(|range| {
                self.composer_references
                    .iter()
                    .find(|reference| reference.range == range && reference.kind == "image")
            })
            .map(|reference| reference.path.clone());
        if self.composer_image_hover != hovered {
            self.composer_image_hover = hovered;
            cx.notify();
        }
    }

    /// The image reference the caret sits in or against, edges included.
    pub(super) fn composer_caret_image(&self, cx: &gpui::App) -> Option<String> {
        if self.composer_reference_draft.as_deref() != Some(self.draft.as_str()) {
            return None;
        }
        let caret = self.input.as_ref()?.read(cx).cursor();
        self.composer_references
            .iter()
            .find(|reference| {
                reference.kind == "image"
                    && caret >= reference.range.start
                    && caret <= reference.range.end
            })
            .map(|reference| reference.path.clone())
    }

    /// The thumbnail to outline.
    ///
    /// CDXC:SessionChat 2026-09-19 DECISION:
    /// User (2026-09-09, React composer): hovering or clicking an image reference, or placing the
    /// caret in it, outlines its preview image in white. A click leaves the caret against the
    /// pill, so the caret rule covers it here.
    pub(super) fn composer_active_image(&self, cx: &gpui::App) -> Option<String> {
        self.composer_image_hover
            .clone()
            .or_else(|| self.composer_caret_image(cx))
    }
}
