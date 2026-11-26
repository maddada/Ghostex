use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::hotkeys::{
    GPUI_DEFAULT_GHOSTEX_HOTKEYS, gpui_configured_hotkey_label, gpui_keystroke_from_shared_hotkey,
    gpui_migrated_hotkey_for_action, gpui_platform_hotkey_for_action,
};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, Context, EntityInputHandler as _, Focusable as _, FontWeight,
    InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled, Window,
    div, px,
};
use gpui_component::tooltip::ManagedTooltipPlacement;
use serde::Deserialize;
use std::{collections::HashSet, sync::LazyLock};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScrollBottom {
    label: String,
    edge_threshold: f32,
    height: f32,
    font_size: f32,
    padding_x: f32,
    bottom: f32,
    error_tone: ErrorTone,
    question_tone: ErrorTone,
    pill_gap: f32,
    question_flash_ms: u64,
    question_jump_margin: f32,
}

#[derive(Deserialize)]
struct ErrorTone {
    dark: ToneColors,
    light: ToneColors,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToneColors {
    background: String,
    border: String,
    border_alpha: f32,
    text: String,
}

static SPEC: LazyLock<ScrollBottom> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../../../packages/gx-chat-core/visual/scroll-bottom.json"
    ))
    .expect("shared scroll button appearance")
});

pub(super) fn label() -> &'static str {
    &SPEC.label
}

pub(super) fn font_size() -> f32 {
    SPEC.font_size
}

/// How close to the transcript's end, unscaled, still counts as the bottom.
pub(super) fn edge_threshold() -> f32 {
    SPEC.edge_threshold
}

/// How long a jump's highlight stays on the question it landed on.
pub(super) fn question_flash_ms() -> u64 {
    SPEC.question_flash_ms
}

/// How far below the transcript's top a jump puts the question it lands on.
pub(super) fn question_jump_margin() -> f32 {
    SPEC.question_jump_margin
}

/// CDXC:SessionChat 2026-09-29 DECISION:
/// User: "remove the hotkey shown for the scroll to bottom indicator in the gpui app on desktop but show it in the tooltip by itself when i hover on it." The floating pill reads only "Scroll to bottom"; hovering it shows the configured shortcut alone, above the pill so it never covers the composer. The working row's icon pill (chat_pills.rs, 2026-10-15) has no text, so its tooltip names it before the shortcut.
pub(super) const TOOLTIP_PLACEMENT: ManagedTooltipPlacement =
    ManagedTooltipPlacement::Preferred(gpui_component::Placement::Top);

/// The configured shortcut's label, or `None` when the action has no hotkey.
pub(super) fn shortcut_label() -> Option<String> {
    gpui_configured_hotkey_label("scrollChatToBottom").filter(|label| !label.is_empty())
}

#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(super) struct ScrollChatToBottom {
    shortcut: String,
}

#[derive(Default)]
struct RegisteredShortcuts(HashSet<String>);
impl gpui::Global for RegisteredShortcuts {}

fn shortcut() -> Option<String> {
    let action = "scrollChatToBottom";
    let (_, default) = GPUI_DEFAULT_GHOSTEX_HOTKEYS
        .iter()
        .find(|(id, _)| *id == action)?;
    let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
    let key = settings
        .object()
        .get("hotkeys")
        .and_then(|hotkeys| hotkeys.get(action))
        .and_then(serde_json::Value::as_str);
    let key = gpui_migrated_hotkey_for_action(action, key.unwrap_or(default), default);
    gpui_keystroke_from_shared_hotkey(gpui_platform_hotkey_for_action(action, key))
}

pub(super) fn register(cx: &mut App) {
    let Some(shortcut) = shortcut() else { return };
    if !cx.has_global::<RegisteredShortcuts>() {
        cx.set_global(RegisteredShortcuts::default());
    }
    if cx
        .global_mut::<RegisteredShortcuts>()
        .0
        .insert(shortcut.clone())
    {
        // Bound in the focused field's context too, the way search and zoom are: a binding on
        // `NativeChat` alone loses by depth to any binding of the same chord on the chat box.
        let bindings = ["NativeChat", "NativeChat > Input"].map(|context| {
            crate::app::hotkeys::gpui_runtime_key_binding(
                &shortcut,
                ScrollChatToBottom {
                    shortcut: shortcut.clone(),
                },
                Some(context),
                cx,
            )
        });
        cx.bind_keys(bindings);
    }
}

impl NativeChatView {
    /// Keep a focused question form field at its screen position while transcript rows stream.
    /// The form inputs are the text fields inside the transcript; the session note and docked
    /// question fields live beside the composer and do not move with this list.
    ///
    /// CDXC:SessionChat 2026-11-02 DECISION: User: "while focus is inside a transcript element (a questions form's fields, a note field, an inline page question), streaming content must not move that element on screen; when focus leaves, normal following resumes." Tail following pins the bottom and slides a focused form under its caret as new rows arrive, so focus pauses following at the current list position. Leaving the field resumes following when the list was following at focus; a reader who was scrolled up, or who scrolled while typing, keeps their position until they return to the bottom.
    pub(super) fn sync_transcript_focus_anchor(&mut self, window: &Window, cx: &App) {
        let focused = self
            .form_inputs
            .values()
            .any(|field| field.input.read(cx).focus_handle(cx).is_focused(window));
        if focused {
            let following = self.list.is_following_tail();
            // A reader already scrolled up is still in GPUI's Tail mode, which can re-engage when a
            // layout reaches the end. Focus holds Normal mode either way.
            if self.transcript_focus_follow.get().is_none() {
                self.transcript_focus_follow.set(Some(following));
                self.list.set_follow_mode(gpui::FollowMode::Normal);
            } else if following {
                self.list.set_follow_mode(gpui::FollowMode::Normal);
            }
        } else if let Some(resume) = self.transcript_focus_follow.take() {
            self.transcript_focus_field_top.set(None);
            // A disclosure that is still moving owns the handoff back to Tail mode.
            if self.disclosure_motion.borrow().follow_paused {
                return;
            }
            // An answer clicked meanwhile keeps the list still (answer_anchor.rs).
            if self.answer_hold_takes_focus_resume(resume) {
                return;
            }
            let top = self.list.logical_scroll_top();
            self.list.set_follow_mode(gpui::FollowMode::Tail);
            if !resume {
                self.list.scroll_to(top);
            }
        }
    }

    /// Paints nothing; placed after the transcript list so its paint reads where the focused form
    /// field was just painted. Normal mode keeps the anchored row still, but rows can still move
    /// the field: OpenCode regroups the finished turn into a summary row when the next one starts,
    /// and that row keeps changing above the form while the turn streams. So the field's own top is
    /// held: a frame that painted it elsewhere scrolls the list back by the difference. A wheel
    /// scroll re-records it (`follow_tail_if_wheel_reaches_end`), and so does the reveal that
    /// scrolls a growing box into view (`form_input_reveal_sync`).
    pub(super) fn focused_field_hold_sync(&self) -> AnyElement {
        let list = self.list.clone();
        let held = self.transcript_focus_field_top.clone();
        let inputs: Vec<_> = self
            .form_inputs
            .values()
            .map(|entry| entry.input.clone())
            .collect();
        gpui::canvas(
            |_, _, _| {},
            move |_, _, window, cx| {
                let Some(top) = inputs.iter().find_map(|input| {
                    let state = input.read(cx);
                    state
                        .focus_handle(cx)
                        .is_focused(window)
                        .then(|| state.input_bounds().top())
                }) else {
                    held.set(None);
                    return;
                };
                match held.get() {
                    Some(want) if (top - want).abs() > px(0.5) => {
                        list.scroll_by(top - want);
                        window.request_animation_frame();
                    }
                    Some(_) => {}
                    None => held.set(Some(top)),
                }
            },
        )
        .absolute()
        .size_0()
        .into_any_element()
    }

    pub(super) fn jump_to_bottom(&mut self, cx: &mut Context<Self>) {
        self.release_answer_hold();
        self.clear_message_hover();
        self.list.set_follow_mode(gpui::FollowMode::Tail);
        self.expand_composer(false, cx);
        cx.notify();
    }

    pub(super) fn scroll_bottom_action(
        &mut self,
        action: &ScrollChatToBottom,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if shortcut().as_deref() != Some(action.shortcut.as_str()) {
            cx.propagate();
            return;
        }
        for input in self
            .input
            .iter()
            .chain(self.answer_input.iter().map(|(_, input)| input))
            .chain(self.async_answer_input.iter().map(|(_, input)| input))
        {
            if input.read(cx).focus_handle(cx).is_focused(window)
                && input.update(cx, |input, cx| {
                    input.marked_text_range(window, cx).is_some()
                })
            {
                cx.propagate();
                return;
            }
        }
        if self.composer_held_key.as_deref() != Some("scroll-to-bottom") {
            self.composer_held_key = Some("scroll-to-bottom".into());
            self.jump_to_bottom(cx);
        }
        cx.stop_propagation();
        window.prevent_default();
    }

    /// The toast that takes the pill's place: "Press Escape again to interrupt" while a first Escape
    /// waits, then the red "Agent was interrupted" (`interrupt` in
    /// `packages/gx-chat-core/src/composer/send.rs`).
    ///
    /// CDXC:SessionChat 2026-10-18 DECISION:
    /// User (2026-10-01): the Escape toast and the scroll-to-bottom pill must be centred on each other, in "exact same style", with the Escape one hiding the scroll one while it shows; the red "Agent was interrupted" one takes the same spot and look. The toasts take that slot and shape (in-window and in the frosted window), visual only: no click, hover, focus or tooltip. User (2026-10-15): the scroll-to-bottom pill left that slot for an icon pill on the working row (chat_pills.rs), so the toasts keep the floating slot on their own. User (2026-10-17, the working card): that pill rides on the working card. User (2026-10-18): the card sits in its own row right above the composer, outside the transcript, so the toasts keep the slot at the transcript's bottom edge, just above the card (supersedes the 2026-10-17 lift over a card pinned inside the transcript).
    pub(super) fn pill_toast(&self) -> Option<PillToast> {
        let toast = &self.snapshot["interruptToast"];
        Some(PillToast {
            text: toast["text"].as_str()?.to_string(),
            error: toast["tone"] == "error",
        })
    }

    /// A downward wheel that lands within the pill's edge threshold of the end has reached the
    /// bottom: the transcript follows its tail again. Called from the transcript's capture-phase
    /// wheel listener, before the list applies the same event, which keeps following on a downward
    /// scroll.
    pub(super) fn follow_tail_if_wheel_reaches_end(&self, event: &gpui::ScrollEvent, scale: f32) {
        // The reader moved the list on purpose: hold the focused field where it lands, and keep
        // the position they chose when the field loses focus.
        self.transcript_focus_field_top.set(None);
        if self.transcript_focus_follow.get().is_some() {
            self.transcript_focus_follow.set(Some(false));
        }
        self.release_answer_hold();
        if self.list.is_following_tail() || self.disclosure_motion.borrow().follow_paused {
            return;
        }
        // gpui's list turns wheel lines into 20px each (`ListElement::paint`).
        let delta = event.delta.pixel_delta(px(20.0)).y;
        if delta < px(0.0)
            && transcript_distance_to_end(&self.list, &self.spliced_height) + delta
                <= px(SPEC.edge_threshold * scale)
        {
            self.list.set_follow_mode(gpui::FollowMode::Tail);
        }
    }

    /// Before rows arrive (the core's splice, a message just sent, the status row): a reader within
    /// the pill's edge threshold of the end follows them, as the phone's list does. Nothing moves a
    /// reader a hold keeps in place (a clicked answer, a focused form field, a disclosure in motion,
    /// a long reply stopped at its top) or one dragging the scrollbar.
    ///
    /// CDXC:SessionChat 2026-11-14 WHY:
    /// GitHub issue #615: on the desktop, new messages often landed below the fold while the reader was at the end. gpui's list stops following on any scroll up, even a fraction of a pixel of trackpad momentum, and starts again only when a layout leaves the viewport within 1px of the end; a reader a few pixels short of it never followed again until they pressed the down pill. The phone follows whenever new content arrives while the reader is within the edge threshold (`autoscrollToTopThreshold` in Transcript.tsx). The desktop now asks the same question at the same moments, with the same threshold, and hides the pill within it (`transcript_left_bottom`), so "at the end" and "following" are one thing again.
    pub(super) fn follow_tail_if_near_end(&self) {
        if self.list.is_following_tail() || self.list.is_scrollbar_dragging() {
            return;
        }
        let held = self.answer_hold_active()
            || self.transcript_focus_follow.get().is_some()
            || self.disclosure_motion.borrow().follow_paused
            || self.reply_hold_placed();
        if held {
            return;
        }
        let scale = ChatAppearance::current(&self.snapshot).scale;
        if transcript_distance_to_end(&self.list, &self.spliced_height)
            <= px(SPEC.edge_threshold * scale)
        {
            self.list.set_follow_mode(gpui::FollowMode::Tail);
        }
    }

    /// Paints nothing; placed after the list so its prepaint reads the layout this frame just made,
    /// and redraws the chat when the pill drawn before that layout no longer matches it. That
    /// layout measured the rows replaced since the one before, so the height they were missing is
    /// let go here (`SplicedHeight`).
    pub(super) fn scroll_bottom_sync(&self, cx: &Context<Self>) -> AnyElement {
        let list = self.list.clone();
        let spliced = self.spliced_height.clone();
        let drawn = self.scroll_bottom_drawn.clone();
        let chat = cx.entity_id();
        let scale = ChatAppearance::current(&self.snapshot).scale;
        gpui::canvas(
            move |_, _, cx| {
                spliced.laid_out();
                if transcript_left_bottom(&list, &spliced, scale) != drawn.get() {
                    cx.notify(chat);
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_0()
        .into_any_element()
    }

    /// What still floats over the transcript's bottom edge: the Escape and "Agent was interrupted"
    /// toasts. "Scroll to bottom" and "Previous question" are icon pills on the working card
    /// (working_card.rs, chat_pills.rs).
    pub(super) fn scroll_bottom_button(&self, cx: &Context<Self>) -> AnyElement {
        let glass = crate::app::helpers::window_glass_active_for(self.main_window);
        let p = ChatAppearance::current(&self.snapshot).on_window_glass(glass);
        let toast = self.pill_toast();
        self.scroll_bottom_drawn.set(transcript_left_bottom(
            &self.list,
            &self.spliced_height,
            p.scale,
        ));
        let shown = toast.is_some();
        if glass {
            let label = toast.map_or_else(String::new, |toast| toast.text);
            return pill_row(&p)
                .child(self.scroll_bottom_window_placeholder(shown, label, &p, cx))
                .into_any_element();
        }
        let Some(toast) = toast else {
            return div().into_any_element();
        };
        let row = pill_row(&p);
        // The pill wears the composer's own chrome: the same opaque card fill, the 1px outline and
        // the inset top highlight session-chat-composer-focus.css gives both of them.
        let outline = if p.light {
            gpui::black().opacity(0.08)
        } else {
            gpui::white().opacity(0.05)
        };
        let highlight = gpui::white().opacity(if p.light { 0.0 } else { 0.03 });
        let (fill, border, text) = match toast.error {
            true => error_tone(p.light),
            false => (p.composer_background, outline, p.primary),
        };
        row.child(
            pill_shape(div(), p.scale)
                .id("chat-interrupt-toast")
                .role(gpui::Role::Status)
                .aria_label(toast.text.clone())
                .border_color(border)
                .bg(fill)
                .text_color(text)
                .when(!toast.error, |pill| pill.child(pill_highlight(highlight)))
                .child(toast.text),
        )
        .into_any_element()
    }

    /// The ↓ "Scroll to bottom" button, while the reader is away from the bottom: a round button
    /// floating over the transcript's bottom edge at the right, right above the Inbox dock (or the
    /// chat box when there is no dock), its right edge on theirs. The second value says whether it
    /// is drawn in the pane over the rows, so the pane must not lift the rows' indicators above it
    /// (render.rs); under window glass it is drawn in a blurred window of its own instead.
    ///
    /// CDXC:SessionChat 2026-11-26 DECISION:
    /// User: "pls move the scroll to bottom button, place it on the right side above the inbox as a button pointing down so it doesn't keep pushing my other pills when it appears, which is annoying. make sure it has same bg style as the header of the inbox pls and should have outline like pills do". The ↓ leaves the chat box's pill strip for good and always floats here, so it coming and going never moves a pill. Its face is the open Inbox header's (the chat box's fill under the pills bar's tint, `STRIP_TINT`) and its edge the pills' 1px outline. Supersedes the 2026-10-17 ↓ at the leftmost of the pills (chat_pills.rs) and the lone pill that stepped aside for the strip. The phone does the same (`LoneScrollBottom` in WorkingCard.tsx).
    pub(super) fn render_jump_down(
        &self,
        p: &ChatAppearance,
        glass: bool,
        cx: &mut Context<Self>,
    ) -> (Option<AnyElement>, bool) {
        let overlay = super::frosted_overlay_window::FrostedOverlay::JumpDown;
        let shown = self.list.item_count() > 0
            && transcript_left_bottom(&self.list, &self.spliced_height, p.scale);
        let s = p.scale;
        let button = if glass {
            let shown = shown && !self.frosted_overlay_covered(overlay, cx);
            let report = self.frosted_overlay_reporter(overlay, shown, cx);
            if !shown {
                // Still painted, so its window closes when the button hides.
                return (
                    Some(div().absolute().size_0().child(report).into_any_element()),
                    false,
                );
            }
            div()
                .relative()
                .size(px(JUMP_DOWN_SIZE * s))
                .child(report)
                .into_any_element()
        } else if shown {
            self.jump_down_button(p, cx)
        } else {
            return (None, false);
        };
        // The chat box's column (composer.rs footer), so the button's right edge is the dock's.
        let column = div()
            .w_full()
            .max_w(px(768.0 * s))
            .px(px(16.0 * s))
            .when(p.composer_matches_transcript, |column| {
                self.with_transcript_width(column, p)
            })
            .flex()
            .justify_end()
            .child(button);
        // The toasts' height over the transcript's bottom edge, so with the stack's own gap it
        // sits just above the dock or the box.
        let element = div()
            .absolute()
            .bottom(px(SPEC.bottom * s))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(column)
            .into_any_element();
        (Some(element), !glass)
    }

    fn jump_down_button(&self, p: &ChatAppearance, cx: &Context<Self>) -> AnyElement {
        use super::cursor::ChatCursor as _;
        let label = label().to_string();
        let tooltip = label.clone();
        let shortcut = shortcut_label().unwrap_or_default();
        jump_down_face(div().id("chat-scroll-bottom"), p)
            .tab_index(0)
            .role(gpui::Role::Button)
            .aria_label(label)
            .chat_cursor_pointer()
            .focus_visible(|style| style.border_color(p.ring))
            .block_mouse_except_scroll()
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(tooltip.clone())
                    .keys(shortcut.clone())
                    .build(window, cx)
            })
            .on_click(cx.listener(|chat, _, _, cx| {
                cx.stop_propagation();
                chat.jump_to_bottom(cx)
            }))
            .into_any_element()
    }
}

/// The ↓ button's diameter, unscaled: the pills' 16px glyph with room around it.
const JUMP_DOWN_SIZE: f32 = 28.0;

/// The ↓ button's look, in the pane and in its glass window: the open Inbox header's face (the
/// chat box's fill under the pills bar's tint, work_cards.rs `work_dock_header`), the pills' 1px
/// outline and hover (chat_pills.rs), and the down arrow. The tint is its own layer so it shows
/// over the glass's see-through wash too.
pub(super) fn jump_down_face(
    button: gpui::Stateful<gpui::Div>,
    p: &ChatAppearance,
) -> gpui::Stateful<gpui::Div> {
    let s = p.scale;
    let tint = p.foreground.opacity(super::pills_strip::STRIP_TINT);
    let hover = p.foreground.opacity(super::pills_strip::STRIP_TINT + 0.06);
    button
        .group("chat-jump-down")
        .relative()
        .flex_shrink_0()
        .size(px(JUMP_DOWN_SIZE * s))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .overflow_hidden()
        .border(px(s))
        .border_color(p.foreground.opacity(0.1))
        .bg(p.composer_background)
        .child(
            div()
                .absolute()
                .inset_0()
                .rounded_full()
                .bg(tint)
                .group_hover("chat-jump-down", move |style| style.bg(hover)),
        )
        .child(super::chat_pills::pill_glyph(
            "titlebar/arrow-down.svg",
            p.primary,
            s,
        ))
}

/// The centred row the toasts sit in, at the transcript's bottom edge, just above the working card
/// or the composer.
fn pill_row(p: &ChatAppearance) -> gpui::Div {
    div()
        .absolute()
        .bottom(px(SPEC.bottom * p.scale))
        .w_full()
        .flex()
        .justify_center()
        .gap(px(SPEC.pill_gap * p.scale))
}

/// How far the viewport's bottom is above the transcript's end, as of the list's last layout,
/// counting the rows replaced since then at the height they had (`SplicedHeight`).
pub(super) fn transcript_distance_to_end(
    list: &gpui::ListState,
    spliced: &SplicedHeight,
) -> gpui::Pixels {
    list_distance_to_end(list) + spliced.0.get()
}

/// The list's own answer: rows it has not laid out since they were replaced count 0px in it.
fn list_distance_to_end(list: &gpui::ListState) -> gpui::Pixels {
    list.max_offset_for_scrollbar().y + list.scroll_px_offset_for_scrollbar().y
}

/// The height that rows replaced since the list's last layout took out from below the reader.
///
/// CDXC:SessionChat 2026-11-26 WHY:
/// GitHub issue #781: a reader scrolled up in a long chat (coordinator threads most of all) was thrown to the bottom while the agent kept working. gpui's list counts a replaced row as 0px until a layout measures it, and the core's splices are a prefix and suffix diff, so a finished turn regrouping into its summary, or a step rewriting the rows under it, takes their whole height out of the list at once. The chat asks whether the reader is at the end again before the list lays out (the next frame's rows, the status row, a message just sent: `follow_tail_if_near_end`), and that read the end as up to several hundred pixels nearer than it was, even below the viewport: reproduced in the web build at 200px from the end, read as -71px, after which following turned on and the list jumped to the end. Each splice of the transcript list goes through [`SplicedHeight::splice`], which keeps the height it took out from below the reader; every reader of the distance adds it back until the list's next layout measures the new rows (`scroll_bottom_sync`). Rows above the reader leave the distance unchanged, so they add nothing; nothing else changes about when the chat follows, so #615 (following a reader within the edge threshold) holds.
#[derive(Clone, Default)]
pub(super) struct SplicedHeight(std::rc::Rc<std::cell::Cell<gpui::Pixels>>);

impl SplicedHeight {
    /// Applies `splice` to `list` (the list's own splice and any re-anchoring after it) and keeps
    /// what it took off the distance to the end.
    pub(super) fn splice(&self, list: &gpui::ListState, splice: impl FnOnce()) {
        let before = list_distance_to_end(list);
        splice();
        let lost = before - list_distance_to_end(list);
        if lost > px(0.0) {
            self.0.set(self.0.get() + lost);
        }
    }

    /// The list laid out: what it measures again is its own.
    fn laid_out(&self) {
        self.0.set(px(0.0));
    }
}

/// The reader has left the bottom of the transcript: the "Scroll to bottom" pill shows, and new
/// rows arrive below the fold. Within the edge threshold the reader is at the end: the next rows
/// to arrive are followed (`follow_tail_if_near_end`).
///
/// CDXC:SessionChat 2026-10-04 WHY:
/// User (Windows): "we're not auto scrolling down to the bottom automatically even when i don't see the scroll to bottom [button]". The pill used to hide within the 10px edge threshold while gpui's list resumes following only within 1px of the end, so a reader a few pixels short of it saw neither the pill nor following. This is now the one definition: the pill hides exactly while the list follows or will at its next layout, `scroll_bottom_sync` redraws it from the layout just made (the render decides it from the previous one), and the edge threshold only widens what counts as reaching the end on a downward wheel (`follow_tail_if_wheel_reaches_end`). Only a scroll up stops following. The 1px half of this is superseded by the 2026-11-14 WHY on `follow_tail_if_near_end`: the pill again hides within the edge threshold, and the list now follows from there too.
pub(super) fn transcript_left_bottom(
    list: &gpui::ListState,
    spliced: &SplicedHeight,
    scale: f32,
) -> bool {
    !list.is_following_tail()
        && transcript_distance_to_end(list, spliced) > px(SPEC.edge_threshold * scale)
}

/// What the pill's slot shows instead of "Scroll to bottom".
pub(super) struct PillToast {
    pub(super) text: String,
    /// The red "Agent was interrupted" tone.
    pub(super) error: bool,
}

/// The pill's shape, shared by "Scroll to bottom", "Previous question" and the toasts that take
/// their place.
pub(super) fn pill_shape(pill: gpui::Div, scale: f32) -> gpui::Div {
    pill.relative()
        .overflow_hidden()
        .flex()
        .items_center()
        .justify_center()
        .h(px(SPEC.height * scale))
        .px(px(SPEC.padding_x * scale))
        .rounded_full()
        .border_1()
        .text_size(px(SPEC.font_size * scale))
        .font_weight(FontWeight::MEDIUM)
        .whitespace_nowrap()
}

/// Visual only, and inside the pill's own frame: the `inset 0 1px` highlight.
fn pill_highlight(highlight: gpui::Hsla) -> gpui::Div {
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .h(px(1.0))
        .bg(highlight)
}

/// The red tone's fill, border and text (`errorTone` in `scroll-bottom.json`).
pub(super) fn error_tone(light: bool) -> (gpui::Hsla, gpui::Hsla, gpui::Hsla) {
    tone_colors(&SPEC.error_tone, light)
}

/// The Previous question pill's fill, border and text (`questionTone` in `scroll-bottom.json`).
pub(super) fn question_tone(light: bool) -> (gpui::Hsla, gpui::Hsla, gpui::Hsla) {
    tone_colors(&SPEC.question_tone, light)
}

fn tone_colors(tone: &ErrorTone, light: bool) -> (gpui::Hsla, gpui::Hsla, gpui::Hsla) {
    let tone = if light { &tone.light } else { &tone.dark };
    let hex = |value: &str| -> gpui::Hsla {
        gpui::rgb(u32::from_str_radix(value.trim_start_matches('#'), 16).unwrap_or(0)).into()
    };
    (
        hex(&tone.background),
        hex(&tone.border).opacity(tone.border_alpha),
        hex(&tone.text),
    )
}

impl NativeChatView {
    /// The toasts' frosted window.
    ///
    /// CDXC:SessionChat 2026-09-23 DECISION:
    /// User: the scroll-to-bottom pill "should match the color and everything of the composer box", then, when the composer's see-through wash let the transcript read through it, "can't we make it's bg more frosted glass??". Under window glass the pill wears the composer's wash and border over a real blur of what is behind it. Supersedes the same day's opaque menu tone and the plain see-through wash.
    ///
    /// CDXC:SessionChat 2026-09-23 WHY:
    /// GPUI cannot blur behind an element inside a window, only behind a whole window, so the pill is drawn in a small blurred child window of its own (frosted_overlay_window.rs). This placeholder lays out an invisible pill of the same size where the in-window one would be and hands its bounds to that window. The pill stays down while something else sits over the pane (an app modal such as a floating page, a chat popup window, the account-switch card, the subagent viewer), or while it would stick out of the pane, because a window of its own would float above all of them.
    fn scroll_bottom_window_placeholder(
        &self,
        shown: bool,
        label: String,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let overlay = super::frosted_overlay_window::FrostedOverlay::ScrollBottom;
        let shown = shown && !self.frosted_overlay_covered(overlay, cx);
        let report = self.frosted_overlay_reporter(overlay, shown, cx);
        if !shown {
            return div().absolute().size_0().child(report).into_any_element();
        }
        placeholder_pill(label, p).child(report).into_any_element()
    }
}

/// An invisible pill the size of the visible one, which a floating control's window is placed on.
pub(super) fn placeholder_pill(label: String, p: &ChatAppearance) -> gpui::Div {
    div()
        .relative()
        .flex()
        .items_center()
        .h(px(SPEC.height * p.scale))
        .px(px(SPEC.padding_x * p.scale))
        .border_1()
        .border_color(gpui::transparent_black())
        .text_color(gpui::transparent_black())
        .text_size(px(SPEC.font_size * p.scale))
        .font_weight(FontWeight::MEDIUM)
        .whitespace_nowrap()
        .child(label)
}
