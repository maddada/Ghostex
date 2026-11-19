//! The quiet warning inside the chat box (box_notes.rs) for text that looks like a key or token: the first Send of
//! it stays in the box and shows the warning, and a second Send of the same text posts it
//! (CDXC:Secrets 2026-11-19 in packages/gx-chat-core/src/composer/secret_like.rs, where the rule
//! that decides what looks like a key lives).
//!
//! CDXC:Secrets 2026-11-19 WHY:
//! The check runs here, before the box empties, and not as a refusal in the chat core: a send the core refuses has already been drawn as the transcript's last row at Enter (send_watch.rs) and comes back afterwards, which would show the key in the chat for a moment. A second Send within `SECOND_SEND_AFTER` of the first does not count (a held Enter or Tab, a double click on Send), and a Retry of text the user already confirmed does not ask again.
//! SEE-ALSO: apps/mobile/app/src/chat/native/composer/SecretSendWarning.tsx and `ComposerModel.submit` in apps/mobile/app/src/chat/rust/composer.ts (the phone's warning and second Send).

use std::hash::{DefaultHasher, Hash as _, Hasher as _};
use std::time::Duration;

use super::{appearance::ChatAppearance, state::NativeChatView};
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};

/// How long after the first Send a second one is taken as the same press, not a confirmation.
const SECOND_SEND_AFTER: Duration = Duration::from_millis(500);

#[derive(Default)]
pub(super) struct SecretSendWarning {
    /// The draft (its id and revision) whose first Send was held, and when.
    held: Option<(String, u64, web_time::Instant)>,
    /// A hash of the text the user last confirmed, so a Retry of it goes without asking again.
    confirmed: Option<u64>,
}

fn text_hash(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

impl NativeChatView {
    /// Answers whether this Send stops here: the first Send of text that looks like a key is held
    /// and warned about, and a second Send of the same, unedited text, at least
    /// `SECOND_SEND_AFTER` later, goes.
    pub(super) fn hold_secret_like_send(&mut self, cx: &mut Context<Self>) -> bool {
        if !ghostex_gx_chat_core::composer::secret_like::looks_like_secret(&self.draft) {
            self.secret_send_warning.held = None;
            return false;
        }
        let hash = text_hash(&self.draft);
        if self.secret_send_warning.confirmed == Some(hash) {
            self.secret_send_warning.held = None;
            return false;
        }
        if self.secret_send_warned() {
            let too_soon = self
                .secret_send_warning
                .held
                .as_ref()
                .is_some_and(|(_, _, at)| at.elapsed() < SECOND_SEND_AFTER);
            if too_soon {
                return true;
            }
            self.secret_send_warning.held = None;
            self.secret_send_warning.confirmed = Some(hash);
            return false;
        }
        self.secret_send_warning.held = Some((
            self.draft_id.clone(),
            self.draft_revision,
            web_time::Instant::now(),
        ));
        cx.notify();
        true
    }

    /// The warning shows for the text in the box: a held Enter's repeats do not count as the
    /// second Send it asks for.
    pub(super) fn secret_send_warned(&self) -> bool {
        self.secret_send_warning
            .held
            .as_ref()
            .is_some_and(|(id, revision, _)| {
                *id == self.draft_id && *revision == self.draft_revision
            })
    }

    /// A quiet note while the chat box holds a key or a piece of one and the warning is not up: the
    /// chat core keeps this draft off gxserver, so it does not follow to other devices
    /// (`draftKeptOnDevice`, packages/gx-chat-core/src/composer/draft_sync.rs).
    pub(super) fn render_draft_kept_note(&self, p: &ChatAppearance) -> Option<AnyElement> {
        if self.snapshot["draftKeptOnDevice"] != true || self.secret_send_warned() {
            return None;
        }
        let s = p.scale;
        let words =
            "This draft stays on this device because it looks like it holds a key or token.";
        Some(
            div()
                .id("draft-kept-note")
                .role(gpui::Role::Status)
                .aria_label(words)
                .flex()
                .items_start()
                .gap(px(6.0 * s))
                .w_full()
                .px(px(4.0 * s))
                .text_size(px(12.0 * s))
                .line_height(px(16.0 * s))
                .text_color(p.muted)
                .child(
                    gpui::svg()
                        .path("titlebar/lock.svg")
                        .size(px(12.0 * s))
                        .mt(px(2.0 * s))
                        .flex_shrink_0()
                        .text_color(p.muted),
                )
                .child(div().min_w_0().whitespace_normal().child(words))
                .into_any_element(),
        )
    }

    /// The warning line, while the text whose Send was held is still in the box unedited.
    pub(super) fn render_secret_send_warning(&self, p: &ChatAppearance) -> Option<AnyElement> {
        if !self.secret_send_warned() {
            return None;
        }
        let s = p.scale;
        let accent: gpui::Hsla = gpui::rgb(if p.light { 0xe17100 } else { 0xfe9a00 }).into();
        let words = if self.secret_request_open() {
            "This looks like a key or token. Paste it into the private card above to keep it out of the chat, or send again to post it here."
        } else {
            "This looks like a key or token, and anyone who can read this chat would see it. Send again to post it anyway."
        };
        Some(
            div()
                .id("secret-send-warning")
                .role(gpui::Role::Alert)
                .aria_label(words)
                .flex()
                .items_start()
                .gap(px(6.0 * s))
                .w_full()
                .px(px(4.0 * s))
                .text_size(px(12.5 * s))
                .line_height(px(17.0 * s))
                .text_color(p.muted)
                .child(
                    gpui::svg()
                        .path("titlebar/alert-triangle.svg")
                        .size(px(13.0 * s))
                        .mt(px(2.0 * s))
                        .flex_shrink_0()
                        .text_color(accent),
                )
                .child(div().min_w_0().whitespace_normal().child(words))
                .into_any_element(),
        )
    }
}
