//! The native GBots Team window: Import Team (a team file's preview, then the import and its
//! Undo) and Export Team (what to write, then the saved file and what was left out of it), both
//! from Main Hub's GBots heading.
//!
//! Like every native modal view it depends only on gpui, gpui-component, serde_json and the shared
//! modal kit, so the `native-modal-demo` binary and the web build include it.
//!
//! CDXC:GBots 2026-11-09 DECISION:
//! Plan piece 13: importing a team file shows a review first and nothing is added before you press Import; the review lists the GBots (with " 2" on names already taken), their departments, their routines (they arrive paused), the approval level they start at, what the file left out, what this Ghostex ignored and anything that keeps it from importing. After the import the same window offers Undo, which removes the whole import. Team files live on the GBots heading's menu (Import Team…, Export Team…), next to New GBot…, rather than in Settings, so there is one place for them.
//! SEE-ALSO: apps/desktop/src/app/gbot_team_modal_lifecycle.rs (file pickers, gxserver calls), server/src/gbots/team_file/ (the format, export, import, Undo).
mod export_view;
mod import_view;

use super::native_modal_kit::*;
use gpui::{
    AnyElement, AnyWindowHandle, App, AppContext as _, Context, Entity, FocusHandle,
    InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, Render, Rgba,
    Styled as _, WeakEntity, Window, div, px, rgb, svg,
};
use gpui_component::h_flex;
use gpui_component::input::{InputState, TextareaState};
use serde_json::Value;
use std::rc::Rc;

pub(crate) const GBOT_TEAM_MODAL_WIDTH: f32 = 560.0;
pub(crate) const GBOT_TEAM_MODAL_INITIAL_HEIGHT: f32 = 560.0;

/// How the window opens.
pub(crate) enum GbotTeamStart {
    /// A team or preset file's preview (`teamPreview`), its text kept for the import.
    Preview { text: String, preview: Value },
    /// A box to paste a file into (the web build, which has no file picker), with why the last
    /// file did not read.
    Paste { error: Option<String> },
    /// Export: the GBots and departments as `list` answered them; `can_save` when a save panel
    /// can open (not in the browser).
    Export { list: Value, can_save: bool },
}

pub(crate) struct GbotTeamModalConfig {
    pub(crate) palette: ModalPalette,
    pub(crate) start: GbotTeamStart,
}

/// What the window asks of its host; each answers through its reply.
pub(crate) enum GbotTeamModalCommand {
    Preview {
        text: String,
        reply: GbotTeamReply,
    },
    Import {
        text: String,
        reply: GbotTeamReply,
    },
    KeepPreset {
        text: String,
        reply: GbotTeamReply,
    },
    Undo {
        change: String,
        reply: GbotTeamReply,
    },
    /// `teamExport` with `params`; then a save panel (`save`) or the clipboard.
    Export {
        params: Value,
        save: bool,
        reply: GbotTeamReply,
    },
    /// `presetSave` of a GBot.
    KeepBotPreset {
        bot: String,
        notes: bool,
        reply: GbotTeamReply,
    },
    Close,
}

pub(crate) type GbotTeamModalHost = Rc<dyn Fn(GbotTeamModalCommand, &mut App)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TeamOp {
    Preview,
    Import,
    KeepPreset,
    Undo,
    Export { saved: bool },
    KeepBotPreset,
}

/// Hands the window gxserver's answer (or its refusal) for the op it asked for.
pub(crate) struct GbotTeamReply {
    view: WeakEntity<GpuiGbotTeamModalWindow>,
    window: AnyWindowHandle,
    op: TeamOp,
}

impl GbotTeamReply {
    pub(crate) fn finish(self, result: Result<Value, String>, cx: &mut App) {
        let view = self.view;
        let op = self.op;
        let _ = self.window.update(cx, |_root, window, cx| {
            view.update(cx, |modal, cx| modal.finish(op, result, window, cx))
        });
    }
}

pub(super) enum Stage {
    Paste,
    Preview,
    Imported(Value),
    Undone(Value),
    PresetKept(Value),
    Export,
    Exported { answer: Value, saved: bool },
}

pub(crate) struct GpuiGbotTeamModalWindow {
    pub(super) host: GbotTeamModalHost,
    pub(super) palette: ModalPalette,
    pub(super) stage: Stage,
    /// The file's text and its preview.
    pub(super) text: String,
    pub(super) preview: Value,
    pub(super) busy: Option<TeamOp>,
    pub(super) error: Option<String>,
    pub(super) paste_input: Entity<TextareaState>,
    pub(super) name_input: Entity<InputState>,
    pub(super) export: export_view::ExportChoice,
    pub(super) list: Value,
    pub(super) can_save: bool,
    pub(super) fit: ModalFit,
    focus_handle: FocusHandle,
}

impl GpuiGbotTeamModalWindow {
    pub(crate) fn new(
        config: GbotTeamModalConfig,
        host: GbotTeamModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let paste_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Paste a .gbots-team.json or .gbots-preset.json file here")
        });
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder("e.g. Travel desk"));
        let (stage, text, preview, error, list, can_save) = match config.start {
            GbotTeamStart::Preview { text, preview } => {
                (Stage::Preview, text, preview, None, Value::Null, false)
            }
            GbotTeamStart::Paste { error } => (
                Stage::Paste,
                String::new(),
                Value::Null,
                error,
                Value::Null,
                false,
            ),
            GbotTeamStart::Export { list, can_save } => (
                Stage::Export,
                String::new(),
                Value::Null,
                None,
                list,
                can_save,
            ),
        };
        Self {
            host,
            palette: config.palette,
            stage,
            text,
            preview,
            busy: None,
            error,
            paste_input,
            name_input,
            export: export_view::ExportChoice::default(),
            list,
            can_save,
            fit: ModalFit::new(),
            focus_handle: cx.focus_handle(),
        }
    }

    pub(super) fn reply(
        &self,
        op: TeamOp,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> GbotTeamReply {
        GbotTeamReply {
            view: cx.entity().downgrade(),
            window: window.window_handle(),
            op,
        }
    }

    /// Sends `command` (built with a reply for `op`) and shows the window busy until it answers.
    pub(super) fn send(
        &mut self,
        op: TeamOp,
        command: impl FnOnce(GbotTeamReply) -> GbotTeamModalCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy.is_some() {
            return;
        }
        self.busy = Some(op);
        self.error = None;
        cx.notify();
        let reply = self.reply(op, window, cx);
        (self.host)(command(reply), cx);
    }

    fn finish(
        &mut self,
        op: TeamOp,
        result: Result<Value, String>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.busy = None;
        match result {
            Err(message) => self.error = Some(message),
            Ok(answer) => match op {
                TeamOp::Preview => {
                    self.preview = answer["preview"].clone();
                    self.stage = Stage::Preview;
                }
                TeamOp::Import => self.stage = Stage::Imported(answer),
                TeamOp::Undo => self.stage = Stage::Undone(answer),
                TeamOp::KeepPreset | TeamOp::KeepBotPreset => {
                    self.stage = Stage::PresetKept(answer)
                }
                TeamOp::Export { saved } => self.stage = Stage::Exported { answer, saved },
            },
        }
        self.fit.refit();
        cx.notify();
    }

    /// The native-modal-demo's states past a click (src/bin/native_modal_demo/gbot_team.rs).
    #[allow(dead_code)]
    pub(crate) fn demo_jump(&mut self, state: &str, window: &mut Window, cx: &mut Context<Self>) {
        let _ = window;
        match state {
            "imported" | "undone" => {
                let answer = serde_json::json!({
                    "changeId": "c_demo",
                    "name": "Travel desk",
                    "bots": self.preview["bots"],
                    "routines": self.preview["routines"],
                    "skipped": [],
                });
                self.stage = Stage::Imported(answer);
                if state == "undone" {
                    self.stage = Stage::Undone(serde_json::json!({ "done": {
                        "removedBots": ["Trip planner", "Researcher 2", "Itinerary writer"],
                        "removedRoutines": 2,
                        "removedDepartments": ["Travel"],
                    }}));
                }
            }
            "export-bot" => {
                self.export.scope = 2;
                self.export.pick = Some("b_researcher".to_string());
                self.export.notes = true;
            }
            "exported" => {
                self.stage = Stage::Exported {
                    answer: serde_json::json!({
                        "kind": "team", "bots": 3, "departments": 1, "routines": 2,
                        "savedTo": "~/Downloads/travel-desk.gbots-team.json",
                        "leftOut": ["1 secret redacted in Researcher's instructions"],
                        "routinesArrivePaused": true,
                    }),
                    saved: true,
                };
            }
            _ => return,
        }
        self.fit.refit();
        cx.notify();
    }

    pub(super) fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.remove_window();
        (self.host)(GbotTeamModalCommand::Close, cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key.as_str() == "escape" {
            cx.stop_propagation();
            self.close(window, cx);
        }
    }
}

/// `#rrggbb` as a colour; the GBots periwinkle when it does not parse.
pub(super) fn hex_color(hex: &str) -> Rgba {
    u32::from_str_radix(hex.trim().trim_start_matches('#'), 16)
        .map(rgb)
        .unwrap_or_else(|_| rgb(0x9fb0ff))
}

/// A GBot's icon in its colour.
pub(super) fn bot_icon(bot: &Value) -> AnyElement {
    let icon = bot["icon"]
        .as_str()
        .filter(|icon| !icon.is_empty())
        .unwrap_or("robot");
    svg()
        .flex_shrink_0()
        .size(px(16.0))
        .path(format!("titlebar/{icon}.svg"))
        .text_color(hex_color(bot["colorHex"].as_str().unwrap_or_default()))
        .into_any_element()
}

pub(super) fn text_of<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

pub(super) fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// A titled list of lines under a panel (left out, ignored, notes), or nothing when empty.
pub(super) fn line_list(
    p: &ModalPalette,
    title: &str,
    lines: &[String],
    tone: Option<Rgba>,
) -> Option<AnyElement> {
    if lines.is_empty() {
        return None;
    }
    let color = tone.unwrap_or(p.muted);
    Some(
        gpui_component::v_flex()
            .gap(px(4.0))
            .child(modal_section_title(p, title.to_string()))
            .children(lines.iter().map(|line| {
                h_flex()
                    .gap(px(6.0))
                    .items_start()
                    .text_size(px(12.0))
                    .line_height(px(17.4))
                    .text_color(color)
                    .child(div().flex_shrink_0().child("•"))
                    .child(div().min_w_0().child(line.clone()))
            }))
            .into_any_element(),
    )
}

impl Render for GpuiGbotTeamModalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let (content, footer) = match &self.stage {
            Stage::Export | Stage::Exported { .. } => self.render_export(window, cx),
            _ => self.render_import(window, cx),
        };
        modal_shell(
            &p,
            "ghostex-gpui-gbot-team-modal",
            &self.focus_handle,
            &self.fit,
            Self::on_key_down,
            content,
            footer,
            None,
            cx,
        )
    }
}

impl ModalCornerClose for GpuiGbotTeamModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close(window, cx);
    }
}
