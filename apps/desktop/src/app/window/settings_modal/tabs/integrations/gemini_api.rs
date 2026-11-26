//! The Gemini API CLI explainer: the (i) on its Tools row opens a sheet over Settings that says
//! what Antigravity CLI already does on the user's Gemini plan, what Google's `gemini-api` adds on
//! an API key, and how to start (`gemini-api configure` once).
//!
//! CDXC:ManagedTools 2026-11-26 DECISION:
//! User: "tell users about configure thing so they run it when they wanna use gemini cli. mention the things that gemini cli gives above agy please and also mention the things u can get from agy cli itself with your subscription. i want a good modal that explains this integration please that appears when i hit i on it in settings (not a tooltip, im kind of over tooltips they're too wordy). table in that modal in settings would be awesome". From the mockup they chose an (i) that shows on hover like the other rows' (i), a sheet with one comparison table and three steps, keeping Run in terminal and the Google AI Studio link and dropping a beta note. The sheet is the nested Settings dialog (`settings_dialog_sheet`, the Connect your accounts guide's sizes). What Antigravity does on the plan was tested in an Antigravity thread on 2026-11-25 (agent-work research page).
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{run_in_terminal_button, settings_dialog_sheet, settings_icon};
use super::super::super::palette::SettingsPalette;
use super::super::super::store::post_store_message;
use super::super::accounts::widgets::copy_command;
use super::IntegrationsTab;
use super::tools::{id, text};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, ElementId, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div,
    px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};

/// The managed tool this explainer belongs to.
pub(super) const TOOL: &str = "geminiApi";
const AI_STUDIO_KEYS: &str = "https://aistudio.google.com/apikey";
const COPY_KEY: &str = "gemini-api-configure";
const CHECK: &str = "modals/settings/check.svg";
const NONE: &str = "modals/settings/minus.svg";
const EXTERNAL: &str = "modals/settings/external-link.svg";
const INFO: &str = "modals/settings/info-circle.svg";
/// The two provider columns' width.
const COLUMN: f32 = 168.0;

/// What one row of the table says for Antigravity and for gemini-api.
enum Cell {
    Yes,
    No,
    Words(&'static str),
}

/// (group, rows): each row is what the user wants, Antigravity on the plan, gemini-api on a key.
const TABLE: [(&str, &[(&str, Cell, Cell)]); 2] = [
    (
        "Already on your plan",
        &[
            (
                "Ask about local PDFs, images, audio and video",
                Cell::Yes,
                Cell::Yes,
            ),
            ("Transcripts and SRT captions", Cell::Yes, Cell::Yes),
            ("Make and edit images", Cell::Yes, Cell::Yes),
            (
                "Research in the background",
                Cell::Words("While the session is open"),
                Cell::Words("Deep Research jobs you fetch later by id"),
            ),
        ],
    ),
    (
        "Only with gemini-api",
        &[
            ("Ask about a YouTube video by its link", Cell::No, Cell::Yes),
            (
                "Gemini voiceovers, also with two speakers",
                Cell::No,
                Cell::Yes,
            ),
            ("Music", Cell::No, Cell::Yes),
            ("Short video", Cell::No, Cell::Yes),
        ],
    ),
];

impl IntegrationsTab {
    /// The (i) beside the row's name, shown while the pointer is on the row like the other rows'
    /// (i); a click opens the explainer instead of a tooltip.
    pub(super) fn gemini_api_info_button(
        &mut self,
        p: &SettingsPalette,
        group: &SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let open = self.managed.explainer.is_some();
        div()
            .id("tool-geminiApi-info")
            .flex_shrink_0()
            .size(px(20.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .cursor_pointer()
            .when(!open, |this| this.opacity(0.0))
            .group_hover(group.clone(), |this| this.opacity(1.0))
            .hover(|this| this.bg(hsla(p.raised_hover)))
            .on_click(cx.listener(|page, _: &ClickEvent, window, cx| {
                let focus = cx.focus_handle();
                focus.focus(window, cx);
                page.managed.explainer = Some(focus);
                cx.notify();
            }))
            .child(settings_icon(INFO, 15.0, p.muted))
            .into_any_element()
    }

    /// The open explainer over the whole Settings window, or `None`.
    pub(super) fn gemini_api_explainer(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let focus = self.managed.explainer.clone()?;
        let tool = self
            .managed
            .tools
            .iter()
            .flatten()
            .find(|tool| id(tool) == TOOL)
            .cloned()?;
        let p = &p.nested_dialog();
        let border = modal_rgba(
            if p.light { 0x000000 } else { 0xffffff },
            if p.light { 0.14 } else { 0.11 },
        );
        let description = paragraph(
            p,
            "Google's gemini-api command lets any agent in Ghostex call Gemini from the shell. It runs on a Gemini API key, which Google bills separately from your Gemini subscription.",
        );
        let body = vec![
            part(vec![
                heading(p, "What you get"),
                table(p, border),
                paragraph(
                    p,
                    "For the first group, an Antigravity thread already does it on what you pay for. Reach for gemini-api for the rest.",
                ),
            ]),
            part(vec![
                heading(p, "How to start"),
                self.steps(p, &tool, border, cx),
            ]),
        ];
        let max_height = f32::from(window.viewport_size().height) * 0.9;
        Some(settings_dialog_sheet(
            p,
            "gemini-api-explainer",
            &focus,
            "Gemini API CLI",
            Some(description),
            640.0,
            Some(max_height),
            24.0,
            20.0,
            true,
            body,
            None,
            |page: &mut Self, _window, cx| {
                page.managed.explainer = None;
                cx.notify();
            },
            window,
            cx,
        ))
    }

    fn steps(
        &mut self,
        p: &SettingsPalette,
        tool: &Value,
        border: gpui::Rgba,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let install = match (tool["installed"] == true, text(tool, "version")) {
            (true, Some(version)) => format!("Version {version} is installed on this computer."),
            (true, None) => "It is installed on this computer.".to_string(),
            (false, _) => "Use the Install button on this row.".to_string(),
        };
        let command = text(&tool["configure"], "command")
            .unwrap_or("gemini-api configure")
            .to_string();
        let mut key_parts = vec![paragraph(
            p,
            "Run this once in any terminal and paste your Gemini API key. gemini-api stores it for you, so you never set it again.",
        )];
        let copied = self.copied == Some(COPY_KEY);
        let copy_text = command.clone();
        let mut command_line =
            h_flex()
                .w_full()
                .items_center()
                .gap(px(8.0))
                .child(div().flex_1().min_w_0().child(copy_command(
                    p,
                    "gemini-api-configure",
                    &command,
                    copied,
                    move |page: &mut Self, _window, cx| {
                        page.copy_command(COPY_KEY, copy_text.clone(), cx);
                    },
                    cx,
                )));
        // Only when that computer's gxserver offers the command (an older one sends no `configure`).
        if let Some(command_id) = text(&tool["configure"], "runInTerminal") {
            command_line = command_line.child(run_in_terminal_button(
                p,
                "gemini-api-configure-run",
                &self.store,
                command_id.to_string(),
                self.shown_machine.clone(),
                cx,
            ));
        }
        key_parts.push(command_line.into_any_element());
        key_parts.push(
            h_flex()
                .items_center()
                .gap(px(4.0))
                .text_size(px(12.0))
                .line_height(px(19.2))
                .text_color(hsla(p.muted))
                .child("No key yet?")
                .child(
                    h_flex()
                        .id("gemini-api-ai-studio")
                        .items_center()
                        .gap(px(4.0))
                        .cursor_pointer()
                        .text_color(hsla(p.foreground))
                        .hover(|this| this.underline())
                        .on_click(cx.listener(|page, _: &ClickEvent, _window, cx| {
                            post_store_message(
                                &page.store,
                                json!({ "type": "openExternalUrl", "url": AI_STUDIO_KEYS }),
                                cx,
                            );
                        }))
                        .child("Get one in Google AI Studio")
                        .child(settings_icon(EXTERNAL, 13.0, p.muted)),
                )
                .into_any_element(),
        );
        v_flex()
            .w_full()
            .gap(px(14.0))
            .child(step(
                p,
                border,
                1,
                "Install it",
                vec![paragraph(p, &install)],
            ))
            .child(step(p, border, 2, "Add your key once", key_parts))
            .child(step(
                p,
                border,
                3,
                "Ask any agent",
                vec![paragraph(
                    p,
                    "Claude, Codex and the others find gemini-api on their own: ask for a voiceover, music or a question about a YouTube link.",
                )],
            ))
            .into_any_element()
    }
}

fn heading(p: &SettingsPalette, label: &str) -> AnyElement {
    div()
        .text_size(px(13.0))
        .line_height(px(18.57))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(hsla(p.foreground))
        .child(label.to_string())
        .into_any_element()
}

fn paragraph(p: &SettingsPalette, words: &str) -> AnyElement {
    div()
        .text_size(px(12.0))
        .line_height(px(19.2))
        .text_color(hsla(p.muted))
        .child(words.to_string())
        .into_any_element()
}

fn part(children: Vec<AnyElement>) -> AnyElement {
    v_flex()
        .w_full()
        .gap(px(10.0))
        .children(children)
        .into_any_element()
}

/// One numbered step: a ringed number, its name, and what to do.
fn step(
    p: &SettingsPalette,
    border: gpui::Rgba,
    number: u8,
    name: &str,
    parts: Vec<AnyElement>,
) -> AnyElement {
    h_flex()
        .w_full()
        .items_start()
        .gap(px(10.0))
        .child(
            div()
                .flex_shrink_0()
                .size(px(20.0))
                .rounded_full()
                .border_1()
                .border_color(hsla(border))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(11.0))
                .text_color(hsla(p.foreground))
                .child(number.to_string()),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(6.0))
                .child(
                    div()
                        .text_size(px(13.0))
                        .line_height(px(20.0))
                        .text_color(hsla(p.foreground))
                        .child(name.to_string()),
                )
                .children(parts),
        )
        .into_any_element()
}

/// The comparison: a header with each provider's logo and what it runs on, then the two groups.
fn table(p: &SettingsPalette, border: gpui::Rgba) -> AnyElement {
    let provider = |logo: &'static str, name: &str, on: &str| {
        v_flex()
            .w(px(COLUMN))
            .flex_shrink_0()
            .gap(px(3.0))
            .child(
                h_flex()
                    .items_center()
                    .gap(px(7.0))
                    .text_size(px(13.0))
                    .line_height(px(18.0))
                    .text_color(hsla(p.foreground))
                    .child(settings_icon(logo, 15.0, p.foreground))
                    .child(name.to_string()),
            )
            .child(
                div()
                    .text_size(px(11.5))
                    .line_height(px(15.0))
                    .text_color(hsla(p.muted))
                    .child(on.to_string()),
            )
    };
    let mut rows: Vec<AnyElement> = vec![
        h_flex()
            .w_full()
            .items_end()
            .px(px(12.0))
            .py(px(10.0))
            .gap(px(12.0))
            .child(div().flex_1().min_w_0())
            .child(provider(
                "agent-icons/antigravity-cli.svg",
                "Antigravity CLI",
                "Your Gemini plan, no key",
            ))
            .child(provider(
                "agent-icons/gemini.svg",
                "gemini-api",
                "Your API key, billed per use",
            ))
            .into_any_element(),
    ];
    for (group, entries) in TABLE {
        rows.push(
            div()
                .w_full()
                .px(px(12.0))
                .py(px(6.0))
                .border_t_1()
                .border_color(hsla(border))
                .bg(hsla(p.foreground_alpha(0.04)))
                .text_size(px(11.5))
                .line_height(px(15.0))
                .text_color(hsla(p.muted))
                .child(group)
                .into_any_element(),
        );
        for (want, agy, api) in entries {
            rows.push(
                h_flex()
                    .w_full()
                    .items_center()
                    .px(px(12.0))
                    .py(px(9.0))
                    .gap(px(12.0))
                    .border_t_1()
                    .border_color(hsla(border))
                    .text_size(px(13.0))
                    .line_height(px(18.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_color(hsla(p.foreground))
                            .child(*want),
                    )
                    .child(cell(p, agy))
                    .child(cell(p, api))
                    .into_any_element(),
            );
        }
    }
    v_flex()
        .id(ElementId::Name("gemini-api-table".into()))
        .w_full()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(border))
        .overflow_hidden()
        .children(rows)
        .into_any_element()
}

fn cell(p: &SettingsPalette, value: &Cell) -> AnyElement {
    let column = div().w(px(COLUMN)).flex_shrink_0();
    match value {
        Cell::Yes => column
            .child(settings_icon(CHECK, 15.0, p.foreground))
            .into_any_element(),
        Cell::No => column
            .child(settings_icon(NONE, 15.0, p.muted))
            .into_any_element(),
        Cell::Words(words) => column
            .text_color(hsla(p.foreground))
            .child(*words)
            .into_any_element(),
    }
}
