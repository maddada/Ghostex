//! The Share popover's look (Ghostex Collab), drawn from a plain model so the app
//! (window/collab_share.rs, which reads gxserver and runs the actions) and the native modal demo
//! (bin/native_modal_demo/share_popover.rs, which draws every state offscreen) share one file.
//!
//! The "live panel": a status band on top says at a glance whether the chat is shared and who is
//! there (Collab isn't installed, Not shared, Creating, Live, Stop sharing?, Sharing ended); under
//! it the link and code, who is watching, what guest messages do and how long a link lasts; a
//! footer with the time left and Stop sharing. The popover is as tall as what it shows.
//!
//! CDXC:Collab 2026-11-08 DECISION:
//! User (Q1780-Q1786, share-popover/page.html): B "Live panel"; Collab installs right in the popover (Install, Touch ID, a progress bar, then "Ready to share"); Share shows before Collab is installed with "Don't show Share"; a guest's waiting message is pointed at (Review goes to the card above the chat box); Stop sharing asks first, naming who gets disconnected; the link lifetime is a chip before Create link that also changes the default, with "Until I stop it"; the phone gets the same design. Frosted light-grey glass, no near-black fills, sized to its content.
//! SEE-ALSO: apps/mobile/app/src/chat/native/cards/CollabShareMenu.tsx (the phone's sheet, same design).

use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    Animation, AnimationExt as _, AnyElement, Context, FontWeight, Hsla, InteractiveElement as _,
    IntoElement, ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    Transformation, Window, div, px, radians, svg,
};
use gpui_component::{h_flex, v_flex};

/// The popover's width; its height follows what it shows.
pub(crate) const SHARE_POPOVER_WIDTH: f32 = 344.0;

const ICON_USERS: &str = "titlebar/users.svg";
const ICON_PUZZLE: &str = "titlebar/puzzle.svg";
const ICON_EYE_OFF: &str = "titlebar/eye-off.svg";
const ICON_LOADER: &str = "titlebar/loader2.svg";
const ICON_LINK: &str = "titlebar/link.svg";
const ICON_COPY: &str = "titlebar/copy.svg";
const ICON_CHECK: &str = "titlebar/check.svg";
const ICON_INFO: &str = "titlebar/info-circle.svg";
const ICON_WORLD: &str = "titlebar/world.svg";
const ICON_LOCK: &str = "titlebar/lock.svg";
const ICON_SHIELD: &str = "modals/settings/shield.svg";
const ICON_DOWNLOAD: &str = "titlebar/download.svg";
const ICON_CLOCK: &str = "titlebar/clock.svg";
const ICON_STOP: &str = "titlebar/player-stop.svg";
const ICON_USER: &str = "titlebar/user.svg";
const ICON_X: &str = "titlebar/x.svg";
const ICON_CHEVRON: &str = "titlebar/chevron-down.svg";

/// What guest messages do, in the segmented control's order.
pub(crate) const MODES: [(&str, &str); 3] = [
    ("send", "Send"),
    ("approve", "Ask me"),
    ("view", "View only"),
];

/// How long a new link lasts (the chip before Create link, and Settings > Collab).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lifetime {
    Hour,
    FourHours,
    Day,
    UntilStopped,
}

impl Lifetime {
    pub(crate) const ALL: [Lifetime; 4] = [
        Lifetime::Hour,
        Lifetime::FourHours,
        Lifetime::Day,
        Lifetime::UntilStopped,
    ];

    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Hour => "hour",
            Self::FourHours => "fourHours",
            Self::Day => "day",
            Self::UntilStopped => "untilStopped",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Hour => "1 hour",
            Self::FourHours => "4 hours",
            Self::Day => "24 hours",
            Self::UntilStopped => "Until I stop it",
        }
    }

    pub(crate) fn parse(text: &str) -> Self {
        match text {
            "fourHours" => Self::FourHours,
            "day" => Self::Day,
            "untilStopped" => Self::UntilStopped,
            _ => Self::Hour,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Guest {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) joined_at_ms: Option<i64>,
    /// A message of theirs waits for the owner's approval.
    pub(crate) waiting: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Live {
    pub(crate) share_id: String,
    /// The link and code, once the owner revealed them with an approval this run.
    pub(crate) link: Option<String>,
    pub(crate) code: Option<String>,
    /// `None`: the link lasts until the owner stops sharing.
    pub(crate) expires_at_ms: Option<i64>,
    pub(crate) guests: Vec<Guest>,
    /// Guest messages waiting for the owner.
    pub(crate) waiting: usize,
    pub(crate) connected: bool,
    /// Index into [`MODES`].
    pub(crate) mode: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Stage {
    Loading,
    /// Collab isn't installed on this computer (Install, Don't show Share).
    NotInstalled {
        can_install: bool,
    },
    /// Installing waits for the owner's approval.
    InstallWaiting,
    /// Downloading (`fraction`) or installing and starting.
    Installing {
        fraction: Option<f32>,
    },
    /// Installed but not running (it restarts on its own).
    NotRunning {
        reason: String,
    },
    Ready,
    /// Create link waits for the owner's approval.
    Creating,
    Live(Live),
    ConfirmStop(Live),
    Ended,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ShareModel {
    pub(crate) stage: Stage,
    /// What a new share's guest messages do (index into [`MODES`]).
    pub(crate) mode: usize,
    pub(crate) lifetime: Lifetime,
    pub(crate) lifetime_menu_open: bool,
    /// Another change waits for the owner's approval (Show, Send mode): gxserver's words.
    pub(crate) waiting: Option<String>,
    pub(crate) copied: Option<&'static str>,
    pub(crate) error: Option<String>,
    /// The relay's guest page failed this release's check: sharing waits.
    pub(crate) notice: Option<String>,
    pub(crate) now_ms: i64,
    pub(crate) light: bool,
    /// Window glass is on: the frost shows through the panel.
    pub(crate) glass: bool,
    /// The panel's fill when the host has its own (the app's frosted popover fill).
    pub(crate) surface: Option<Hsla>,
}

impl ShareModel {
    pub(crate) fn new(stage: Stage) -> Self {
        Self {
            stage,
            mode: 1,
            lifetime: Lifetime::Hour,
            lifetime_menu_open: false,
            waiting: None,
            copied: None,
            error: None,
            notice: None,
            now_ms: 0,
            light: false,
            glass: true,
            surface: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ShareAction {
    Install,
    DontShowShare,
    CancelWait,
    SetMode(usize),
    ToggleLifetimeMenu,
    PickLifetime(Lifetime),
    CreateLink,
    CopyLink,
    CopyCode,
    ShowLink,
    RemoveGuest(String),
    Review,
    AskStop,
    KeepSharing,
    Stop,
}

pub(crate) type ShareHandler<V> = Rc<dyn Fn(&mut V, ShareAction, &mut Window, &mut Context<V>)>;

// ---- colours ------------------------------------------------------------------------------

/// The palette: white ink on frosted warm grey in dark mode, dark ink on frosted light grey in
/// light mode. No near-black fills.
#[derive(Clone, Copy)]
struct Palette {
    light: bool,
    glass: bool,
}

fn rgba(r: u8, g: u8, b: u8, a: f32) -> Hsla {
    gpui::Rgba {
        r: f32::from(r) / 255.0,
        g: f32::from(g) / 255.0,
        b: f32::from(b) / 255.0,
        a,
    }
    .into()
}

impl Palette {
    fn ink(self, alpha: f32) -> Hsla {
        if self.light {
            rgba(22, 22, 24, alpha)
        } else {
            rgba(255, 255, 255, alpha)
        }
    }

    /// Panel fill: translucent over window glass, a solid mid grey without it.
    fn surface(self) -> Hsla {
        match (self.light, self.glass) {
            (false, true) => rgba(64, 60, 58, 0.56),
            (false, false) => rgba(52, 49, 47, 1.0),
            (true, true) => rgba(246, 244, 242, 0.72),
            (true, false) => rgba(244, 242, 240, 1.0),
        }
    }

    fn primary_fill(self) -> Hsla {
        if self.light {
            rgba(36, 36, 38, 1.0)
        } else {
            rgba(236, 236, 236, 1.0)
        }
    }

    fn primary_text(self) -> Hsla {
        if self.light {
            rgba(250, 250, 250, 1.0)
        } else {
            rgba(22, 22, 22, 1.0)
        }
    }

    fn danger(self) -> Hsla {
        if self.light {
            rgba(184, 50, 44, 1.0)
        } else {
            rgba(240, 146, 142, 1.0)
        }
    }

    fn live(self) -> Hsla {
        if self.light {
            rgba(31, 157, 85, 1.0)
        } else {
            rgba(74, 222, 128, 1.0)
        }
    }

    fn menu_fill(self) -> Hsla {
        if self.light {
            rgba(252, 251, 250, 0.97)
        } else {
            rgba(78, 74, 72, 0.97)
        }
    }
}

// ---- small pieces -------------------------------------------------------------------------

fn icon(p: Palette, path: &'static str, size: f32, alpha: f32) -> gpui::Svg {
    svg()
        .path(path)
        .flex_shrink_0()
        .size(px(size))
        .text_color(p.ink(alpha))
}

fn spinner(p: Palette, size: f32) -> AnyElement {
    icon(p, ICON_LOADER, size, 0.85)
        .with_animation(
            "share-popover-spinner",
            Animation::new(std::time::Duration::from_millis(900)).repeat(),
            |svg, delta| {
                svg.with_transformation(Transformation::rotate(radians(
                    delta * std::f32::consts::TAU,
                )))
            },
        )
        .into_any_element()
}

#[derive(Clone, Copy, PartialEq)]
enum Tone {
    Normal,
    Primary,
    Ghost,
    Danger,
}

fn click<V: 'static>(
    on: &ShareHandler<V>,
    action: ShareAction,
    cx: &mut Context<V>,
) -> impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static {
    let on = on.clone();
    cx.listener(move |view, _, window, cx| on(view, action.clone(), window, cx))
}

#[allow(clippy::too_many_arguments)]
fn button<V: 'static>(
    p: Palette,
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    leading: Option<&'static str>,
    tone: Tone,
    full: bool,
    small: bool,
    action: ShareAction,
    on: &ShareHandler<V>,
    cx: &mut Context<V>,
) -> AnyElement {
    let label: SharedString = label.into();
    let (fill, border, text, hover) = match tone {
        Tone::Primary => (
            p.primary_fill(),
            p.primary_fill(),
            p.primary_text(),
            p.primary_fill().opacity(0.88),
        ),
        Tone::Ghost => (p.ink(0.0), p.ink(0.0), p.ink(0.7), p.ink(0.07)),
        Tone::Danger => (p.ink(0.09), p.ink(0.12), p.danger(), p.ink(0.14)),
        Tone::Normal => (p.ink(0.09), p.ink(0.12), p.ink(0.92), p.ink(0.14)),
    };
    h_flex()
        .id(id.into())
        .role(gpui::Role::Button)
        .aria_label(label.clone())
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .gap(px(if small { 5.0 } else { 6.0 }))
        .h(px(if full {
            32.0
        } else if small {
            24.0
        } else {
            28.0
        }))
        .px(px(if small { 8.0 } else { 11.0 }))
        .when(full, |this| this.w_full())
        .rounded(px(6.0))
        .bg(fill)
        .border_1()
        .border_color(border)
        .text_size(px(if small { 12.0 } else { 12.5 }))
        .when(tone == Tone::Primary, |this| {
            this.font_weight(FontWeight::SEMIBOLD)
        })
        .text_color(text)
        .whitespace_nowrap()
        .cursor_pointer()
        .hover(move |this| this.bg(hover))
        .when_some(leading, |this, path| {
            this.child(
                svg()
                    .path(path)
                    .flex_shrink_0()
                    .size(px(if small { 12.0 } else { 13.0 }))
                    .text_color(text),
            )
        })
        .child(label)
        .on_click(click(on, action, cx))
        .into_any_element()
}

/// The status band: an icon tile, a bold line and a quieter one.
fn band(p: Palette, tile: AnyElement, title: AnyElement, subtitle: String) -> AnyElement {
    h_flex()
        .w_full()
        .items_center()
        .gap(px(11.0))
        .px(px(13.0))
        .py(px(12.0))
        .border_b_1()
        .border_color(p.ink(0.07))
        .bg(p.ink(0.03))
        .child(
            div()
                .flex_shrink_0()
                .size(px(34.0))
                .rounded(px(9.0))
                .bg(p.ink(0.09))
                .flex()
                .items_center()
                .justify_center()
                .child(tile),
        )
        .child(
            v_flex().min_w_0().flex_1().child(title).child(
                div()
                    .text_size(px(12.0))
                    .text_color(p.ink(0.62))
                    .truncate()
                    .child(subtitle),
            ),
        )
        .into_any_element()
}

fn band_title(p: Palette, text: &str, live: bool) -> AnyElement {
    h_flex()
        .items_center()
        .gap(px(7.0))
        .text_size(px(14.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(p.ink(0.96))
        .when(live, |this| {
            this.child(div().size(px(7.0)).rounded_full().bg(p.live()))
        })
        .child(text.to_string())
        .into_any_element()
}

fn section_label(p: Palette, text: &str) -> AnyElement {
    div()
        .text_size(px(11.0))
        .text_color(p.ink(0.55))
        .mt(px(2.0))
        .mb(px(-4.0))
        .child(text.to_string())
        .into_any_element()
}

fn hint(p: Palette, path: &'static str, text: &str, centered: bool) -> AnyElement {
    h_flex()
        .w_full()
        .items_start()
        .when(centered, |this| this.justify_center())
        .gap(px(6.0))
        .text_size(px(11.5))
        .text_color(p.ink(0.58))
        .when(!path.is_empty(), |this| {
            this.child(div().mt(px(2.0)).child(icon(p, path, 12.0, 0.58)))
        })
        .child(div().min_w_0().child(text.to_string()))
        .into_any_element()
}

fn card(p: Palette) -> gpui::Div {
    div()
        .w_full()
        .rounded(px(8.0))
        .bg(p.ink(0.055))
        .border_1()
        .border_color(p.ink(0.09))
}

fn fact(p: Palette, path: &'static str, text: &str) -> AnyElement {
    h_flex()
        .w_full()
        .items_center()
        .gap(px(9.0))
        .text_size(px(12.5))
        .text_color(p.ink(0.82))
        .child(
            div()
                .flex_shrink_0()
                .size(px(26.0))
                .rounded(px(7.0))
                .bg(p.ink(0.07))
                .flex()
                .items_center()
                .justify_center()
                .child(icon(p, path, 13.0, 0.85)),
        )
        .child(div().min_w_0().flex_1().child(text.to_string()))
        .into_any_element()
}

fn facts(p: Palette, dim: bool) -> AnyElement {
    v_flex()
        .w_full()
        .gap(px(7.0))
        .when(dim, |this| this.opacity(0.42))
        .child(fact(
            p,
            ICON_WORLD,
            "Opens live in any browser, no app or account",
        ))
        .child(fact(p, ICON_LOCK, "Guests also type the eight-digit code"))
        .child(fact(
            p,
            ICON_SHIELD,
            "You approve each message before the agent sees it",
        ))
        .into_any_element()
}

fn body() -> gpui::Div {
    v_flex()
        .w_full()
        .gap(px(10.0))
        .px(px(13.0))
        .pt(px(12.0))
        .pb(px(13.0))
}

/// "Waiting for your approval": the card a change shows while Touch ID or the phone answers.
fn wait_card<V: 'static>(
    p: Palette,
    what: &str,
    on: &ShareHandler<V>,
    cx: &mut Context<V>,
) -> AnyElement {
    card(p)
        .child(
            h_flex()
                .items_center()
                .gap(px(10.0))
                .pl(px(11.0))
                .pr(px(10.0))
                .py(px(10.0))
                .child(
                    div()
                        .flex_shrink_0()
                        .size(px(30.0))
                        .rounded_full()
                        .bg(p.ink(0.08))
                        .border_1()
                        .border_color(p.ink(0.12))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(icon(p, ICON_LOCK, 13.0, 0.85)),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .text_size(px(12.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(p.ink(0.94))
                                .child("Waiting for your approval"),
                        )
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(p.ink(0.6))
                                .child(format!("{what}: Touch ID on this Mac, or your phone")),
                        ),
                )
                .child(button(
                    p,
                    "share-wait-cancel",
                    "Cancel",
                    None,
                    Tone::Ghost,
                    false,
                    false,
                    ShareAction::CancelWait,
                    on,
                    cx,
                )),
        )
        .into_any_element()
}

/// The guest-message control: Send (with a lock: it asks for Touch ID), Ask me, View only.
fn modes<V: 'static>(
    p: Palette,
    selected: usize,
    on: &ShareHandler<V>,
    cx: &mut Context<V>,
) -> AnyElement {
    let segments = MODES.iter().enumerate().map(|(index, (_, label))| {
        let on_seg = selected == index;
        h_flex()
            .id(("share-mode", index))
            .role(gpui::Role::RadioButton)
            .aria_label(SharedString::from(*label))
            .flex_1()
            .flex_basis(px(0.0))
            .h(px(26.0))
            .items_center()
            .justify_center()
            .gap(px(4.0))
            .rounded(px(5.0))
            .text_size(px(12.0))
            .text_color(p.ink(if on_seg { 0.96 } else { 0.68 }))
            .when(on_seg, |this| this.bg(p.ink(0.15)))
            .when(!on_seg, |this| this.hover(|this| this.bg(p.ink(0.07))))
            .cursor_pointer()
            .when(index == 0, |this| {
                this.child(icon(p, ICON_LOCK, 11.0, if on_seg { 0.96 } else { 0.62 }))
            })
            .child(*label)
            .on_click(click(on, ShareAction::SetMode(index), cx))
    });
    h_flex()
        .w_full()
        .p(px(2.0))
        .rounded(px(7.0))
        .bg(p.ink(0.06))
        .border_1()
        .border_color(p.ink(0.08))
        .children(segments)
        .into_any_element()
}

fn mode_line(mode: usize) -> &'static str {
    match MODES[mode.min(2)].0 {
        "send" => "Messages go straight to the agent. Turning this on asks for Touch ID.",
        "view" => "Guests watch; they can't write.",
        _ => "You approve each message before the agent sees it.",
    }
}

fn ends_at(p: &ShareModel, expires_at_ms: Option<i64>) -> String {
    match expires_at_ms {
        None => "stays live until you stop it".to_string(),
        Some(at) => match chrono::DateTime::from_timestamp_millis(at) {
            Some(at) => format!(
                "ends {}",
                at.with_timezone(&chrono::Local).format("%-I:%M %p")
            ),
            None => {
                let _ = p;
                "ends soon".to_string()
            }
        },
    }
}

fn ends_in(now_ms: i64, expires_at_ms: Option<i64>) -> String {
    let Some(at) = expires_at_ms else {
        return "Live until you stop it".to_string();
    };
    let left = (at - now_ms).max(0) / 60_000;
    match left {
        0 => "Ends in under a minute".to_string(),
        1..=59 => format!("Ends in {left} min"),
        _ => {
            let hours = (left + 30) / 60;
            format!("Ends in {hours} h")
        }
    }
}

fn joined(now_ms: i64, at: Option<i64>) -> String {
    let Some(at) = at else {
        return "Watching".to_string();
    };
    match (now_ms - at).max(0) / 60_000 {
        0 => "Joined just now".to_string(),
        1 => "Joined 1 min ago".to_string(),
        minutes @ 2..=59 => format!("Joined {minutes} min ago"),
        minutes => format!("Joined {} h ago", minutes / 60),
    }
}

/// Who gets disconnected, in words: "Alper and Sam", "Alper, Sam and 2 others".
pub(crate) fn names(guests: &[Guest]) -> Option<String> {
    let names: Vec<&str> = guests.iter().map(|guest| guest.name.as_str()).collect();
    match names.as_slice() {
        [] => None,
        [one] => Some(one.to_string()),
        [first, second] => Some(format!("{first} and {second}")),
        [first, second, rest @ ..] => Some(format!(
            "{first}, {second} and {} other{}",
            rest.len(),
            if rest.len() == 1 { "" } else { "s" }
        )),
    }
}

/// The band's second line while the link is live.
pub(crate) fn live_subtitle(model: &ShareModel, live: &Live) -> String {
    if !live.connected {
        return "Connecting to the share relay\u{2026}".to_string();
    }
    if live.waiting > 0 {
        return if live.waiting == 1 {
            "1 message waits for you".to_string()
        } else {
            format!("{} messages wait for you", live.waiting)
        };
    }
    let who = match live.guests.len() {
        0 => "Nobody is watching yet".to_string(),
        count => format!("{count} watching"),
    };
    format!("{who} \u{b7} {}", ends_at(model, live.expires_at_ms))
}

fn guest_row<V: 'static>(
    p: Palette,
    model: &ShareModel,
    guest: &Guest,
    on: &ShareHandler<V>,
    cx: &mut Context<V>,
) -> AnyElement {
    h_flex()
        .w_full()
        .min_h(px(34.0))
        .items_center()
        .gap(px(9.0))
        .child(
            div()
                .flex_shrink_0()
                .size(px(26.0))
                .rounded_full()
                .bg(p.ink(0.1))
                .flex()
                .items_center()
                .justify_center()
                .child(icon(p, ICON_USER, 13.0, 0.8)),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(p.ink(0.94))
                        .truncate()
                        .child(guest.name.clone()),
                )
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(p.ink(if guest.waiting { 0.85 } else { 0.55 }))
                        .child(if guest.waiting {
                            "Message waiting".to_string()
                        } else {
                            joined(model.now_ms, guest.joined_at_ms)
                        }),
                ),
        )
        .child(if guest.waiting {
            button(
                p,
                SharedString::from(format!("share-review-{}", guest.id)),
                "Review",
                None,
                Tone::Primary,
                false,
                true,
                ShareAction::Review,
                on,
                cx,
            )
        } else {
            div()
                .id(SharedString::from(format!("share-remove-{}", guest.id)))
                .role(gpui::Role::Button)
                .aria_label(SharedString::from(format!("Remove {}", guest.name)))
                .flex_shrink_0()
                .size(px(24.0))
                .rounded(px(6.0))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .hover(|this| this.bg(p.ink(0.08)))
                .child(icon(p, ICON_X, 13.0, 0.5))
                .on_click(click(on, ShareAction::RemoveGuest(guest.id.clone()), cx))
                .into_any_element()
        })
        .into_any_element()
}

fn copy_label(model: &ShareModel, what: &str, label: &'static str) -> (&'static str, &'static str) {
    if model.copied == Some(what) {
        ("Copied", ICON_CHECK)
    } else {
        (label, ICON_COPY)
    }
}

/// The link and code right after Create link: both in full, each with Copy.
fn invite_card<V: 'static>(
    p: Palette,
    model: &ShareModel,
    link: &str,
    code: &str,
    on: &ShareHandler<V>,
    cx: &mut Context<V>,
) -> AnyElement {
    let (link_label, link_icon) = copy_label(model, "link", "Copy");
    let (code_label, code_icon) = copy_label(model, "code", "Copy");
    let cells = h_flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(2.0))
        .children(code.chars().enumerate().flat_map(|(index, digit)| {
            let mut out: Vec<AnyElement> = Vec::new();
            if index == 4 {
                out.push(div().w(px(6.0)).into_any_element());
            }
            out.push(
                div()
                    .w(px(18.0))
                    .h(px(26.0))
                    .rounded(px(5.0))
                    .bg(p.ink(0.08))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(15.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.ink(0.96))
                    .child(digit.to_string())
                    .into_any_element(),
            );
            out
        }));
    let row = |label: &str, value: AnyElement, copy: AnyElement, top: bool| {
        h_flex()
            .w_full()
            .min_h(px(40.0))
            .items_center()
            .gap(px(8.0))
            .when(top, |this| this.border_t_1().border_color(p.ink(0.07)))
            .child(
                div()
                    .flex_shrink_0()
                    .w(px(30.0))
                    .text_size(px(11.5))
                    .text_color(p.ink(0.55))
                    .child(label.to_string()),
            )
            .child(div().flex_1().min_w_0().child(value))
            .child(copy)
    };
    card(p)
        .px(px(10.0))
        .py(px(4.0))
        .child(row(
            "Link",
            div()
                .font_family("Menlo")
                .text_size(px(12.0))
                .text_color(p.ink(0.86))
                .truncate()
                .child(shown_link(link))
                .into_any_element(),
            button(
                p,
                "share-copy-link",
                link_label,
                Some(link_icon),
                Tone::Normal,
                false,
                false,
                ShareAction::CopyLink,
                on,
                cx,
            ),
            false,
        ))
        .child(row(
            "Code",
            cells.into_any_element(),
            button(
                p,
                "share-copy-code",
                code_label,
                Some(code_icon),
                Tone::Normal,
                false,
                false,
                ShareAction::CopyCode,
                on,
                cx,
            ),
            true,
        ))
        .into_any_element()
}

/// The link in one row once people are watching, with Link and Code copy buttons; or, after the
/// app restarted, "Link and code hidden" with Show (it asks for Touch ID).
fn invite_row<V: 'static>(
    p: Palette,
    model: &ShareModel,
    live: &Live,
    on: &ShareHandler<V>,
    cx: &mut Context<V>,
) -> AnyElement {
    match (&live.link, &live.code) {
        (Some(link), Some(_)) => {
            let (link_label, link_icon) = copy_label(model, "link", "Link");
            let (code_label, code_icon) = copy_label(model, "code", "Code");
            card(p)
                .child(
                    h_flex()
                        .items_center()
                        .gap(px(7.0))
                        .pl(px(10.0))
                        .pr(px(6.0))
                        .py(px(6.0))
                        .child(icon(p, ICON_LINK, 13.0, 0.6))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .font_family("Menlo")
                                .text_size(px(12.0))
                                .text_color(p.ink(0.86))
                                .truncate()
                                .child(shown_link(link)),
                        )
                        .child(button(
                            p,
                            "share-copy-link",
                            link_label,
                            Some(link_icon),
                            Tone::Normal,
                            false,
                            true,
                            ShareAction::CopyLink,
                            on,
                            cx,
                        ))
                        .child(button(
                            p,
                            "share-copy-code",
                            code_label,
                            Some(code_icon),
                            Tone::Normal,
                            false,
                            true,
                            ShareAction::CopyCode,
                            on,
                            cx,
                        )),
                )
                .into_any_element()
        }
        _ => v_flex()
            .w_full()
            .gap(px(8.0))
            .child(
                card(p).child(
                    h_flex()
                        .items_center()
                        .gap(px(7.0))
                        .pl(px(10.0))
                        .pr(px(6.0))
                        .py(px(6.0))
                        .child(icon(p, ICON_EYE_OFF, 13.0, 0.6))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(12.5))
                                .text_color(p.ink(0.6))
                                .child("Link and code hidden"),
                        )
                        .child(button(
                            p,
                            "share-show-link",
                            "Show",
                            Some(ICON_LOCK),
                            Tone::Normal,
                            false,
                            true,
                            ShareAction::ShowLink,
                            on,
                            cx,
                        )),
                ),
            )
            .child(hint(
                p,
                ICON_INFO,
                "Showing them again asks for Touch ID or your phone.",
                false,
            ))
            .into_any_element(),
    }
}

/// The link as the popover shows it: without `https://` (Copy copies it whole).
fn shown_link(link: &str) -> String {
    link.trim_start_matches("https://")
        .trim_start_matches("http://")
        .to_string()
}

fn footer(p: Palette) -> gpui::Div {
    h_flex()
        .w_full()
        .items_center()
        .gap(px(8.0))
        .pl(px(13.0))
        .pr(px(12.0))
        .py(px(9.0))
        .border_t_1()
        .border_color(p.ink(0.08))
        .bg(p.ink(0.025))
}

// ---- the popover --------------------------------------------------------------------------

/// The whole popover for `model`; `on` runs what the owner clicks.
pub(crate) fn render_share_popover<V: 'static>(
    model: &ShareModel,
    on: &ShareHandler<V>,
    cx: &mut Context<V>,
) -> AnyElement {
    let p = Palette {
        light: model.light,
        glass: model.glass,
    };
    let header = h_flex()
        .w_full()
        .h(px(40.0))
        .items_center()
        .gap(px(8.0))
        .pl(px(13.0))
        .pr(px(12.0))
        .border_b_1()
        .border_color(p.ink(0.08))
        .child(icon(p, ICON_USERS, 15.0, 0.9))
        .child(
            div()
                .text_size(px(13.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.ink(0.96))
                .whitespace_nowrap()
                .child("Share this chat"),
        );
    let mut panel = v_flex()
        .id("share-popover")
        .w(px(SHARE_POPOVER_WIDTH))
        .rounded(px(10.0))
        .overflow_hidden()
        .bg(model.surface.unwrap_or_else(|| p.surface()))
        .border_1()
        .border_color(p.ink(0.12))
        .text_color(p.ink(0.92))
        .child(header);
    panel = match &model.stage {
        Stage::Loading => {
            panel.child(body().child(hint(p, "", "Reading Ghostex Collab\u{2026}", false)))
        }
        Stage::NotInstalled { can_install } => panel
            .child(band(
                p,
                icon(p, ICON_PUZZLE, 17.0, 0.88).into_any_element(),
                band_title(p, "Collab isn't installed", false),
                "It's an extension: Ghostex downloads it once".to_string(),
            ))
            .child(
                body()
                    .child(facts(p, false))
                    .child(if *can_install {
                        button(
                            p,
                            "share-install",
                            "Install Collab",
                            Some(ICON_DOWNLOAD),
                            Tone::Primary,
                            true,
                            false,
                            ShareAction::Install,
                            on,
                            cx,
                        )
                    } else {
                        hint(
                            p,
                            ICON_INFO,
                            "This build of Ghostex has no Collab component to install.",
                            false,
                        )
                    })
                    .child(hint(
                        p,
                        "",
                        "Installing asks for Touch ID or your phone",
                        true,
                    )),
            )
            .child(footer(p).justify_center().child(button(
                p,
                "share-dont-show",
                "Don't show Share",
                Some(ICON_EYE_OFF),
                Tone::Ghost,
                false,
                true,
                ShareAction::DontShowShare,
                on,
                cx,
            ))),
        Stage::InstallWaiting => panel
            .child(band(
                p,
                spinner(p, 17.0),
                band_title(p, "Installing Collab\u{2026}", false),
                "Approve with Touch ID, or on your phone".to_string(),
            ))
            .child(
                body()
                    .child(facts(p, true))
                    .child(wait_card(p, "Install Collab", on, cx)),
            ),
        Stage::Installing { fraction } => {
            let percent = fraction.map(|fraction| (fraction.clamp(0.0, 1.0) * 100.0).floor());
            panel
                .child(band(
                    p,
                    icon(p, ICON_DOWNLOAD, 17.0, 0.88).into_any_element(),
                    band_title(
                        p,
                        if percent.is_some() {
                            "Downloading Collab\u{2026}"
                        } else {
                            "Installing Collab\u{2026}"
                        },
                        false,
                    ),
                    match percent {
                        Some(percent) => {
                            format!("{percent}% \u{b7} then it starts on this computer")
                        }
                        None => "It starts on this computer in a moment".to_string(),
                    },
                ))
                .child(
                    body()
                        .child(
                            div()
                                .w_full()
                                .h(px(6.0))
                                .rounded(px(3.0))
                                .bg(p.ink(0.1))
                                .overflow_hidden()
                                .child(
                                    div()
                                        .h_full()
                                        .rounded(px(3.0))
                                        .bg(p.ink(0.8))
                                        .w(gpui::relative(fraction.unwrap_or(1.0).clamp(0.0, 1.0))),
                                ),
                        )
                        .child(hint(
                            p,
                            "",
                            "You can close this; Share is ready when it finishes",
                            true,
                        )),
                )
        }
        Stage::NotRunning { reason } => panel
            .child(band(
                p,
                spinner(p, 17.0),
                band_title(p, "Collab is starting\u{2026}", false),
                "It restarts on its own".to_string(),
            ))
            .when(!reason.is_empty(), |panel| {
                panel.child(body().child(hint(p, ICON_INFO, reason, false)))
            }),
        Stage::Ready | Stage::Creating => {
            let creating = model.stage == Stage::Creating;
            let options = v_flex()
                .w_full()
                .gap(px(10.0))
                .when(creating, |this| this.opacity(0.42))
                .child(section_label(p, "Guest messages"))
                .child(modes(p, model.mode, on, cx))
                .child(hint(p, ICON_SHIELD, mode_line(model.mode), false))
                .child(
                    h_flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_size(px(11.5))
                        .text_color(p.ink(0.6))
                        .child(icon(p, ICON_WORLD, 12.0, 0.6))
                        .child("Opens in any browser")
                        .child(div().w(px(8.0)))
                        .child(icon(p, ICON_LOCK, 12.0, 0.6))
                        .child("Plus an eight-digit code"),
                )
                .child(lifetime_row(p, model, on, cx));
            panel
                .child(if creating {
                    band(
                        p,
                        spinner(p, 17.0),
                        band_title(p, "Creating the link\u{2026}", false),
                        "Approve with Touch ID, or on your phone".to_string(),
                    )
                } else {
                    band(
                        p,
                        icon(p, ICON_EYE_OFF, 17.0, 0.88).into_any_element(),
                        band_title(p, "Not shared", false),
                        "Only you can see this chat".to_string(),
                    )
                })
                .child(body().child(options).child(if creating {
                    wait_card(p, "Create the link", on, cx)
                } else {
                    button(
                        p,
                        "share-create",
                        "Create link",
                        Some(ICON_LINK),
                        Tone::Primary,
                        true,
                        false,
                        ShareAction::CreateLink,
                        on,
                        cx,
                    )
                }))
        }
        Stage::Live(live) => {
            let mut content = body();
            let fresh = live.guests.is_empty() && live.link.is_some() && live.code.is_some();
            if fresh {
                content = content
                    .child(invite_card(
                        p,
                        model,
                        live.link.as_deref().unwrap_or_default(),
                        live.code.as_deref().unwrap_or_default(),
                        on,
                        cx,
                    ))
                    .child(hint(
                        p,
                        ICON_INFO,
                        "Send the code another way than the link: guests need both.",
                        false,
                    ));
            } else {
                content = content.child(invite_row(p, model, live, on, cx));
            }
            if !live.guests.is_empty() {
                content = content.child(section_label(p, "Watching")).child(
                    v_flex().w_full().gap(px(4.0)).children(
                        live.guests
                            .iter()
                            .map(|guest| guest_row(p, model, guest, on, cx)),
                    ),
                );
            }
            content = content
                .child(section_label(p, "Guest messages"))
                .child(modes(p, live.mode, on, cx));
            if let Some(what) = &model.waiting {
                content = content.child(wait_card(p, what, on, cx));
            }
            panel
                .child(band(
                    p,
                    icon(p, ICON_USERS, 17.0, 0.88).into_any_element(),
                    band_title(p, "Live", live.connected),
                    live_subtitle(model, live),
                ))
                .child(content)
                .child(
                    footer(p)
                        .child(
                            h_flex()
                                .flex_1()
                                .min_w_0()
                                .items_center()
                                .gap(px(6.0))
                                .text_size(px(12.0))
                                .text_color(p.ink(0.6))
                                .whitespace_nowrap()
                                .child(icon(p, ICON_CLOCK, 13.0, 0.6))
                                .child(ends_in(model.now_ms, live.expires_at_ms)),
                        )
                        .child(button(
                            p,
                            "share-stop",
                            "Stop sharing",
                            Some(ICON_STOP),
                            Tone::Normal,
                            false,
                            false,
                            ShareAction::AskStop,
                            on,
                            cx,
                        )),
                )
        }
        Stage::ConfirmStop(live) => {
            let line = match names(&live.guests) {
                Some(who) => format!(
                    "{who} see \u{201c}Sharing ended\u{201d} and can't come back with this link. You can make a new link any time."
                ),
                None => "Nobody is watching. You can make a new link any time.".to_string(),
            };
            panel
                .child(band(
                    p,
                    icon(p, ICON_STOP, 17.0, 0.88).into_any_element(),
                    band_title(p, "Stop sharing?", false),
                    "The link and code stop working".to_string(),
                ))
                .child(
                    div()
                        .px(px(13.0))
                        .py(px(12.0))
                        .text_size(px(12.5))
                        .text_color(p.ink(0.82))
                        .child(line),
                )
                .child(
                    footer(p)
                        .justify_end()
                        .child(button(
                            p,
                            "share-keep",
                            "Keep sharing",
                            None,
                            Tone::Normal,
                            false,
                            false,
                            ShareAction::KeepSharing,
                            on,
                            cx,
                        ))
                        .child(button(
                            p,
                            "share-stop-confirm",
                            "Stop sharing",
                            Some(ICON_STOP),
                            Tone::Danger,
                            false,
                            false,
                            ShareAction::Stop,
                            on,
                            cx,
                        )),
                )
        }
        Stage::Ended => panel
            .child(band(
                p,
                icon(p, ICON_EYE_OFF, 17.0, 0.88).into_any_element(),
                band_title(p, "Sharing ended", false),
                "Only you can see this chat again".to_string(),
            ))
            .child(body().child(button(
                p,
                "share-create-new",
                "Create a new link",
                Some(ICON_LINK),
                Tone::Normal,
                true,
                false,
                ShareAction::CreateLink,
                on,
                cx,
            ))),
    };
    if let Some(text) = model.error.clone().or_else(|| model.notice.clone()) {
        panel = panel.child(
            div()
                .px(px(13.0))
                .pb(px(11.0))
                .text_size(px(12.0))
                .text_color(p.danger())
                .child(text),
        );
    }
    panel.into_any_element()
}

/// "Link lasts" and its chip; the chip opens the lifetimes, "Until I stop it" among them.
fn lifetime_row<V: 'static>(
    p: Palette,
    model: &ShareModel,
    on: &ShareHandler<V>,
    cx: &mut Context<V>,
) -> AnyElement {
    let chip = h_flex()
        .id("share-lifetime")
        .role(gpui::Role::Button)
        .aria_label(SharedString::from(format!(
            "Link lasts {}",
            model.lifetime.label()
        )))
        .items_center()
        .gap(px(5.0))
        .h(px(24.0))
        .pl(px(9.0))
        .pr(px(8.0))
        .rounded(px(6.0))
        .bg(p.ink(if model.lifetime_menu_open { 0.14 } else { 0.08 }))
        .border_1()
        .border_color(p.ink(0.1))
        .text_size(px(12.0))
        .text_color(p.ink(0.9))
        .whitespace_nowrap()
        .cursor_pointer()
        .hover(|this| this.bg(p.ink(0.12)))
        .child(model.lifetime.label())
        .child(icon(p, ICON_CHEVRON, 11.0, 0.7))
        .on_click(click(on, ShareAction::ToggleLifetimeMenu, cx));
    let row = h_flex()
        .w_full()
        .items_center()
        .justify_between()
        .text_size(px(12.0))
        .text_color(p.ink(0.62))
        .child("Link lasts")
        .child(chip);
    if !model.lifetime_menu_open {
        return row.into_any_element();
    }
    let options = Lifetime::ALL.iter().map(|lifetime| {
        let on_now = *lifetime == model.lifetime;
        h_flex()
            .id(SharedString::from(format!(
                "share-lifetime-{}",
                lifetime.id()
            )))
            .role(gpui::Role::MenuItem)
            .aria_label(SharedString::from(lifetime.label()))
            .w_full()
            .h(px(26.0))
            .px(px(8.0))
            .items_center()
            .justify_between()
            .rounded(px(5.0))
            .text_size(px(12.5))
            .text_color(p.ink(0.92))
            .when(on_now, |this| this.bg(p.ink(0.12)))
            .hover(|this| this.bg(p.ink(0.1)))
            .cursor_pointer()
            .child(lifetime.label())
            .when(on_now, |this| this.child(icon(p, ICON_CHECK, 12.0, 0.85)))
            .on_click(click(on, ShareAction::PickLifetime(*lifetime), cx))
    });
    v_flex()
        .w_full()
        .gap(px(6.0))
        .child(row)
        .child(
            h_flex().w_full().justify_end().child(
                v_flex()
                    .w(px(150.0))
                    .p(px(4.0))
                    .rounded(px(8.0))
                    .bg(p.menu_fill())
                    .border_1()
                    .border_color(p.ink(0.14))
                    .children(options),
            ),
        )
        .when(model.lifetime == Lifetime::UntilStopped, |this| {
            this.child(hint(
                p,
                ICON_INFO,
                "The link stays live until you stop sharing or delete the chat.",
                false,
            ))
        })
        .into_any_element()
}

/// The model the app shows for gxserver's `/api/collab` read (`state`) and this run's revealed
/// link and code (`revealed`), for the chat `project_id`/`session_id`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn stage_from_read(
    state: &serde_json::Value,
    project_id: &str,
    session_id: &str,
    revealed: Option<(String, String)>,
    confirm_stop: bool,
    just_stopped: bool,
) -> Stage {
    use serde_json::Value;
    if state.is_null() {
        return Stage::Loading;
    }
    if state["installed"] != true {
        let component = &state["component"];
        return match component["phase"].as_str() {
            Some("downloading") => Stage::Installing {
                fraction: match (
                    component["doneBytes"].as_f64(),
                    component["totalBytes"].as_f64(),
                ) {
                    (Some(done), Some(total)) if total > 0.0 => Some((done / total) as f32),
                    _ => Some(0.0),
                },
            },
            Some("installing") => Stage::Installing { fraction: None },
            _ => Stage::NotInstalled {
                can_install: component["canInstall"] != false,
            },
        };
    }
    if state["running"] == false || state["config"].is_null() {
        return Stage::NotRunning {
            reason: state["component"]["statusError"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
        };
    }
    let share = state["shares"].as_array().and_then(|shares| {
        shares
            .iter()
            .find(|share| share["projectId"] == project_id && share["sessionId"] == session_id)
    });
    let Some(share) = share else {
        return if just_stopped {
            Stage::Ended
        } else {
            Stage::Ready
        };
    };
    let mode = MODES
        .iter()
        .position(|(id, _)| share["mode"] == *id)
        .unwrap_or(1);
    let waiting_ids: Vec<&str> = share["waitingViewerIds"]
        .as_array()
        .map(|ids| ids.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let guests = share["viewers"]
        .as_array()
        .map(|viewers| {
            viewers
                .iter()
                .map(|viewer| Guest {
                    id: viewer["id"].as_str().unwrap_or_default().to_string(),
                    name: viewer["name"].as_str().unwrap_or_default().to_string(),
                    joined_at_ms: viewer["joinedAtMs"].as_i64(),
                    waiting: waiting_ids.contains(&viewer["id"].as_str().unwrap_or_default()),
                })
                .collect()
        })
        .unwrap_or_default();
    let (link, code) = match revealed {
        Some((link, code)) => (Some(link), Some(code)),
        None => (None, None),
    };
    let live = Live {
        share_id: share["shareId"].as_str().unwrap_or_default().to_string(),
        link,
        code,
        expires_at_ms: share["expiresAtMs"].as_i64(),
        guests,
        waiting: share["held"].as_u64().unwrap_or(0) as usize,
        connected: state["relayConnected"] == true,
        mode,
    };
    if confirm_stop {
        Stage::ConfirmStop(live)
    } else {
        Stage::Live(live)
    }
}
