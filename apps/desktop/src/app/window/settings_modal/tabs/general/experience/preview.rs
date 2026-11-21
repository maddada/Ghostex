//! The Experience page's preview: a small Ghostex window (the rail, the sessions list, a
//! coordinator's chat and its chat box) drawn with the app's own icons, showing what the tried
//! settings show. A part a change brings in grows in with a highlight; a part it takes away is
//! outlined in red, then folds out. The area the pointer is on in the page stays lit and the others
//! dim.
//!
//! CDXC:Experience 2026-11-21 WHY:
//! The preview is drawn, not the real sessions list and chat box scaled down: those renderers draw
//! live sessions from the app's stores, which the Settings window does not hold, and GPUI cannot
//! scale an element tree, so a small copy of them would need every size passed through anyway.
use super::super::super::super::super::native_modal_kit::*;
use super::super::super::super::palette::SettingsPalette;
use super::draft::{ExperienceDraft, Look, Part};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Rgba,
    SharedString, Styled as _, div, px, rgb,
};
use gpui_component::{h_flex, v_flex};
use std::collections::HashMap;

/// The preview's scale: the app's own sizes times this.
const SCALE: f32 = 0.64;

fn z(size: f32) -> Pixels {
    px(size * SCALE)
}

fn icon(path: &'static str, size: f32, color: Rgba) -> gpui::Svg {
    modal_icon(path, size * SCALE, color)
}

/// The colours the preview draws with, from the Settings palette and the app's brand colours.
struct Ink {
    window: Rgba,
    side: Rgba,
    card: Rgba,
    line: Rgba,
    fg: Rgba,
    muted: Rgba,
    dim: Rgba,
    accent: Rgba,
    bad: Rgba,
    gold: Rgba,
    claude: Rgba,
    pink: Rgba,
    mode: Rgba,
    added: Rgba,
    removed: Rgba,
    send: Rgba,
    on_send: Rgba,
}

impl Ink {
    fn new(p: &SettingsPalette) -> Self {
        let tone = |dark: u32, light: u32| rgb(if p.light { light } else { dark });
        Self {
            window: p.surface,
            side: css_mix(p.foreground, 0.025, p.surface),
            card: p.raised,
            line: p.hairline,
            fg: p.foreground,
            muted: p.muted,
            dim: css_mix(p.muted, 0.7, p.surface),
            accent: p.settings_accent,
            bad: p.destructive,
            gold: tone(0xd6a740, 0xa87a12),
            claude: tone(0xd97757, 0xc4623f),
            pink: tone(0xe0679b, 0xc2407a),
            mode: tone(0xffb6c5, 0x86606a),
            added: tone(0x7fbf7f, 0x2f7d32),
            removed: tone(0xc98585, 0xb44444),
            send: p.foreground,
            on_send: p.surface,
        }
    }
}

/// What the preview needs from the page: the parts and how they move, and the lit area.
pub(super) struct PreviewState<'a> {
    pub(super) shown: HashMap<Part, bool>,
    pub(super) draft: &'a ExperienceDraft,
    pub(super) lit: Option<&'static str>,
}

impl PreviewState<'_> {
    fn look(&self, part: Part) -> Look {
        self.draft
            .look(part, self.shown.get(&part).copied().unwrap_or(false))
    }
}

/// The highlight or the going-away outline around a part.
fn marked(look: Look, ink: &Ink, content: impl IntoElement) -> gpui::Div {
    let edge = if look.leaving {
        ink.bad
    } else {
        css_fade(ink.accent, look.glow)
    };
    div()
        .rounded(z(7.0))
        .border_1()
        .border_color(hsla(edge))
        .when(look.leaving, |this| this.border_dashed().opacity(0.55))
        .when(look.glow > 0.0, |this| {
            this.bg(hsla(css_fade(ink.accent, 0.26 * look.glow)))
        })
        .child(content)
}

/// A part in a row: `width` (with its `gap` before it) times how much of it shows.
fn inline_part(
    look: Look,
    width: f32,
    gap: f32,
    ink: &Ink,
    content: impl IntoElement,
) -> AnyElement {
    if look.size <= 0.0 {
        return div().into_any_element();
    }
    div()
        .flex_shrink_0()
        .w(z((width + gap) * look.size))
        .overflow_hidden()
        .opacity(look.size)
        .child(
            h_flex().w(z(width + gap)).pl(z(gap)).child(
                marked(look, ink, content)
                    .w(z(width))
                    .flex()
                    .items_center()
                    .justify_center(),
            ),
        )
        .into_any_element()
}

/// A part on its own line, `height` tall, times how much of it shows.
fn block_part(look: Look, height: f32, ink: &Ink, content: impl IntoElement) -> AnyElement {
    if look.size <= 0.0 {
        return div().into_any_element();
    }
    div()
        .w_full()
        .flex_shrink_0()
        .h(z(height * look.size))
        .overflow_hidden()
        .opacity(look.size)
        .child(marked(look, ink, content).w_full().h(z(height)))
        .into_any_element()
}

fn label(text: &'static str, size: f32, color: Rgba) -> gpui::Div {
    div()
        .text_size(z(size))
        .line_height(z(size * 1.35))
        .text_color(hsla(color))
        .whitespace_nowrap()
        .child(text)
}

/// The area wrapper: dims while another area is lit.
fn area(id: &'static str, lit: Option<&'static str>, content: gpui::Div) -> gpui::Div {
    content.opacity(if lit.is_some_and(|lit| lit != id) {
        0.28
    } else {
        1.0
    })
}

pub(super) fn preview(p: &SettingsPalette, state: &PreviewState) -> AnyElement {
    let ink = Ink::new(p);
    h_flex()
        .w_full()
        .h(z(600.0))
        .items_stretch()
        .rounded(z(14.0))
        .border_1()
        .border_color(hsla(ink.line))
        .bg(hsla(ink.window))
        .overflow_hidden()
        .child(rail(&ink))
        .child(area("sessionsList", state.lit, sessions_list(&ink, state)))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(chat_header(&ink))
                .child(area("chat", state.lit, chat(&ink, state)))
                .child(area("chatBox", state.lit, chat_box(&ink, state))),
        )
        .into_any_element()
}

fn rail(ink: &Ink) -> gpui::Div {
    let icons = [
        "titlebar/layout-sidebar.svg",
        "titlebar/search.svg",
        "titlebar/ghostex-flower.svg",
        "titlebar/users.svg",
        "titlebar/clock.svg",
        "titlebar/dots.svg",
    ];
    v_flex()
        .flex_shrink_0()
        .w(z(46.0))
        .items_center()
        .gap(z(18.0))
        .pt(z(16.0))
        .border_r_1()
        .border_color(hsla(ink.line))
        .children(icons.map(|path| icon(path, 18.0, ink.muted)))
}

#[derive(Clone, Copy)]
enum Agent {
    Coordinator,
    Claude,
    Browser,
}

struct Row {
    agent: Agent,
    title: &'static str,
    machine: &'static str,
    time: Option<&'static str>,
    selected: bool,
}

const APPLE: &str = "titlebar/brand-apple.svg";
const WINDOWS: &str = "titlebar/brand-windows.svg";

fn session_row(index: usize, row: &Row, ink: &Ink, state: &PreviewState) -> AnyElement {
    let group: SharedString = format!("experience-preview-row-{index}").into();
    let (path, color, hover_part) = match row.agent {
        Agent::Coordinator => (
            "titlebar/coordinator-crown.svg",
            ink.gold,
            Part::AgentIconsOnHover,
        ),
        Agent::Claude => (
            "agent-icons/claude.svg",
            ink.claude,
            Part::AgentIconsOnHover,
        ),
        Agent::Browser => ("titlebar/world.svg", ink.accent, Part::FaviconsOnHover),
    };
    let on_hover = state.shown.get(&hover_part).copied().unwrap_or(false);
    let look = state.draft.look(hover_part, on_hover);
    let agent = marked(look, ink, icon(path, 15.0, color))
        .flex_shrink_0()
        .p(z(1.0))
        .when(on_hover && !look.leaving, |this| {
            this.opacity(0.0)
                .group_hover(group.clone(), |style| style.opacity(1.0))
        });
    h_flex()
        .id(group.clone())
        .group(group)
        .h(z(30.0))
        .pl(z(16.0))
        .pr(z(8.0))
        .gap(z(7.0))
        .items_center()
        .rounded(z(8.0))
        .when(row.selected, |this| {
            this.bg(hsla(css_mix(ink.fg, 0.11, ink.side)))
        })
        .child(agent)
        .child(
            label(row.title, 13.5, ink.fg)
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis(),
        )
        .when(row.selected, |this| {
            this.child(
                h_flex()
                    .gap(z(3.0))
                    .child(icon("titlebar/users-group.svg", 12.0, ink.gold))
                    .child(label("4", 12.0, ink.gold)),
            )
        })
        .child(icon(row.machine, 12.0, ink.dim))
        .when(row.selected, |this| {
            this.child(div().size(z(9.0)).rounded_full().bg(hsla(ink.pink)))
        })
        .children(row.time.map(|time| {
            inline_part(
                state.look(Part::LastActive),
                24.0,
                4.0,
                ink,
                label(time, 12.5, ink.dim),
            )
        }))
        .into_any_element()
}

fn project_row(name: &'static str, ghostex: bool, ink: &Ink, state: &PreviewState) -> gpui::Div {
    let tile = if ghostex {
        div()
            .size(z(18.0))
            .rounded(z(5.0))
            .bg(hsla(rgb(0x2f6fd6)))
            .flex()
            .items_center()
            .justify_center()
            .child(icon("titlebar/ghostex-flower.svg", 13.0, rgb(0xffffff)))
    } else {
        div()
            .size(z(18.0))
            .rounded(z(4.0))
            .bg(hsla(rgb(0x000000)))
            .flex()
            .items_center()
            .justify_center()
            .text_size(z(8.5))
            .font_weight(FontWeight::BOLD)
            .text_color(hsla(rgb(0xffffff)))
            .child("T3")
    };
    h_flex()
        .h(z(30.0))
        .mt(z(10.0))
        .px(z(4.0))
        .items_center()
        .child(icon("titlebar/chevron-down.svg", 12.0, ink.muted))
        .child(inline_part(
            state.look(Part::ProjectIcons),
            20.0,
            6.0,
            ink,
            tile,
        ))
        .child(label(name, 15.0, ink.fg).ml(z(7.0)))
        .child(div().ml(z(6.0)).child(icon(APPLE, 12.0, ink.dim)))
        .child(div().flex_1())
        .when(ghostex, |this| {
            this.child(inline_part(
                state.look(Part::DiffStats),
                70.0,
                0.0,
                ink,
                h_flex()
                    .gap(z(6.0))
                    .child(label("+928", 13.0, ink.added))
                    .child(label("-678", 13.0, ink.removed)),
            ))
            .child(inline_part(
                state.look(Part::FileCount),
                46.0,
                6.0,
                ink,
                label("14 files", 12.5, ink.dim),
            ))
        })
}

fn group_title(text: &'static str, ink: &Ink) -> gpui::Div {
    label(text, 13.0, ink.dim).pl(z(20.0)).pt(z(8.0)).pb(z(4.0))
}

fn sessions_list(ink: &Ink, state: &PreviewState) -> gpui::Div {
    let pinned = [
        Row {
            agent: Agent::Coordinator,
            title: "Main Coordinator",
            machine: APPLE,
            time: None,
            selected: true,
        },
        Row {
            agent: Agent::Coordinator,
            title: "Ghostex deps and macOS",
            machine: APPLE,
            time: Some("10m"),
            selected: false,
        },
        Row {
            agent: Agent::Claude,
            title: "Experience settings page",
            machine: APPLE,
            time: Some("2m"),
            selected: false,
        },
    ];
    let sessions = [
        Row {
            agent: Agent::Claude,
            title: "Gxserver wmx.exe check",
            machine: WINDOWS,
            time: Some("58m"),
            selected: false,
        },
        Row {
            agent: Agent::Browser,
            title: "Easy Connect docs",
            machine: APPLE,
            time: Some("3h"),
            selected: false,
        },
        Row {
            agent: Agent::Coordinator,
            title: "Arch Chungus",
            machine: APPLE,
            time: Some("8h"),
            selected: false,
        },
    ];
    let other = Row {
        agent: Agent::Coordinator,
        title: "t3code-pm PM",
        machine: APPLE,
        time: Some("6h"),
        selected: false,
    };
    let lights = [0xff5f57, 0xfebc2e, 0x28c840]
        .map(|color| div().size(z(12.0)).rounded_full().bg(hsla(rgb(color))));
    v_flex()
        .flex_shrink_0()
        .w(z(278.0))
        .px(z(10.0))
        .py(z(12.0))
        .bg(hsla(ink.side))
        .border_r_1()
        .border_color(hsla(ink.line))
        .child(
            h_flex()
                .gap(z(8.0))
                .px(z(4.0))
                .pb(z(12.0))
                .items_center()
                .children(lights)
                .child(div().flex_1())
                .child(icon("titlebar/folder-plus.svg", 15.0, ink.muted))
                .child(icon("titlebar/bell.svg", 15.0, ink.muted))
                .child(icon("titlebar/menu-2.svg", 15.0, ink.muted)),
        )
        .child(
            h_flex()
                .h(z(32.0))
                .px(z(10.0))
                .gap(z(8.0))
                .items_center()
                .rounded(z(9.0))
                .bg(hsla(ink.card))
                .child(icon("titlebar/plus.svg", 14.0, ink.muted))
                .child(label("New session", 13.5, ink.fg).flex_1())
                .child(icon("titlebar/chevron-right.svg", 13.0, ink.muted)),
        )
        .child(project_row("Ghostex", true, ink, state))
        .child(group_title("Pinned", ink))
        .children(
            pinned
                .iter()
                .enumerate()
                .map(|(index, row)| session_row(index, row, ink, state)),
        )
        .child(group_title("Sessions", ink))
        .children(
            sessions
                .iter()
                .enumerate()
                .map(|(index, row)| session_row(index + 3, row, ink, state)),
        )
        .child(project_row("t3code", false, ink, state))
        .child(session_row(6, &other, ink, state))
}

fn chat_header(ink: &Ink) -> gpui::Div {
    h_flex()
        .flex_shrink_0()
        .h(z(42.0))
        .px(z(16.0))
        .gap(z(10.0))
        .items_center()
        .border_b_1()
        .border_color(hsla(ink.line))
        .child(icon("titlebar/chevron-right.svg", 13.0, ink.muted))
        .child(label("Main Coordinator", 13.0, ink.fg))
        .child(div().flex_1())
        .child(
            h_flex()
                .h(z(26.0))
                .px(z(9.0))
                .gap(z(6.0))
                .items_center()
                .rounded(z(7.0))
                .border_1()
                .border_color(hsla(ink.line))
                .child(icon("titlebar/stack.svg", 13.0, ink.muted))
                .child(label("Work", 13.0, ink.muted)),
        )
        .child(icon("titlebar/dots.svg", 15.0, ink.muted))
}

fn time_mark(text: &'static str, ink: &Ink) -> gpui::Div {
    label(text, 12.0, ink.dim).my(z(5.0))
}

fn bubble(text: &'static str, ink: &Ink) -> gpui::Div {
    h_flex().w_full().justify_end().my(z(6.0)).child(
        div()
            .px(z(13.0))
            .py(z(8.0))
            .rounded(z(14.0))
            .bg(hsla(ink.card))
            .child(label(text, 13.5, ink.fg)),
    )
}

/// A row of a work card: an icon, a verb in the muted colour and what it acted on.
fn work_row(path: &'static str, verb: &'static str, what: &'static str, ink: &Ink) -> gpui::Div {
    h_flex()
        .h(z(30.0))
        .px(z(12.0))
        .gap(z(9.0))
        .items_center()
        .child(icon(path, 14.0, ink.muted))
        .child(label(verb, 13.0, ink.muted))
        .child(label(what, 13.0, ink.fg).flex_1().min_w_0())
}

fn work_card(rows: Vec<AnyElement>, ink: &Ink) -> gpui::Div {
    let line = hsla(ink.line);
    v_flex()
        .w_full()
        .rounded(z(12.0))
        .border_1()
        .border_color(line)
        .bg(hsla(ink.card))
        .overflow_hidden()
        .children(rows.into_iter().enumerate().map(move |(index, row)| {
            div()
                .w_full()
                .when(index > 0, |this| this.border_t_1().border_color(line))
                .child(row)
        }))
}

const CODE: [(&str, &str, bool); 7] = [
    ("41", "    fn preview(&mut self, level: Preset) {", false),
    ("42", "+       self.draft = writes_all(level);", true),
    ("43", "+       self.preview.animate(&self.draft);", true),
    ("44", "        self.show_apply_bar();", false),
    ("45", "    }", false),
    ("46", "", false),
    ("47", "    fn apply(&mut self) {", false),
];
const CODE_HEIGHT: f32 = 7.0 * 16.0 + 12.0;

fn chat(ink: &Ink, state: &PreviewState) -> gpui::Div {
    let folded_line = h_flex()
        .h_full()
        .gap(z(7.0))
        .items_center()
        .child(icon("titlebar/file-text.svg", 14.0, ink.muted))
        .child(label("1 report", 13.0, ink.fg))
        .child(inline_part(
            state.look(Part::FoldedToolCount),
            84.0,
            0.0,
            ink,
            label("· 2 tool calls", 13.0, ink.muted),
        ))
        .child(label("6:04 PM", 13.0, ink.muted));
    let report = h_flex()
        .h(z(30.0))
        .px(z(12.0))
        .gap(z(9.0))
        .items_center()
        .child(icon("titlebar/file.svg", 14.0, ink.pink))
        .child(label("Experience presets: what each level sets", 13.0, ink.fg).flex_1())
        .child(label("6:04 PM", 12.5, ink.muted));
    let open_work = work_card(
        vec![
            report.into_any_element(),
            work_row("titlebar/file-search.svg", "Read", "experience.rs", ink).into_any_element(),
            work_row(
                "titlebar/search.svg",
                "Searched",
                "settings-catalog for \"Quiet View\"",
                ink,
            )
            .into_any_element(),
        ],
        ink,
    );
    let preview_look = state.look(Part::FileEditPreview);
    let code = v_flex()
        .w_full()
        .h(z(CODE_HEIGHT))
        .px(z(12.0))
        .py(z(6.0))
        .bg(hsla(css_mix(ink.fg, 0.03, ink.window)))
        .children(CODE.map(|(number, line, added)| {
            h_flex()
                .h(z(16.0))
                .gap(z(12.0))
                .text_size(z(11.5))
                .font_family("Menlo")
                .child(div().w(z(18.0)).text_color(hsla(ink.dim)).child(number))
                .child(
                    div()
                        .whitespace_nowrap()
                        .text_color(hsla(if added { ink.added } else { ink.fg }))
                        .child(line),
                )
        }));
    let edited = h_flex()
        .h(z(30.0))
        .px(z(12.0))
        .gap(z(9.0))
        .items_center()
        .child(icon("titlebar/file-diff.svg", 14.0, ink.muted))
        .child(label("Edited", 13.0, ink.muted))
        .child(label("experience/page.rs", 13.0, ink.fg).flex_1())
        .child(label("+12", 12.0, ink.added))
        .child(label("−3", 12.0, ink.removed));
    let thinking = h_flex()
        .h(z(30.0))
        .px(z(12.0))
        .gap(z(9.0))
        .items_center()
        .child(icon("titlebar/sparkles.svg", 14.0, ink.claude))
        .child(label("Thinking…", 13.0, ink.muted));
    let running_open = work_card(
        vec![
            work_row(
                "titlebar/file-search.svg",
                "Read",
                "experience/page.rs",
                ink,
            )
            .into_any_element(),
            v_flex()
                .child(edited)
                .child(block_part(preview_look, CODE_HEIGHT, ink, code))
                .into_any_element(),
            work_row(
                "titlebar/terminal-2.svg",
                "Ran",
                "cargo test -p settings-catalog",
                ink,
            )
            .into_any_element(),
            thinking.into_any_element(),
        ],
        ink,
    );
    // The card's rows, its edges and the space under it, plus the code while it shows.
    let running_open_height = 4.0 * 31.0 + 10.0 + CODE_HEIGHT * preview_look.size;
    let one_line = |path: &'static str, color: Rgba, text: &'static str| {
        h_flex()
            .h_full()
            .gap(z(8.0))
            .items_center()
            .child(icon(path, 14.0, color))
            .child(label(text, 13.0, ink.muted))
    };
    v_flex()
        .flex_1()
        .min_h_0()
        // Like a chat scrolled to its end: what does not fit goes off the top.
        .overflow_hidden()
        .justify_end()
        .px(z(40.0))
        .pb(z(10.0))
        .child(time_mark("6:02 PM", ink))
        .child(bubble("Can you make the Experience page clearer?", ink))
        .child(block_part(
            state.look(Part::FoldedLine),
            26.0,
            ink,
            folded_line,
        ))
        .child(block_part(
            state.look(Part::OpenWork),
            104.0,
            ink,
            v_flex().pt(z(4.0)).child(open_work),
        ))
        .child(
            div().my(z(6.0)).child(
                h_flex()
                    .gap(z(4.0))
                    .child(label("Done:", 13.5, ink.fg).font_weight(FontWeight::SEMIBOLD))
                    .child(label(
                        "the page says what each level shows, with a live preview.",
                        13.5,
                        ink.fg,
                    )),
            ),
        )
        .child(time_mark("6:07 PM", ink))
        .child(bubble("Let me try a level before it saves", ink))
        .child(block_part(
            state.look(Part::RunningCalm),
            26.0,
            ink,
            one_line("titlebar/sparkles.svg", ink.claude, "Working · 1m 12s"),
        ))
        .child(block_part(
            state.look(Part::RunningFolded),
            26.0,
            ink,
            one_line(
                "titlebar/sparkles.svg",
                ink.claude,
                "3 tool calls · Working 1m 12s",
            ),
        ))
        .child(block_part(
            state.look(Part::RunningOpen),
            running_open_height,
            ink,
            v_flex().pt(z(4.0)).child(running_open),
        ))
}

fn chip(path: &'static str, text: &'static str, color: Rgba, ink: &Ink) -> gpui::Div {
    h_flex()
        .h(z(26.0))
        .px(z(10.0))
        .gap(z(6.0))
        .items_center()
        .rounded_full()
        .border_1()
        .border_color(hsla(if color == ink.gold {
            css_fade(ink.gold, 0.55)
        } else {
            ink.line
        }))
        .child(icon(
            path,
            13.0,
            if color == ink.gold {
                ink.gold
            } else {
                ink.muted
            },
        ))
        .child(label(text, 12.5, color))
}

fn button(path: &'static str, ink: &Ink) -> gpui::Div {
    div()
        .flex_shrink_0()
        .size(z(28.0))
        .flex()
        .items_center()
        .justify_center()
        .child(icon(path, 17.0, ink.muted))
}

/// The context ring, a quarter full, as `native_chat/context_meter.rs` draws it.
fn context_ring(ink: &Ink) -> AnyElement {
    let track = hsla(css_fade(ink.muted, 0.3));
    let fill = hsla(ink.muted);
    gpui::canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let center = bounds.center();
            let radius = 6.5 * SCALE;
            for (fraction, color) in [(1.0_f32, track), (0.3, fill)] {
                let mut path = gpui::PathBuilder::stroke(px(2.0 * SCALE));
                let steps = (fraction * 96.0).ceil() as usize;
                for step in 0..=steps {
                    let angle = -std::f32::consts::FRAC_PI_2
                        + std::f32::consts::TAU * fraction * step as f32 / steps as f32;
                    let point =
                        center + gpui::point(px(angle.cos() * radius), px(angle.sin() * radius));
                    if step == 0 {
                        path.move_to(point);
                    } else {
                        path.line_to(point);
                    }
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, color);
                }
            }
        },
    )
    .size(z(17.0))
    .into_any_element()
}

fn chat_box(ink: &Ink, state: &PreviewState) -> gpui::Div {
    let ring = context_ring(ink);
    let row = h_flex()
        .h(z(40.0))
        .px(z(10.0))
        .items_center()
        .child(
            h_flex()
                .gap(z(6.0))
                .items_center()
                .child(icon("agent-icons/claude.svg", 15.0, ink.claude))
                .child(label("Opus 5.5", 13.5, ink.fg))
                .child(label("High", 13.5, ink.muted))
                .child(icon("titlebar/chevron-down.svg", 12.0, ink.muted)),
        )
        .child(inline_part(
            state.look(Part::ModePicker),
            28.0,
            6.0,
            ink,
            icon("titlebar/mode-advance.svg", 16.0, ink.mode),
        ))
        .child(inline_part(
            state.look(Part::ContextRing),
            28.0,
            6.0,
            ink,
            ring,
        ))
        .child(div().flex_1())
        .child(button("titlebar/dots.svg", ink))
        .child(button("titlebar/paperclip.svg", ink).ml(z(4.0)))
        .child(inline_part(
            state.look(Part::Maximize),
            28.0,
            4.0,
            ink,
            icon("titlebar/maximize.svg", 17.0, ink.muted),
        ))
        .child(inline_part(
            state.look(Part::TerminalButton),
            28.0,
            4.0,
            ink,
            icon("titlebar/terminal-2.svg", 17.0, ink.muted),
        ))
        .child(button("titlebar/microphone.svg", ink).ml(z(4.0)))
        .child(
            div()
                .ml(z(6.0))
                .size(z(28.0))
                .rounded(z(7.0))
                .bg(hsla(ink.send))
                .flex()
                .items_center()
                .justify_center()
                .child(icon("titlebar/arrow-up.svg", 15.0, ink.on_send)),
        );
    div().flex_shrink_0().px(z(40.0)).pb(z(16.0)).child(
        v_flex()
            .rounded(z(16.0))
            .border_1()
            .border_color(hsla(css_mix(ink.fg, 0.14, ink.window)))
            .bg(hsla(ink.card))
            .child(
                h_flex()
                    .px(z(10.0))
                    .py(z(8.0))
                    .gap(z(8.0))
                    .border_b_1()
                    .border_color(hsla(ink.line))
                    .child(chip("titlebar/brand-apple.svg", "Ghostex", ink.fg, ink))
                    .child(chip("titlebar/users-group.svg", "Workers 4", ink.fg, ink))
                    .child(chip("titlebar/plane.svg", "Autopilot", ink.fg, ink))
                    .child(chip("titlebar/sparkles.svg", "Boost", ink.gold, ink)),
            )
            .child(div().px(z(14.0)).pt(z(12.0)).pb(z(18.0)).child(label(
                "try Chill on the chat box",
                14.0,
                ink.fg,
            )))
            .child(row),
    )
}
