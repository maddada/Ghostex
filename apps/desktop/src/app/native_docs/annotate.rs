//! Annotate on HTML files in Files: the header's buttons that drive the page's Agentation (whose
//! own floating toolbar is hidden), the requests the page sends about it, the state Files keeps
//! for it, and the Annotate hotkey (which also comments on a Markdown file's selection).
//!
//! CDXC:Docs 2026-11-14 SEE-ALSO: the decision is in apps/desktop/views/manage/preview/html-viewer.tsx
//! (`buildManageAgentationBootstrapScript`); the hotkey's in packages/settings-catalog/src/hotkey_definitions.rs.

use std::cell::Cell;
use std::collections::{BTreeSet, HashMap};

use gpui::{
    AnyElement, Bounds, ClipboardItem, Context, IntoElement, ParentElement as _, Pixels,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use serde_json::{Value, json};

use super::annotate_storage as storage;
use super::files_list::{header_icon, header_tile};
use super::palette::DocsPalette;
use super::state::DocsFileKind;
use crate::GhostexGpuiApp;
use crate::app::context_menu::GpuiContextMenu;
use crate::app::helpers::{titlebar_tooltip, titlebar_tooltip_label};

/// The event the Files embed page listens for (`MANAGE_ANNOTATE_COMMAND_EVENT` in
/// apps/desktop/views/manage/preview/html-viewer.tsx).
const COMMAND_EVENT: &str = "ghostex-manage-annotate-command";
/// The catalog id of the Annotate hotkey.
pub(crate) const ANNOTATE_HOTKEY_ID: &str = "toggleAnnotate";
/// Below this width (the side panel) Layout, Settings, Open in Browser and Open With fold into ⋯
/// while Annotate is on (Q41).
pub(crate) const ANNOTATE_COMPACT_WIDTH: f32 = 560.0;

thread_local! {
    static MORE_MENU_ANCHOR: Cell<Bounds<Pixels>> = Cell::new(Bounds::default());
    static SETTINGS_MENU_ANCHOR: Cell<Bounds<Pixels>> = Cell::new(Bounds::default());
}

/// What the page last reported for the header's buttons (`onStateChange` in the fork).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct DocsAnnotateState {
    pub(crate) frozen: bool,
    pub(crate) layout: bool,
    pub(crate) marks_visible: bool,
    pub(crate) notes: u32,
    pub(crate) can_copy: bool,
    pub(crate) can_send: bool,
    pub(crate) can_clear: bool,
    pub(crate) sending: bool,
}

impl DocsAnnotateState {
    fn from_page(state: &Value) -> Self {
        let flag = |key: &str| state[key].as_bool().unwrap_or(false);
        Self {
            frozen: flag("frozen"),
            layout: flag("layout"),
            marks_visible: flag("marksVisible"),
            notes: state["notes"].as_u64().unwrap_or(0).min(u32::MAX as u64) as u32,
            can_copy: flag("canCopy"),
            can_send: flag("canSend"),
            can_clear: flag("canClear"),
            sending: state["sendState"].as_str() == Some("sending"),
        }
    }
}

/// Annotate's saved state as this run knows it; writes go to storage in the background, so this
/// copy answers the next page load even before they land.
#[derive(Default)]
pub(crate) struct DocsAnnotateStore {
    /// Project id to its HTML files with Annotate on, read once per project.
    files: HashMap<String, BTreeSet<String>>,
    /// `project\u{1f}path` to the file's notes.
    notes: HashMap<String, Value>,
    settings: Option<Value>,
}

impl DocsAnnotateStore {
    fn files(&mut self, project_id: &str) -> &mut BTreeSet<String> {
        self.files
            .entry(project_id.to_string())
            .or_insert_with(|| storage::read_annotated_files(project_id))
    }

    fn notes(&mut self, project_id: &str, path: &str) -> Option<Value> {
        let key = format!("{project_id}\u{1f}{path}");
        if let Some(notes) = self.notes.get(&key) {
            return Some(notes.clone());
        }
        let notes = storage::read_notes(project_id, path)?;
        self.notes.insert(key, notes.clone());
        Some(notes)
    }

    /// The saved settings over the defaults Ghostex gives Agentation.
    fn settings(&mut self) -> Value {
        let saved = self.settings.get_or_insert_with(storage::read_settings);
        let mut settings = json!({
            "outputDetail": "standard",
            "autoClearAfterCopy": true,
            "blockInteractions": true,
            "reactEnabled": true,
            "annotationColorId": "blue",
        });
        if let (Some(base), Some(saved)) = (settings.as_object_mut(), saved.as_object()) {
            for (key, value) in saved {
                base.insert(key.clone(), value.clone());
            }
        }
        settings
    }
}

/// Agentation's Output detail levels and mark colours, as its settings name them.
const OUTPUT_DETAILS: [(&str, &str); 4] = [
    ("compact", "Compact"),
    ("standard", "Standard"),
    ("detailed", "Detailed"),
    ("forensic", "Forensic"),
];
const MARK_COLOURS: [(&str, &str); 7] = [
    ("indigo", "Indigo"),
    ("blue", "Blue"),
    ("cyan", "Cyan"),
    ("green", "Green"),
    ("yellow", "Yellow"),
    ("orange", "Orange"),
    ("red", "Red"),
];

fn menu_action(command: &str, value: Value) -> Box<dyn gpui::Action> {
    super::actions::action(json!({ "type": "annotate", "command": command, "value": value }))
}

/// A configured hotkey chord as the page's own key listener reads it, or `None` when unbound.
fn page_hotkey_for(action_id: &str) -> Option<Value> {
    let snapshot = crate::shared_settings::shared_sidebar_settings_snapshot();
    let chord = crate::app::hotkeys::gpui_normal_hotkey_key_in(&snapshot, action_id)?;
    // GPUI's spelling: modifiers then the key, joined by `-` ("alt-shift-a", "cmd-ctrl-a").
    let keystroke = crate::gpui_keystroke_from_shared_hotkey(&chord)?;
    let mut parts: Vec<&str> = keystroke.split('-').collect();
    let key = parts
        .pop()
        .filter(|key| !key.is_empty())?
        .to_ascii_lowercase();
    let has = |name: &[&str]| parts.iter().any(|part| name.contains(part));
    let (control, alt, shift, platform) = (
        has(&["ctrl", "control"]),
        has(&["alt", "option"]),
        has(&["shift"]),
        has(&["cmd", "super", "win", "platform"]),
    );
    // Letters, digits and brackets by their physical key: Option and Shift change the character typed.
    let code = match key.as_bytes() {
        [letter @ b'a'..=b'z'] => format!("Key{}", (*letter as char).to_ascii_uppercase()),
        [digit @ b'0'..=b'9'] => format!("Digit{}", *digit as char),
        b"[" => "BracketLeft".to_string(),
        b"]" => "BracketRight".to_string(),
        _ => String::new(),
    };
    Some(json!({
        "key": key,
        "code": code,
        "ctrl": control,
        "alt": alt,
        "shift": shift,
        "meta": platform,
    }))
}

/// The configured Annotate chord as the page's own key listener reads it, or `None` when unbound.
fn page_hotkey() -> Option<Value> {
    page_hotkey_for(ANNOTATE_HOTKEY_ID)
}

impl GhostexGpuiApp {
    fn native_docs_annotate_project_id(&self) -> Option<String> {
        self.native_docs
            .project
            .as_ref()
            .map(|project| project.project_id.clone())
    }

    /// Reads whether Annotate was left on for `path`, once per document.
    fn native_docs_annotate_ensure_loaded(&mut self, path: &str) {
        let Some(project_id) = self.native_docs_annotate_project_id() else {
            return;
        };
        let loaded = self
            .native_docs
            .document(path)
            .is_none_or(|document| document.html_annotate_loaded);
        if loaded {
            return;
        }
        let on = self.native_docs.annotate.files(&project_id).contains(path);
        if let Some(document) = self.native_docs.document_mut(path) {
            document.html_annotate = on && document.page.is_none();
            document.html_annotate_loaded = true;
        }
    }

    /// Hands a header command to the page's Agentation (through the embed page).
    pub(crate) fn native_docs_annotate_command(
        &mut self,
        command: &str,
        value: Value,
        cx: &mut Context<Self>,
    ) {
        let detail = json!({ "command": command, "value": value });
        self.dispatch_project_workarea_json_event(
            crate::ProjectWorkareaCefSurfaceSlotKey::Manage,
            COMMAND_EVENT,
            &detail.to_string(),
            cx,
        );
    }

    /// Turns Annotate on or off for the HTML file at `path`, keeps that for the file, and tells
    /// its page (which stays loaded: Agentation is already there, idle).
    pub(crate) fn native_docs_toggle_annotate(&mut self, path: &str, cx: &mut Context<Self>) {
        self.native_docs_annotate_ensure_loaded(path);
        let Some(project_id) = self.native_docs_annotate_project_id() else {
            return;
        };
        let Some(document) = self.native_docs.document_mut(path) else {
            return;
        };
        if document.kind != DocsFileKind::Html || document.page.is_some() {
            return;
        }
        document.html_annotate = !document.html_annotate;
        document.html_annotate_loaded = true;
        let on = document.html_annotate;
        let files = self.native_docs.annotate.files(&project_id);
        if on {
            files.insert(path.to_string());
        } else {
            files.remove(path);
        }
        let files = files.clone();
        cx.background_executor()
            .spawn(async move { storage::write_annotated_files(&project_id, &files) })
            .detach();
        if self.native_docs.active.as_deref() == Some(path) {
            self.native_docs_annotate_command("setActive", Value::Bool(on), cx);
        }
        self.native_docs_notify(cx);
    }

    /// Ghostex's theme changed: the page's marks and popups follow it.
    pub(crate) fn native_docs_annotate_send_theme(&mut self, light: bool, cx: &mut Context<Self>) {
        let theme = if light { "light" } else { "dark" };
        self.native_docs_annotate_command("theme", json!(theme), cx);
    }

    /// The Annotate hotkey with the Files view focused: Annotate on or off for an HTML file, or a
    /// comment on the selected text of a Markdown file. Anything else lets it go on.
    pub(crate) fn native_docs_annotate_hotkey(&mut self, cx: &mut Context<Self>) {
        let Some(document) = self.native_docs.active_document() else {
            cx.propagate();
            return;
        };
        let path = document.path.to_string();
        match document.kind {
            DocsFileKind::Html if document.page.is_none() => {
                self.native_docs_toggle_annotate(&path, cx)
            }
            DocsFileKind::Markdown => self.native_docs_open_composer(None, None, "", cx),
            _ => cx.propagate(),
        }
    }

    /// The embed page's Annotate requests (`annotateLoad`, `annotateState`, `annotateNotes`,
    /// `annotateToggle`, `copyPageAnnotation`), already checked to name its own file
    /// (`manage_embed_page_rules.rs`). Returns the answer.
    pub(crate) fn native_docs_annotate_request(
        &mut self,
        action: &str,
        request: &Value,
        cx: &mut Context<Self>,
    ) -> Value {
        let request_id = request["requestId"].as_str().unwrap_or_default();
        let path = request["path"].as_str().unwrap_or_default().to_string();
        let answer = |extra: Value| {
            let mut answer = json!({ "action": action, "requestId": request_id });
            if let (Some(answer), Some(extra)) = (answer.as_object_mut(), extra.as_object()) {
                answer.extend(extra.clone());
            }
            answer
        };
        self.native_docs_annotate_ensure_loaded(&path);
        let Some(project_id) = self.native_docs_annotate_project_id() else {
            return answer(json!({ "error": "Files has no project." }));
        };
        let on = self
            .native_docs
            .document(&path)
            .is_some_and(|document| document.html_annotate);
        match action {
            "annotateLoad" => {
                let light = crate::app::helpers::CHROME_LIGHT_APPEARANCE
                    .load(std::sync::atomic::Ordering::Relaxed);
                let notes = self.native_docs.annotate.notes(&project_id, &path);
                let settings = self.native_docs.annotate.settings();
                let scroll = self
                    .native_docs
                    .document_mut(&path)
                    .and_then(|document| document.pending_scroll.take())
                    .map(|scroll| json!({ "x": scroll.x, "y": scroll.y }));
                answer(json!({ "annotate": {
                    "on": on,
                    "theme": if light { "light" } else { "dark" },
                    "settings": settings,
                    "notes": notes,
                    "hotkey": page_hotkey(),
                    "scroll": scroll,
                    "navHotkeys": {
                        "back": page_hotkey_for("navigateFilesBack"),
                        "forward": page_hotkey_for("navigateFilesForward"),
                    },
                } }))
            }
            "annotateState" => {
                let state = DocsAnnotateState::from_page(&request["annotateState"]);
                if let Some(document) = self.native_docs.document_mut(&path)
                    && document.html_annotate_state != state
                {
                    document.html_annotate_state = state;
                    self.native_docs_notify(cx);
                }
                answer(json!({}))
            }
            "annotateNotes" => {
                let notes = storage::capped_notes(request["notes"].clone());
                if !notes.is_array() {
                    return answer(json!({ "error": "These notes cannot be kept." }));
                }
                self.native_docs
                    .annotate
                    .notes
                    .insert(format!("{project_id}\u{1f}{path}"), notes.clone());
                cx.background_executor()
                    .spawn(async move { storage::write_notes(&project_id, &path, &notes) })
                    .detach();
                answer(json!({}))
            }
            "annotateToggle" => {
                self.native_docs_toggle_annotate(&path, cx);
                answer(json!({}))
            }
            "copyPageAnnotation" if on => {
                let markdown = request["markdown"].as_str().unwrap_or_default();
                if !markdown.is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(markdown.to_string()));
                    crate::app::helpers::gpui_copy_feedback(cx);
                }
                answer(json!({}))
            }
            _ => answer(json!({ "error": "Annotate is off for this file." })),
        }
    }

    /// Whether Annotate is on for `path` (Send from its page is refused otherwise).
    pub(crate) fn native_docs_annotate_on(&self, path: &str) -> bool {
        self.native_docs
            .document(path)
            .is_some_and(|document| document.html_annotate)
    }

    /// A row of the ⋯ or Settings menu.
    pub(crate) fn native_docs_annotate_menu_action(
        &mut self,
        command: &str,
        value: &Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match command {
            "toggleLayout" => self.native_docs_annotate_command("toggleLayout", Value::Null, cx),
            "setting" => {
                let (Some(key), Some(setting)) = (value["key"].as_str(), value.get("value")) else {
                    return;
                };
                let mut saved = self
                    .native_docs
                    .annotate
                    .settings
                    .take()
                    .unwrap_or_else(storage::read_settings);
                if let Some(object) = saved.as_object_mut() {
                    object.insert(key.to_string(), setting.clone());
                }
                self.native_docs.annotate.settings = Some(saved.clone());
                cx.background_executor()
                    .spawn(async move { storage::write_settings(&saved) })
                    .detach();
                self.native_docs_annotate_command("updateSettings", json!({ key: setting }), cx);
            }
            "openInBrowser" => {
                if let Some(path) = value.as_str() {
                    self.native_docs_open_externally(path, None, cx);
                }
            }
            _ => {}
        }
        let _ = window;
    }

    /// The Settings menu: what Annotate does, shared by every HTML file (Q43).
    fn native_docs_annotate_settings_menu(&mut self) -> GpuiContextMenu {
        let settings = self.native_docs.annotate.settings();
        let setting =
            |key: &str, value: Value| menu_action("setting", json!({ "key": key, "value": value }));
        let flag = |key: &str| settings[key].as_bool().unwrap_or(false);
        let mut detail = GpuiContextMenu::new();
        for (id, label) in OUTPUT_DETAILS {
            detail = detail.menu_with_check(
                label,
                settings["outputDetail"].as_str() == Some(id),
                setting("outputDetail", json!(id)),
            );
        }
        let mut colours = GpuiContextMenu::new();
        for (id, label) in MARK_COLOURS {
            colours = colours.menu_with_check(
                label,
                settings["annotationColorId"].as_str() == Some(id),
                setting("annotationColorId", json!(id)),
            );
        }
        GpuiContextMenu::new()
            .submenu_menu("Output detail", detail)
            .menu_with_check(
                "Clear notes after Copy or Send",
                flag("autoClearAfterCopy"),
                setting("autoClearAfterCopy", json!(!flag("autoClearAfterCopy"))),
            )
            .menu_with_check(
                "Block page clicks while annotating",
                flag("blockInteractions"),
                setting("blockInteractions", json!(!flag("blockInteractions"))),
            )
            .menu_with_check(
                "Find React components",
                flag("reactEnabled"),
                setting("reactEnabled", json!(!flag("reactEnabled"))),
            )
            .separator()
            .submenu_menu("Mark colour", colours)
    }

    fn show_native_docs_annotate_settings_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let menu = self.native_docs_annotate_settings_menu();
        let trigger = SETTINGS_MENU_ANCHOR.with(|cell| cell.get());
        self.native_docs_show_menu(menu, trigger, true, window, cx);
    }

    /// The ⋯ menu at side-panel width: Layout, Settings, Open in Browser and Open With.
    fn show_native_docs_annotate_more_menu(
        &mut self,
        path: &str,
        layout: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let settings = self.native_docs_annotate_settings_menu();
        let mut menu = GpuiContextMenu::new()
            .menu_with_check(
                "Layout mode",
                layout,
                menu_action("toggleLayout", Value::Null),
            )
            .submenu_menu("Settings", settings)
            .separator();
        if super::open_externally::file_opens_externally(path) {
            menu = menu
                .menu("Open in Browser", menu_action("openInBrowser", json!(path)))
                .submenu_menu("Open With", self.native_docs_open_with_menu(path));
        }
        let trigger = MORE_MENU_ANCHOR.with(|cell| cell.get());
        self.native_docs_show_menu(menu, trigger, true, window, cx);
    }

    /// The header's Annotate buttons for an HTML file: Annotate, then while it is on the page's
    /// toolbar buttons (Pause, Layout, Hide marks, Copy, Send, Clear, Settings), each once.
    pub(crate) fn render_native_docs_annotate_tiles(
        &mut self,
        path: &str,
        p: &DocsPalette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        self.native_docs_annotate_ensure_loaded(path);
        let Some(document) = self.native_docs.document(path) else {
            return Vec::new();
        };
        let on = document.html_annotate;
        let state = document.html_annotate_state.clone();
        let compact = super::render::view_width() < ANNOTATE_COMPACT_WIDTH;
        let mut tiles = Vec::new();

        let toggle_path = path.to_string();
        let annotate_tooltip = titlebar_tooltip_label(
            if on { "Turn off Annotate" } else { "Annotate" },
            ANNOTATE_HOTKEY_ID,
        );
        let annotate = header_tile(
            "native-docs-html-annotate",
            div()
                .flex()
                .items_center()
                .gap(px(5.0))
                .child(header_icon("files-view/t-message-plus-2.svg", false, p))
                .when(!compact, |this| {
                    this.child(
                        div()
                            .text_size(px(12.5))
                            .text_color(if on { p.text } else { p.toolbar_icon })
                            .child("Annotate"),
                    )
                }),
            on,
            false,
            p,
        )
        .when(!compact, |this| this.w_auto().px(px(8.0)))
        .tooltip(move |window, cx| titlebar_tooltip(annotate_tooltip.clone(), window, cx))
        .on_click(cx.listener(move |this, _, _, cx| {
            this.native_docs_toggle_annotate(&toggle_path, cx);
        }));
        tiles.push(annotate.into_any_element());
        if !on {
            return tiles;
        }

        let command_tile = |id: &'static str,
                            icon: &'static str,
                            active: bool,
                            disabled: bool,
                            tooltip: &'static str,
                            command: &'static str,
                            value: Value,
                            cx: &mut Context<Self>| {
            header_tile(id, header_icon(icon, disabled, p), active, disabled, p)
                .tooltip(move |window, cx| titlebar_tooltip(tooltip, window, cx))
                .when(!disabled, |this| {
                    this.on_click(cx.listener(move |this, _, _, cx| {
                        this.native_docs_annotate_command(command, value.clone(), cx);
                    }))
                })
                .into_any_element()
        };
        tiles.push(
            div()
                .flex_none()
                .w(px(1.0))
                .h(px(16.0))
                .mx(px(4.0))
                .bg(p.border)
                .into_any_element(),
        );
        tiles.push(command_tile(
            "native-docs-annotate-pause",
            if state.frozen {
                "titlebar/player-play.svg"
            } else {
                "titlebar/player-pause.svg"
            },
            state.frozen,
            false,
            if state.frozen {
                "Resume animations (P)"
            } else {
                "Pause animations (P)"
            },
            "toggleFreeze",
            Value::Null,
            cx,
        ));
        if !compact {
            tiles.push(command_tile(
                "native-docs-annotate-layout",
                "titlebar/layout-dashboard.svg",
                state.layout,
                false,
                if state.layout {
                    "Leave layout mode (L)"
                } else {
                    "Layout mode (L)"
                },
                "toggleLayout",
                Value::Null,
                cx,
            ));
        }
        let marks_disabled = state.notes == 0;
        tiles.push(command_tile(
            "native-docs-annotate-marks",
            if state.marks_visible || marks_disabled {
                "titlebar/eye.svg"
            } else {
                "titlebar/eye-off.svg"
            },
            false,
            marks_disabled,
            if state.marks_visible {
                "Hide marks (H)"
            } else {
                "Show marks (H)"
            },
            "setMarksVisible",
            Value::Bool(!state.marks_visible),
            cx,
        ));
        tiles.push(command_tile(
            "native-docs-annotate-copy",
            "titlebar/copy.svg",
            false,
            !state.can_copy,
            "Copy notes (C)",
            "copy",
            Value::Null,
            cx,
        ));
        let send_disabled = !state.can_send;
        let notes = state.notes;
        tiles.push(
            header_tile(
                "native-docs-annotate-send",
                div()
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .child(header_icon("titlebar/send.svg", send_disabled, p))
                    .when(!compact, |this| {
                        this.child(
                            div()
                                .text_size(px(12.5))
                                .text_color(if send_disabled {
                                    p.toolbar_disabled
                                } else {
                                    p.text
                                })
                                .child(if state.sending { "Sending" } else { "Send" }),
                        )
                    })
                    .when(notes > 0, |this| {
                        this.child(
                            div()
                                .text_size(px(12.5))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(p.text)
                                .child(notes.to_string()),
                        )
                    }),
                notes > 0,
                send_disabled,
                p,
            )
            .when(!compact || notes > 0, |this| this.w_auto().px(px(7.0)))
            .tooltip(|window, cx| titlebar_tooltip("Send notes to the agent (S)", window, cx))
            .when(!send_disabled, |this| {
                this.on_click(cx.listener(|this, _, _, cx| {
                    this.native_docs_annotate_command("send", Value::Null, cx);
                }))
            })
            .into_any_element(),
        );
        tiles.push(command_tile(
            "native-docs-annotate-clear",
            "titlebar/trash.svg",
            false,
            !state.can_clear,
            "Clear notes (X)",
            "clearAll",
            Value::Null,
            cx,
        ));
        let anchor = |cell: &'static std::thread::LocalKey<Cell<Bounds<Pixels>>>| {
            gpui::canvas(
                move |bounds, _, _| cell.with(|anchor| anchor.set(bounds)),
                |_, _, _, _| {},
            )
            .absolute()
            .size_full()
        };
        if compact {
            let menu_path = path.to_string();
            let layout = state.layout;
            tiles.push(
                header_tile(
                    "native-docs-annotate-more",
                    header_icon("titlebar/dots.svg", false, p),
                    false,
                    false,
                    p,
                )
                .child(anchor(&MORE_MENU_ANCHOR))
                .tooltip(|window, cx| titlebar_tooltip("More", window, cx))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.show_native_docs_annotate_more_menu(&menu_path, layout, window, cx);
                }))
                .into_any_element(),
            );
        } else {
            tiles.push(
                header_tile(
                    "native-docs-annotate-settings",
                    header_icon("titlebar/settings.svg", false, p),
                    false,
                    false,
                    p,
                )
                .child(anchor(&SETTINGS_MENU_ANCHOR))
                .tooltip(|window, cx| titlebar_tooltip("Annotate settings", window, cx))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.show_native_docs_annotate_settings_menu(window, cx);
                }))
                .into_any_element(),
            );
        }
        tiles
    }
}

use gpui::prelude::FluentBuilder as _;
