//! The bar on top of a new chat's message box: project › computer › where › detail, plus the draft's
//! agent, with each choice's shortcut shown in its hover tooltip.
//! The draft's "No project" and "Coordinator" toggle pills live in the composer's own top strip
//! (`pills_strip.rs`). gx-chat-core decides the worktree/branch parts
//! (`packages/gx-chat-core/src/menus/draft_worktree.rs`); this file measures the parts, draws them
//! and opens their menus. The phone draws the worktree bar in
//! `apps/mobile/app/src/chat/native/composer/WorktreeTray.tsx`.
//!
//! The project and computer parts, and the base branch and worktree lists, open the search card
//! in `draft_project_picker.rs`; the Cloud Box boot card is
//! `box_picker.rs`; the hotkeys that open each part are `tray_hotkeys.rs`.
//!
//! CDXC:Worktrees 2026-10-31 DECISION:
//! User (Q828): the tray sits on top of the message box, attached to it like a tab, while the chat is
//! new.
//!
//! CDXC:Worktrees 2026-11-04 DECISION:
//! User (Q1227 "D. One breadcrumb", "dont have a border around the breadcrumb"): the chips and the
//! 4-way switch are gone; the bar reads as one path whose parts open their own menus, with no border
//! of their own (a hover wash shows what can be clicked). A part cut short shows its full words in
//! its tooltip, with its hotkey (Q1228).
use super::tray_layout::{NARROW_WIDTH, PartSize, shrink};
use super::{
    appearance::ChatAppearance, composer::hotkey_chord, option_pills::agent_accent,
    state::NativeChatView,
};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Bounds, Context, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    StatefulInteractiveElement as _, Styled as _, Window, div, px, rgb, svg,
};
use gpui_component::ElementExt as _;
use serde_json::{Value, json};
use std::cell::Cell;
use std::rc::Rc;

const WHERE_TRIGGER: &str = "chat-worktree-where";
const DETAIL_TRIGGER: &str = "chat-worktree-detail";
const CLOUD_TRIGGER: &str = "chat-cloud-box-picker";
const AGENT_TRIGGER: &str = "chat-draft-agent";
const FAILED_COLOR: u32 = 0xef4444;
const STAR_COLOR: u32 = 0xf5b301;
const MENU_WIDTH: f32 = 280.0;
const AGENT_MENU_WIDTH: f32 = 220.0;
/// The message box's corner radius (`composer.rs`), which the tray is inset by.
const COMPOSER_RADIUS: f32 = 22.0;
/// The gap the composer's footer leaves between the tray and the box (`composer.rs`).
const FOOTER_GAP: f32 = 8.0;
/// The tray's own padding at its sides.
const TRAY_PADDING: f32 = 8.0;
/// A part: its height, the hover wash's inset at each side, its icon and the gap after it.
const PART_HEIGHT: f32 = 24.0;
const PART_PADDING: f32 = 6.0;
const ICON: f32 = 13.0;
const GAP: f32 = 5.0;
const FONT: f32 = 13.0;
/// The › between place/where parts, with its margins.
const SEPARATOR: f32 = 14.0;
/// The vertical hairline between the place/where group and the agent on one row.
const GROUP_DIVIDER: f32 = 13.0;
/// The least a cut part keeps of its words.
const MIN_WORDS: f32 = 36.0;

/// What a part does when it is clicked or its hotkey is pressed.
#[derive(Clone)]
enum PartAction {
    /// Only shown (a computer with nowhere else to go, an unswitchable agent).
    None,
    /// The project search, or the computers that have the project (`draft_project_picker.rs`).
    Place { machines: bool },
    /// The base branch or worktree list in the search card (`draft_project_picker.rs`).
    List {
        kind: super::draft_project_picker::DraftListKind,
        rows: Vec<Value>,
    },
    /// The chat menu with these rows, from the part.
    Menu {
        trigger: &'static str,
        rows: Vec<Value>,
        width: f32,
    },
}

#[derive(Clone)]
struct Part {
    id: &'static str,
    glyph: Option<String>,
    agent_icon: Option<String>,
    star: bool,
    prefix: Option<String>,
    label: String,
    muted: bool,
    tooltip: String,
    hotkey: Option<&'static str>,
    action: PartAction,
    bounds: Rc<Cell<Bounds<Pixels>>>,
}

/// The chat menu's rows with the core's `glyph` turned into the desktop's icon path.
fn menu_rows(rows: &Value) -> Vec<Value> {
    rows.as_array()
        .into_iter()
        .flatten()
        .map(|row| {
            let mut row = row.clone();
            if let Some(glyph) = row["glyph"].as_str().map(str::to_owned) {
                row["iconPath"] = json!(format!("titlebar/{glyph}.svg"));
            }
            row
        })
        .collect()
}

/// Fallback human label when `availableAgents` has no matching name yet.
fn fallback_agent_label(raw: &str) -> String {
    match raw {
        "claude" => "Claude".to_string(),
        "codex" => "Codex".to_string(),
        "antigravity" | "antigravity-cli" => "Antigravity".to_string(),
        "opencode" => "OpenCode".to_string(),
        "cursor" => "Cursor".to_string(),
        "gemini" => "Gemini".to_string(),
        "amp" => "Amp".to_string(),
        "pi" => "Pi".to_string(),
        "hermes" => "Hermes".to_string(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                None => String::new(),
            }
        }
    }
}

impl NativeChatView {
    /// Whether this chat is an unsent draft whose setup bar should be visible even before
    /// `/api/draftWorktree` answers (e.g. No project or a slow remote computer).
    fn draft_setup_bar_fallback_active(&self) -> bool {
        self.snapshot["availableAgents"].is_array()
            && !self.snapshot["firstMessageCard"].is_object()
            && !self.pending_send
            && self.snapshot["working"] != true
            && self.items.is_empty()
            && self.snapshot["queue"]["prompts"]
                .as_array()
                .is_none_or(Vec::is_empty)
    }

    /// The `worktreeTray` snapshot object when present, or an empty object for the fallback draft
    /// setup bar while it is still an unsent draft. When the user picked a pending target project
    /// before Send, overlays its prewarmed `/api/draftWorktree` options (or hides stale source
    /// project branches while loading) without moving the session.
    fn effective_draft_tray(&self) -> Option<Value> {
        let base = if let Some(tray) = self
            .snapshot
            .get("worktreeTray")
            .filter(|tray| tray.is_object())
        {
            Some(tray.clone())
        } else {
            self.draft_setup_bar_fallback_active().then(|| json!({}))
        }?;
        let Some(target) = self.pending_draft_target.as_ref() else {
            return Some(base);
        };
        if target.is_main_hub {
            return Some(json!({
                "projectName": target.label,
                "busy": false,
                "status": Value::Null,
            }));
        }
        if let Some(pending) = self
            .pending_target_tray
            .as_ref()
            .filter(|pending| pending.project_id == target.project_id)
            && let Some(preview) = ghostex_gx_chat_core::menus::draft_worktree::preview_tray(
                &pending.answer,
                pending.mode.as_deref(),
                pending.base_branch.as_deref(),
                pending.worktree_key.as_deref(),
            )
        {
            return Some(preview);
        }
        // Target project's tray is still loading (or on another computer): show its project/computer
        // immediately without showing the old project's branches/worktrees.
        Some(json!({
            "projectName": target.label,
            "busy": false,
            "status": Value::Null,
        }))
    }

    /// Whether the bar is on screen (the hotkeys act only then).
    pub(super) fn worktree_tray_shown(&self) -> bool {
        self.effective_draft_tray().is_some()
    }

    /// Whether the draft's "No project" and "Coordinator" toggle pills should show in the
    /// composer's pill strip (`pills_strip.rs`): while this draft is unsent and idle.
    pub(super) fn draft_toggle_pills_shown(&self) -> bool {
        self.effective_draft_tray()
            .is_some_and(|tray| tray["busy"] != true && tray["status"].is_null())
    }

    /// The Agent segment for a draft setup bar (`Option+A` / click opens the agent list).
    fn draft_agent_part(&self) -> Option<Part> {
        let current_id = self.snapshot["sessionAgentId"]
            .as_str()
            .map(str::trim)
            .filter(|id| !id.is_empty());
        let agents = self.snapshot["availableAgents"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let current_entry = current_id.and_then(|id| {
            agents
                .iter()
                .find(|row| row["agentId"].as_str() == Some(id))
        });
        let label = current_entry
            .and_then(|row| row["name"].as_str())
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .or_else(|| {
                self.snapshot["optionLabels"]["agentChip"]["name"]
                    .as_str()
                    .filter(|name| !name.is_empty())
                    .map(str::to_owned)
            })
            .or_else(|| {
                self.snapshot["optionLabels"]["agentIcon"]
                    .as_str()
                    .or(current_id)
                    .map(fallback_agent_label)
                    .filter(|name| !name.is_empty())
            })?;
        let icon = current_entry
            .and_then(|row| row["icon"].as_str().or_else(|| row["baseAgentId"].as_str()))
            .or_else(|| self.snapshot["optionLabels"]["agentIcon"].as_str())
            .or(current_id)
            .filter(|icon| !icon.is_empty())
            .map(str::to_owned);
        let rows: Vec<Value> = agents
            .iter()
            .filter_map(|row| {
                let agent_id = row["agentId"].as_str()?.trim();
                let name = row["name"].as_str()?.trim();
                if agent_id.is_empty() || name.is_empty() {
                    return None;
                }
                let mut item = json!({
                    "label": name,
                    "checked": current_id == Some(agent_id),
                    "command": { "type": "switchDraftAgent", "agentId": agent_id },
                });
                if let Some(row_icon) = row["icon"]
                    .as_str()
                    .or_else(|| row["baseAgentId"].as_str())
                    .filter(|icon| !icon.is_empty())
                {
                    item["icon"] = json!(row_icon);
                }
                Some(item)
            })
            .collect();
        let pickable = !rows.is_empty();
        Some(Part {
            id: AGENT_TRIGGER,
            glyph: icon.is_none().then(|| "sparkles".to_string()),
            agent_icon: icon,
            star: false,
            prefix: None,
            tooltip: format!("Agent: {label}"),
            label,
            muted: false,
            hotkey: pickable.then_some("newChatPickAgent"),
            action: if pickable {
                PartAction::Menu {
                    trigger: AGENT_TRIGGER,
                    rows,
                    width: AGENT_MENU_WIDTH,
                }
            } else {
                PartAction::None
            },
            bounds: self.worktree_tray_agent_bounds.clone(),
        })
    }

    /// Toggles Coordinator mode on this draft, showing the coordinator first-message card and a
    /// toast prompting the user to pick model/effort and click Send.
    pub(super) fn toggle_draft_coordinator(&mut self, cx: &mut Context<Self>) {
        self.draft_coordinator_mode = !self.draft_coordinator_mode;
        self.draft_setup_changed = true;
        if self.draft_coordinator_mode {
            if let Some(app) = self.config.app.as_ref().and_then(|app| app.upgrade()) {
                cx.defer(move |cx| {
                    app.update(cx, |app, cx| {
                        app.gx_store_create_toast(
                            "info",
                            "Coordinator",
                            Some(
                                "Pick a model and effort, then click Send to start this coordinator.",
                            ),
                            cx,
                        );
                    });
                });
            }
        }
        cx.notify();
    }

    /// The setup bar's parts in two groups:
    /// - Row 0: place and where (`project`, `computer`, `where`, `branch/detail`)
    /// - Row 1: the agent
    ///
    /// CDXC:Worktrees 2026-11-25 DECISION:
    /// User: "no need to be able to pick the model again here wtf. it's already there bottom left of
    /// the chat box". The bar carries no model or effort part; Option+P opens the model pop-up from
    /// the chat box's own model pill (`option_pills.rs`).
    fn tray_parts(&self, tray: &Value, cx: &Context<Self>) -> [Vec<Part>; 2] {
        let mut place = Vec::new();
        let no_project = self.draft_is_no_project(cx);
        let idle = tray["busy"] != true && tray["status"].is_null();
        if no_project {
            // In "No project" mode, hide Project, Where and Branch/Worktree; keep only the machine picker.
            if let Some((label, local, pickable)) = self.draft_machine_part(cx) {
                place.push(Part {
                    id: "chat-draft-machine",
                    glyph: Some(if local { "device-desktop" } else { "server" }.into()),
                    agent_icon: None,
                    star: false,
                    prefix: None,
                    tooltip: format!("Computer: {label}"),
                    label,
                    muted: !pickable,
                    hotkey: pickable.then_some("newChatPickComputer"),
                    action: if pickable {
                        PartAction::Place { machines: true }
                    } else {
                        PartAction::None
                    },
                    bounds: self.draft_machine_chip_bounds.clone(),
                });
            }
        } else {
            let project_name = tray["projectName"]
                .as_str()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
                .or_else(|| self.draft_choice(cx).map(|choice| choice.label));
            if let Some(name) = project_name {
                place.push(Part {
                    id: "chat-draft-project",
                    glyph: Some("folder".into()),
                    agent_icon: None,
                    star: false,
                    prefix: None,
                    tooltip: format!("Project: {name}"),
                    label: name,
                    muted: false,
                    hotkey: Some("newChatPickProject"),
                    action: PartAction::Place { machines: false },
                    bounds: self.draft_project_chip_bounds.clone(),
                });
                if let Some((label, local, pickable)) = self.draft_machine_part(cx) {
                    place.push(Part {
                        id: "chat-draft-machine",
                        glyph: Some(if local { "device-desktop" } else { "server" }.into()),
                        agent_icon: None,
                        star: false,
                        prefix: None,
                        tooltip: format!("Computer: {label}"),
                        label,
                        muted: !pickable,
                        hotkey: pickable.then_some("newChatPickComputer"),
                        action: if pickable {
                            PartAction::Place { machines: true }
                        } else {
                            PartAction::None
                        },
                        bounds: self.draft_machine_chip_bounds.clone(),
                    });
                }
            }
            let place_of = &tray["where"];
            if let Some(label) = place_of["label"].as_str() {
                place.push(Part {
                    id: WHERE_TRIGGER,
                    glyph: Some(place_of["glyph"].as_str().unwrap_or("folder-open").into()),
                    agent_icon: None,
                    star: false,
                    prefix: None,
                    label: label.to_owned(),
                    muted: false,
                    tooltip: format!("Where it runs: {label}"),
                    hotkey: Some("newChatPickWhere"),
                    action: PartAction::Menu {
                        trigger: WHERE_TRIGGER,
                        rows: menu_rows(&place_of["menu"]),
                        width: MENU_WIDTH,
                    },
                    bounds: self.worktree_tray_where_bounds.clone(),
                });
            }
            let detail = &tray["detail"];
            if let Some(label) = detail["label"].as_str() {
                let cloud = detail["kind"] == "cloudBox";
                let has_menu = detail["menu"].is_array();
                let list_kind = match detail["kind"].as_str() {
                    Some("baseBranch") => {
                        Some(super::draft_project_picker::DraftListKind::BaseBranch)
                    }
                    Some("worktree") => Some(super::draft_project_picker::DraftListKind::Worktree),
                    Some("folderBranch") => {
                        Some(super::draft_project_picker::DraftListKind::FolderBranch)
                    }
                    _ => None,
                };
                place.push(Part {
                    id: if cloud { CLOUD_TRIGGER } else { DETAIL_TRIGGER },
                    glyph: Some(detail["glyph"].as_str().unwrap_or("git-branch").into()),
                    agent_icon: None,
                    star: detail["star"] == true,
                    prefix: detail["prefix"].as_str().map(str::to_owned),
                    label: label.to_owned(),
                    muted: !has_menu || detail["placeholder"] == true,
                    tooltip: detail["tooltip"].as_str().unwrap_or(label).to_owned(),
                    hotkey: has_menu.then_some("newChatPickBranch"),
                    action: if let (true, Some(kind)) = (has_menu, list_kind) {
                        let mut rows = menu_rows(&detail["menu"]);
                        // The search's "Create <typed> from <branch>" (This folder's branch only).
                        if let Some(from) = detail["create"]["from"].as_str() {
                            rows.push(json!({
                                "createFrom": from,
                                "command": detail["create"]["command"].clone(),
                            }));
                        }
                        PartAction::List { kind, rows }
                    } else if has_menu {
                        PartAction::Menu {
                            trigger: if cloud { CLOUD_TRIGGER } else { DETAIL_TRIGGER },
                            rows: menu_rows(&detail["menu"]),
                            width: detail["menuWidth"]
                                .as_f64()
                                .map_or(MENU_WIDTH, |w| w as f32),
                        }
                    } else {
                        PartAction::None
                    },
                    bounds: self.worktree_tray_detail_bounds.clone(),
                });
            }
        }
        let mut agent_group = Vec::new();
        if idle {
            if let Some(agent) = self.draft_agent_part() {
                agent_group.push(agent);
            }
        }
        [place, agent_group]
    }

    /// Opens what a part opens, from a click or its hotkey.
    fn open_tray_part(
        &mut self,
        action: &PartAction,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            PartAction::None => {}
            PartAction::Place { machines } => self.toggle_draft_place_picker(*machines, window, cx),
            PartAction::List { kind, rows } => {
                self.toggle_draft_list_picker(*kind, rows.clone(), bounds, window, cx)
            }
            PartAction::Menu {
                trigger,
                rows,
                width,
            } => {
                if self.chat_menu_toggled_shut(trigger, cx) {
                    return;
                }
                // Menus open upward from their part with their left edges lined up, as a composer
                // menu does, instead of the side panel's place that put them below the bar over the
                // chat box (CDXC:Worktrees 2026-11-10 in draft_project_picker.rs).
                self.menu_side = Some(super::option_menu::MenuSide::Left);
                self.show_chat_menu(rows.clone(), bounds, *width, window, cx);
            }
        }
    }

    /// A bar hotkey (`tray_hotkeys.rs`): opens its part's menu. A part the bar does not have
    /// (no other computer, nothing to pick next) does nothing.
    pub(super) fn run_tray_hotkey(
        &mut self,
        action_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tray) = self.effective_draft_tray() else {
            return;
        };
        if tray["busy"] == true {
            return;
        }
        let parts = self.tray_parts(&tray, cx);
        if let Some(part) = parts
            .iter()
            .flatten()
            .find(|part| part.hotkey == Some(action_id))
            .cloned()
        {
            self.open_tray_part(&part.action, part.bounds.get(), window, cx);
        } else if action_id == "newChatPickProject" && self.draft_is_no_project(cx) {
            self.toggle_draft_place_picker(false, window, cx);
        }
    }

    /// A part's natural width and the least it may be cut to, at the chat's zoom.
    fn measure_tray_part(&self, part: &Part, p: &ChatAppearance, window: &Window) -> PartSize {
        let s = p.scale;
        let mut style = window.text_style();
        style.font_family = p.font.clone().into();
        let text_width = |text: &str| {
            window
                .text_system()
                .shape_line(
                    text.to_owned().into(),
                    px(FONT * s),
                    &[style.to_run(text.len())],
                    None,
                )
                .width
                .as_f32()
        };
        let has_icon = part.glyph.is_some() || part.agent_icon.is_some();
        let mut chrome = (PART_PADDING * 2.0) * s;
        if has_icon {
            chrome += (ICON + GAP) * s;
        }
        if let Some(prefix) = part.prefix.as_deref() {
            chrome += text_width(prefix) + GAP * s;
        }
        // A pixel of slack so a part measured whole is never drawn cut by rounding.
        let words = text_width(&part.label).ceil() + 1.0;
        PartSize {
            natural: chrome + words,
            min: chrome + words.min(MIN_WORDS * s),
        }
    }

    fn render_tray_part(
        &self,
        part: Part,
        width: f32,
        natural: f32,
        p: &ChatAppearance,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let clickable = !busy && !matches!(part.action, PartAction::None);
        let cut = width + 0.5 < natural;
        let bounds = part.bounds.clone();
        let tooltip = part.tooltip.clone();
        let hotkey = part.hotkey;
        let action = part.action.clone();
        let icon = if let Some(agent_icon) = part.agent_icon.as_deref() {
            Some(
                svg()
                    .path(format!("agent-icons/{agent_icon}.svg"))
                    .size(px(ICON * s))
                    .flex_shrink_0()
                    .text_color(rgb(agent_accent(agent_icon, p))),
            )
        } else {
            part.glyph.as_deref().map(|glyph| {
                svg()
                    .path(format!("titlebar/{glyph}.svg"))
                    .size(px(ICON * s))
                    .flex_shrink_0()
                    .text_color(if part.star {
                        rgb(STAR_COLOR).into()
                    } else {
                        p.muted
                    })
            })
        };
        div()
            .on_prepaint(move |painted, _, _| bounds.set(painted))
            .id(part.id)
            .role(gpui::Role::Button)
            .aria_label(part.tooltip.clone())
            .flex_shrink_0()
            .when(cut, |this| this.w(px(width)))
            .h(px(PART_HEIGHT * s))
            .px(px(PART_PADDING * s))
            .flex()
            .items_center()
            .gap(px(GAP * s))
            .rounded(px(6.0 * s))
            .text_size(px(FONT * s))
            .when(clickable, |this| {
                let trigger = part.bounds.clone();
                this.chat_cursor_pointer()
                    .hover(|style| style.bg(p.border))
                    .on_click(cx.listener(move |chat, _, window, cx| {
                        chat.open_tray_part(&action, trigger.get(), window, cx);
                    }))
            })
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(tooltip.clone())
                    .keys(hotkey_chord(hotkey))
                    .build(window, cx)
            })
            .children(icon)
            .when_some(part.prefix, |this, prefix| {
                this.child(div().flex_shrink_0().text_color(p.muted).child(prefix))
            })
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_color(if part.muted { p.muted } else { p.primary })
                    .child(part.label),
            )
            .into_any_element()
    }

    fn tray_separator(p: &ChatAppearance) -> AnyElement {
        div()
            .flex_shrink_0()
            .w(px(SEPARATOR * p.scale))
            .flex()
            .justify_center()
            .child(
                svg()
                    .path("titlebar/chevron-right.svg")
                    .size(px(12.0 * p.scale))
                    .text_color(p.muted),
            )
            .into_any_element()
    }

    fn tray_group_divider(p: &ChatAppearance) -> AnyElement {
        div()
            .flex_shrink_0()
            .w(px(GROUP_DIVIDER * p.scale))
            .h(px(12.0 * p.scale))
            .flex()
            .items_center()
            .justify_center()
            .child(div().w(px(1.0)).h_full().bg(p.border))
            .into_any_element()
    }

    /// Computes widths for one natural row (using `shrink` only when the row exceeds `available`).
    fn fit_row_widths(sizes: &[PartSize], gap_total: f32, available: f32) -> Vec<f32> {
        let natural: Vec<f32> = sizes.iter().map(|part| part.natural).collect();
        let total = natural.iter().sum::<f32>() + gap_total;
        if available > 0.0 && total > available {
            let mins: Vec<f32> = sizes.iter().map(|part| part.min).collect();
            shrink(&natural, &mins, (available - gap_total).max(0.0))
        } else {
            natural
        }
    }

    /// The setup bar breadcrumb: one row while every segment fits, else two compact rows
    /// (place/where on top, the agent underneath) so nothing clips or stretches awkwardly.
    fn render_tray_breadcrumb(
        &self,
        tray: &Value,
        p: &ChatAppearance,
        busy: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let s = p.scale;
        let [place_parts, agent_parts] = self.tray_parts(tray, cx);
        if place_parts.is_empty() && agent_parts.is_empty() {
            return None;
        }
        let place_sizes: Vec<PartSize> = place_parts
            .iter()
            .map(|part| self.measure_tray_part(part, p, window))
            .collect();
        let agent_sizes: Vec<PartSize> = agent_parts
            .iter()
            .map(|part| self.measure_tray_part(part, p, window))
            .collect();
        // The room inside the tray: the box's width less the tray's inset, padding and border.
        let composer = self.composer_bounds.get().size.width.as_f32();
        let available = if composer > 0.0 {
            composer - (COMPOSER_RADIUS + TRAY_PADDING) * 2.0 * s - 2.0
        } else {
            0.0
        };
        let place_sep_total = SEPARATOR * s * place_parts.len().saturating_sub(1) as f32;
        let divider_total = if !place_parts.is_empty() && !agent_parts.is_empty() {
            GROUP_DIVIDER * s
        } else {
            0.0
        };
        let one_row_natural = place_sizes.iter().map(|part| part.natural).sum::<f32>()
            + agent_sizes.iter().map(|part| part.natural).sum::<f32>()
            + place_sep_total
            + divider_total;
        let measured = available > 0.0;
        let two_rows = measured
            && !place_parts.is_empty()
            && !agent_parts.is_empty()
            && one_row_natural > available;
        let centered = measured && available < NARROW_WIDTH * s;

        let breadcrumb = if !two_rows {
            let mut all_sizes = Vec::with_capacity(place_sizes.len() + agent_sizes.len());
            all_sizes.extend_from_slice(&place_sizes);
            all_sizes.extend_from_slice(&agent_sizes);
            let widths =
                Self::fit_row_widths(&all_sizes, place_sep_total + divider_total, available);
            let split = place_parts.len();
            let mut cells = Vec::new();
            for (index, (part, width)) in place_parts.into_iter().zip(&widths[..split]).enumerate()
            {
                if index > 0 {
                    cells.push(Self::tray_separator(p));
                }
                cells.push(self.render_tray_part(
                    part,
                    *width,
                    place_sizes[index].natural,
                    p,
                    busy,
                    cx,
                ));
            }
            if split > 0 && !agent_parts.is_empty() {
                cells.push(Self::tray_group_divider(p));
            }
            for (index, (part, width)) in agent_parts.into_iter().zip(&widths[split..]).enumerate()
            {
                cells.push(self.render_tray_part(
                    part,
                    *width,
                    agent_sizes[index].natural,
                    p,
                    busy,
                    cx,
                ));
            }
            div()
                .w_full()
                .flex()
                .items_center()
                .when(centered, |this| this.justify_center())
                .children(cells)
        } else {
            let place_widths = Self::fit_row_widths(&place_sizes, place_sep_total, available);
            let agent_widths = Self::fit_row_widths(&agent_sizes, 0.0, available);
            let mut row_zero = Vec::new();
            for (index, (part, width)) in place_parts.into_iter().zip(place_widths).enumerate() {
                if index > 0 {
                    row_zero.push(Self::tray_separator(p));
                }
                row_zero.push(self.render_tray_part(
                    part,
                    width,
                    place_sizes[index].natural,
                    p,
                    busy,
                    cx,
                ));
            }
            let mut row_one = Vec::new();
            for (index, (part, width)) in agent_parts.into_iter().zip(agent_widths).enumerate() {
                row_one.push(self.render_tray_part(
                    part,
                    width,
                    agent_sizes[index].natural,
                    p,
                    busy,
                    cx,
                ));
            }
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(2.0 * s))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .when(centered, |this| this.justify_center())
                        .children(row_zero),
                )
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .when(centered, |this| this.justify_center())
                        .children(row_one),
                )
        };
        Some(
            breadcrumb
                .id("chat-worktree-breadcrumb")
                .role(gpui::Role::Toolbar)
                .aria_label("New chat setup")
                .when(busy, |this| this.opacity(0.6))
                .into_any_element(),
        )
    }

    pub(crate) fn render_worktree_tray(
        &self,
        p: &ChatAppearance,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let tray = self.effective_draft_tray()?;
        let s = p.scale;
        let busy = tray["busy"] == true;
        let breadcrumb = self.render_tray_breadcrumb(&tray, p, busy, window, cx);
        let status = &tray["status"];
        let status_line = match status["state"].as_str() {
            Some("creating") | Some("opened") => Some(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0 * s))
                    .min_w_0()
                    .px(px(PART_PADDING * s))
                    .text_size(px(12.5 * s))
                    .text_color(p.muted)
                    .child(
                        svg()
                            .path("titlebar/loader2.svg")
                            .size(px(13.0 * s))
                            .flex_shrink_0()
                            .text_color(p.muted),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(status["label"].as_str().unwrap_or_default().to_owned()),
                    )
                    .into_any_element(),
            ),
            Some("failed") => {
                let message = status["message"].as_str().unwrap_or_default().to_owned();
                // A line of its own, like the boot card, so the reason is never cut to a word.
                Some(
                    div()
                        .id("chat-worktree-failure")
                        .flex()
                        .w_full()
                        .items_center()
                        .gap(px(6.0 * s))
                        .min_w_0()
                        .px(px(PART_PADDING * s))
                        .text_size(px(12.5 * s))
                        .text_color(rgb(FAILED_COLOR))
                        .child(
                            svg()
                                .path("titlebar/alert-triangle.svg")
                                .size(px(13.0 * s))
                                .flex_shrink_0()
                                .text_color(rgb(FAILED_COLOR)),
                        )
                        .child(
                            div()
                                .id("chat-worktree-failure-message")
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .child(message.clone())
                                .tooltip(move |window, cx| {
                                    gpui_component::tooltip::Tooltip::new(message.clone())
                                        .build(window, cx)
                                }),
                        )
                        .child(
                            div()
                                .id("chat-worktree-retry")
                                .role(gpui::Role::Button)
                                .aria_label("Try again")
                                .flex_shrink_0()
                                .h(px(PART_HEIGHT * s))
                                .px(px(10.0 * s))
                                .flex()
                                .items_center()
                                .rounded(px(6.0 * s))
                                .text_color(p.primary)
                                .chat_cursor_pointer()
                                .hover(|style| style.bg(p.border))
                                .child("Try again")
                                .on_click(cx.listener(|chat, _, window, cx| {
                                    chat.submit("send", window, cx);
                                })),
                        )
                        .into_any_element(),
                )
            }
            _ => None,
        };
        let folder_prompt = self.render_folder_prompt(&tray["folderPrompt"], p, cx);
        if breadcrumb.is_none()
            && status_line.is_none()
            && folder_prompt.is_none()
            && !status["card"].is_object()
        {
            return None;
        }
        /*
        CDXC:Worktrees 2026-10-31 DECISION:
        User: "the bar above the composer must be transparent like it". The tray is part of the
        message box: the box's own fill and border (`composer_background`, which window glass turns
        into the bubble's wash), never the pane's solid background. It ends exactly on the box's top
        edge instead of reaching over its border, because two translucent fills stacked there read
        darker; the box's top border is the seam. It is inset by the box's corner radius so its sides
        meet the straight part of that edge, and the parts on it draw no fill of their own.
        */
        Some(
            div()
                .w_full()
                .relative()
                .px(px(COMPOSER_RADIUS * s))
                .mb(px(-FOOTER_GAP * s))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap(px(2.0 * s))
                        .px(px(TRAY_PADDING * s))
                        .pt(px(5.0 * s))
                        .pb(px(6.0 * s))
                        .rounded_t(px(12.0 * s))
                        .border_1()
                        .border_b_0()
                        .border_color(p.composer_border)
                        .bg(p.composer_background)
                        .children(breadcrumb)
                        .children(status_line)
                        .children(folder_prompt)
                        .when(status["card"].is_object(), |tray| {
                            tray.child(self.render_cloud_box_card(&status["card"], p))
                        }),
                )
                .into_any_element(),
        )
    }
}

impl NativeChatView {
    /// The line under the bar about This folder's branch switch (`menus/folder_branch.rs` in
    /// gx-chat-core): how it went, or why it waits, with the ways on as buttons. Its words wrap
    /// instead of being cut, so the reason always reads whole.
    fn render_folder_prompt(
        &self,
        prompt: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let message = prompt["message"].as_str()?.to_owned();
        let s = p.scale;
        let tone = prompt["tone"].as_str().unwrap_or_default();
        let (icon, color) = match tone {
            "busy" => ("titlebar/loader2.svg", p.muted),
            "done" => ("titlebar/check.svg", p.muted),
            "error" => ("titlebar/alert-triangle.svg", rgb(FAILED_COLOR).into()),
            _ => ("titlebar/alert-triangle.svg", p.muted),
        };
        let actions: Vec<AnyElement> = prompt["actions"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .filter_map(|(index, action)| {
                let label = action["label"].as_str()?.to_owned();
                let command = action["command"].clone();
                Some(
                    div()
                        .id(("chat-folder-branch-action", index))
                        .role(gpui::Role::Button)
                        .aria_label(label.clone())
                        .flex_shrink_0()
                        .h(px(PART_HEIGHT * s))
                        .px(px(8.0 * s))
                        .flex()
                        .items_center()
                        .rounded(px(6.0 * s))
                        .text_color(if index == 0 { p.primary } else { p.foreground })
                        .chat_cursor_pointer()
                        .hover(|style| style.bg(p.border))
                        .child(label)
                        .on_click(cx.listener(move |chat, _, window, cx| {
                            chat.handle_action(
                                &super::actions::NativeChatAction {
                                    command: command.clone(),
                                },
                                window,
                                cx,
                            );
                        }))
                        .into_any_element(),
                )
            })
            .collect();
        Some(
            div()
                .id("chat-folder-branch-prompt")
                .w_full()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(4.0 * s))
                .px(px(PART_PADDING * s))
                .text_size(px(12.5 * s))
                .child(
                    div()
                        .flex()
                        .items_start()
                        .gap(px(6.0 * s))
                        .min_w_0()
                        .flex_1()
                        .py(px(4.0 * s))
                        .text_color(if tone == "error" { color } else { p.muted })
                        .child(
                            svg()
                                .path(icon)
                                .size(px(13.0 * s))
                                .mt(px(1.0 * s))
                                .flex_shrink_0()
                                .text_color(color),
                        )
                        .child(div().min_w_0().child(message)),
                )
                .children(actions)
                .into_any_element(),
        )
    }
}
