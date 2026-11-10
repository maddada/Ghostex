//! Main Hub's GBots section: the GBots heading under Chief of Staff, then one row per GBot (its
//! icon in its colour, its name, a status dot while a chat works or waits) whose chats fold under
//! it like a project's sessions. A bot row's menu has New Chat, GBot Settings…, Memory…,
//! Department… and Remove GBot…; the heading's has New GBot…, Import Team… and Export Team…. Department sub-headings are
//! gbots_departments.rs.
//!
//! CDXC:GBots 2026-11-09 DECISION:
//! User (Q2101 A, Q2102 A, Q2104 A): GBots sit under Chief of Staff in Main Hub, between Chief of Staff and Main Hub's loose sessions; a bot is marked by "an icon and a colour"; its settings open "in their own window from its row" (right-click, GBot Settings…). A bot's chats are ordinary session rows (their own menus, status and click), folding under the bot's row the way a project's sessions fold under its header.
//! SEE-ALSO: packages/gx-core/src/sidebar_view/gbots.rs (which groups, in what order), server/src/gbots/ (`/api/gbots`), apps/desktop/src/app/gbot_settings_modal_lifecycle.rs (the settings window), apps/mobile/app/src/components/sessions/GbotsRows.tsx (the phone's same rows).

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, FontWeight, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, div, px, svg,
};
use gpui_component::{ElementExt as _, h_flex, v_flex};
use serde_json::{Value, json};

use super::appearance::SidebarAppearance;
use super::model::{NativeSidebarGroup, NativeSidebarSnapshot};
use crate::GhostexGpuiApp;
use crate::app::{consts::*, helpers::*};

/// A GBot's look as its project row carries it (gx-core `ProjectContextView::gbot`).
pub(crate) struct GbotLook {
    pub(crate) bot_id: String,
    pub(crate) name: String,
    pub(crate) title: String,
    pub(crate) icon: String,
    pub(crate) color: gpui::Hsla,
    pub(crate) paused: bool,
    /// It leads its department (a small crown after its name).
    pub(crate) is_lead: bool,
    /// Where its chats run when not here: `computer` or `cloudBox`, and its name (a chip after
    /// its name, gbots/computer/).
    pub(crate) computer: Option<(String, String)>,
    /// It may use that computer's screen and apps.
    pub(crate) computer_use: bool,
}

impl NativeSidebarGroup {
    /// The GBot this group is the project of; `None` for every other group.
    pub(crate) fn gbot(&self) -> Option<GbotLook> {
        let gbot = self.project_context.as_ref()?.get("gbot")?;
        let text = |key: &str| gbot.get(key).and_then(Value::as_str).unwrap_or_default();
        let color = gpui::Rgba::try_from(text("colorHex"))
            .map(gpui::Hsla::from)
            .unwrap_or_else(|_| gpui::rgb(0x9fb0ff).into());
        let icon = text("icon");
        Some(GbotLook {
            bot_id: text("id").to_string(),
            name: text("name").to_string(),
            title: text("title").to_string(),
            icon: if icon.is_empty() { "robot" } else { icon }.to_string(),
            color,
            paused: gbot.get("paused").and_then(Value::as_bool) == Some(true),
            is_lead: gbot.get("isLead").and_then(Value::as_bool) == Some(true),
            computer: gbot
                .get("computer")
                .map(|computer| {
                    let field = |key: &str| {
                        computer
                            .get(key)
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string()
                    };
                    (field("kind"), field("name"))
                })
                .filter(|(kind, name)| kind != "this" && !name.is_empty()),
            computer_use: gbot
                .pointer("/computer/computerUse")
                .and_then(Value::as_bool)
                == Some(true),
        })
    }

    /// The machine tab this group is on (`local` for this computer).
    pub(super) fn machine_id(&self) -> String {
        self.remote_machine_context
            .as_ref()
            .and_then(|context| context.get("machineId"))
            .and_then(Value::as_str)
            .unwrap_or(ghostex_gx_core::LOCAL_MACHINE_ID)
            .to_string()
    }
}

/// The small "Chief" tag after the title of a GBot chat Chief of Staff opened, in the muted text
/// a Cloud Box row names its box with.
pub(crate) fn opened_by_chief_tag(
    session: &super::model::NativeSidebarSession,
    appearance: &SidebarAppearance,
) -> Option<AnyElement> {
    if session.details.get("openedByChief") != Some(&Value::Bool(true)) {
        return None;
    }
    let scale = appearance.scale;
    Some(
        div()
            .flex_shrink_0()
            .px(px(4.0 * scale))
            .rounded(px(4.0 * scale))
            .border_1()
            .border_color(appearance.muted.opacity(0.35))
            .text_size(px(10.0 * scale))
            .text_color(appearance.muted)
            .child("Chief")
            .into_any_element(),
    )
}

/// Whether GBots are on in this computer's settings: Main Hub and the GBots switch both on
/// (server/src/gbots/feature.rs reads the same two).
pub(crate) fn gbots_on() -> bool {
    use ghostex_settings_catalog::built_in_extensions::{GBOTS, on_with};
    let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
    let settings = settings.object();
    super::chief::chief_on() && on_with(GBOTS, |key| settings.get(key).and_then(Value::as_bool))
}

/// What a folded GBots heading shows of the bots under it.
#[derive(Default)]
pub(super) struct FoldedSummary {
    pub(super) bots: usize,
    pub(super) working: bool,
    pub(super) attention: bool,
    pub(super) background_work: bool,
    pub(super) question: bool,
    pub(super) approval: bool,
}

impl FoldedSummary {
    pub(super) fn any(&self) -> bool {
        self.working || self.attention || self.background_work || self.question || self.approval
    }
}

pub(super) fn folded_summary(
    snapshot: &NativeSidebarSnapshot,
    group_ids: &[String],
) -> FoldedSummary {
    let mut summary = FoldedSummary {
        bots: group_ids.len(),
        ..FoldedSummary::default()
    };
    for group in snapshot
        .groups
        .iter()
        .filter(|group| group_ids.contains(&group.group_id))
    {
        let count = |key: &str| group.summary[key].as_u64().unwrap_or_default() > 0;
        summary.working |= count("workingCount");
        summary.attention |= count("attentionCount");
        summary.background_work |= count("backgroundWorkCount");
        summary.question |= count("questionCount");
        summary.approval |= count("approvalCount");
    }
    summary
}

pub(super) fn gbots_command(op: &str, extra: Value) -> Value {
    let mut command = json!({ "type": "gbots", "op": op });
    if let (Some(command), Some(extra)) = (command.as_object_mut(), extra.as_object()) {
        command.extend(extra.clone());
    }
    command
}

/// A bot row's menu (the sessions sidebar's menu builder draws it).
fn gbot_menu(look: &GbotLook, machine_id: &str) -> Value {
    let target = json!({ "botId": look.bot_id, "name": look.name, "machineId": machine_id });
    let mut items = vec![
        json!({ "label": "New Chat", "icon": "plus", "disabled": look.paused, "command": gbots_command("newChat", target.clone()) }),
    ];
    items.extend(call_item(look, &target));
    items.extend([
        json!({ "label": "GBot Settings…", "icon": "settings", "command": gbots_command("settings", target.clone()) }),
        json!({ "label": "Memory…", "icon": "brain", "command": gbots_command("memory", target.clone()) }),
        json!({ "label": "Department…", "icon": "users", "command": gbots_command("department", target.clone()) }),
        json!({ "label": "Computer…", "icon": "device-laptop", "command": gbots_command("computer", target.clone()) }),
        json!({ "separator": true }),
        json!({ "label": "Remove GBot…", "icon": "trash", "command": gbots_command("remove", target) }),
    ]);
    Value::Array(items)
}

/// Call (a call with the GBot through the Voice agent, voice_agent/gbot_call.rs): shown only while
/// the Voice agent is on (GBots are, or this row would not be drawn), so never on the web build,
/// which has no voice panel; greyed for a GBot whose chats run on another computer, which a call
/// can't reach.
fn call_item(look: &GbotLook, target: &Value) -> Option<Value> {
    #[cfg(not(target_arch = "wasm32"))]
    let on = crate::app::voice_agent::gbot_call::gbot_calls_on();
    #[cfg(target_arch = "wasm32")]
    let on = false;
    on.then(|| {
        json!({ "label": "Call", "icon": "headset", "disabled": look.computer.is_some(), "command": gbots_command("call", target.clone()) })
    })
}

/// The GBots heading's menu.
fn heading_menu() -> Value {
    json!([
        { "label": "New GBot…", "icon": "plus", "command": gbots_command("newGbot", json!({})) },
        { "separator": true },
        { "label": "Import Team…", "icon": "download", "command": gbots_command("importTeam", json!({})) },
        { "label": "Export Team…", "icon": "upload", "command": gbots_command("exportTeam", json!({})) },
    ])
}

impl GhostexGpuiApp {
    /// The GBots section drawn inside `home` (Main Hub's group), or `None` when that group has none.
    pub(crate) fn render_native_gbots_section(
        &self,
        home: &NativeSidebarGroup,
        snapshot: &NativeSidebarSnapshot,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let section = snapshot
            .gbots
            .iter()
            .find(|section| section.home_group_id == home.group_id)?;
        let key = format!("group:{}", section.storage_id);
        let bots = self
            .native_sidebar
            .disclosures
            .present(&key, section.collapsed)
            .then(|| {
                // The GBots of no department, then each department's sub-heading and bots
                // (gbots_departments.rs).
                let rows = v_flex()
                    .w_full()
                    .children(self.render_native_gbot_blocks(
                        &section.loose_group_ids,
                        snapshot,
                        appearance,
                        cx,
                    ))
                    .children(section.departments.iter().map(|department| {
                        self.render_native_gbots_department(department, snapshot, appearance, cx)
                    }))
                    .into_any_element();
                self.render_native_disclosure(key, rows, cx)
            });
        Some(
            v_flex()
                .w_full()
                .flex_shrink_0()
                .child(self.render_native_gbots_heading(
                    &section.storage_id,
                    section.collapsed,
                    folded_summary(snapshot, &section.group_ids),
                    appearance,
                    cx,
                ))
                .children(bots)
                .into_any_element(),
        )
    }

    /// "GBOTS", in the look of a section heading: a click folds every bot, the "+" and the
    /// right-click menu add one.
    fn render_native_gbots_heading(
        &self,
        storage_id: &str,
        collapsed: bool,
        folded: FoldedSummary,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let hover_key = format!("gbots-heading-{storage_id}");
        let hovered = self.native_sidebar.hovered_section.as_ref() == Some(&hover_key);
        let fold_id = storage_id.to_string();
        let stop_key = format!("gbots-heading:{storage_id}");
        let owner = super::menus::MenuOwner::default();
        let menu_owner = owner.clone();
        h_flex()
            .id(format!("native-sidebar-gbots-heading-{storage_id}"))
            .on_prepaint(super::menus::record_menu_owner(&owner))
            .role(gpui::Role::Button)
            .aria_label("GBots")
            .aria_expanded(!collapsed)
            .mx(px(3.0 * scale))
            .mt(px(4.0 * scale))
            .h(px(20.0 * scale))
            .py(px(3.0 * scale))
            .pl(px(5.0 * scale))
            .pr(px(4.0 * scale))
            .gap(px(5.0 * scale))
            .text_size(px(12.0 * scale))
            .font_weight(FontWeight::LIGHT)
            .text_color(chrome_color(0xd8d8d8, 0x292929).opacity(0.34))
            .rounded(px(5.0 * scale))
            .hover(|row| {
                row.bg(chrome_ink().opacity(0.06))
                    .text_color(chrome_color(0xd8d8d8, 0x292929).opacity(0.58))
            })
            .child("GBOTS")
            .when(hovered, |row| {
                row.child(titlebar_svg_icon(
                    if collapsed {
                        COMMAND_ICON_CHEVRON_RIGHT
                    } else {
                        COMMAND_ICON_CHEVRON_DOWN
                    },
                    12.0 * scale,
                    appearance.muted,
                ))
            })
            // Folded, the heading carries what the bots' rows would show: their status and how many.
            .when(collapsed && !hovered && folded.any(), |row| {
                row.child(super::status::folded_status_indicator(
                    folded.working,
                    folded.attention,
                    folded.background_work,
                    folded.question,
                    folded.approval,
                    scale,
                    titlebar_background(),
                ))
            })
            .child(div().flex_1())
            .when(collapsed && !hovered, |row| {
                row.child(folded.bots.to_string())
            })
            .when(hovered, |row| {
                row.child(
                    div()
                        .id(format!("native-sidebar-gbots-add-{storage_id}"))
                        .size(px(16.0 * scale))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(4.0 * scale))
                        .hover(|button| button.bg(chrome_ink().opacity(0.08)))
                        .child(titlebar_svg_icon(
                            "titlebar/plus.svg",
                            12.0 * scale,
                            appearance.muted,
                        ))
                        .on_click(cx.listener(|app, _, window, cx| {
                            cx.stop_propagation();
                            app.run_native_sidebar_gbots_command(
                                &gbots_command("newGbot", json!({})),
                                window,
                                cx,
                            );
                        })),
                )
            })
            .on_hover(cx.listener(move |app, hovered, _, cx| {
                if *hovered {
                    app.native_sidebar.hovered_section = Some(hover_key.clone());
                } else if app.native_sidebar.hovered_section.as_ref() == Some(&hover_key) {
                    app.native_sidebar.hovered_section = None;
                }
                cx.notify();
            }))
            .on_click(cx.listener(move |app, _, _, cx| {
                cx.stop_propagation();
                app.dispatch_native_sidebar_ui(
                    json!({"type": "toggleGroup", "groupId": fold_id}),
                    cx,
                );
            }))
            .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                cx.stop_propagation();
                Self::show_native_sidebar_context_menu(
                    &heading_menu(),
                    menu_owner.get(),
                    event.position,
                    scale,
                    window,
                    cx,
                );
            })
            .map(|row| {
                self.sidebar_focus_stop(
                    row,
                    stop_key,
                    super::focus_order::SidebarStopPlace::List,
                    appearance,
                    cx,
                )
            })
            .into_any_element()
    }

    /// The bot blocks of `group_ids`, in that order.
    pub(super) fn render_native_gbot_blocks(
        &self,
        group_ids: &[String],
        snapshot: &NativeSidebarSnapshot,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> Vec<AnyElement> {
        group_ids
            .iter()
            .filter_map(|group_id| {
                let group = snapshot
                    .groups
                    .iter()
                    .find(|group| &group.group_id == group_id)?;
                Some(self.render_native_gbot_block(group, appearance, cx))
            })
            .collect()
    }

    /// One bot: its row and, while it is open or folding, its chats.
    fn render_native_gbot_block(
        &self,
        group: &NativeSidebarGroup,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let key = format!("group:{}", group.group_id);
        let body = self
            .native_sidebar
            .disclosures
            .present(&key, group.collapsed)
            .then(|| {
                let content = if group.sessions.is_empty() {
                    self.render_native_gbot_empty(group, appearance, cx)
                } else {
                    self.render_native_session_rows(group, group.sessions.clone(), appearance, cx)
                };
                let content = div()
                    .w_full()
                    .pl(px(12.0 * appearance.scale))
                    .child(content)
                    .into_any_element();
                self.render_native_disclosure(key, content, cx)
            });
        v_flex()
            .w_full()
            .flex_shrink_0()
            .child(self.render_native_gbot_row(group, appearance, cx))
            .children(body)
            .into_any_element()
    }

    /// The bot's own row: chevron on hover, its icon in its colour, its name, and its status.
    fn render_native_gbot_row(
        &self,
        group: &NativeSidebarGroup,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let Some(look) = group.gbot() else {
            return div().into_any_element();
        };
        let machine_id = group.machine_id();
        let menu = gbot_menu(&look, &machine_id);
        let hover_key = format!("gbot-{}", group.group_id);
        let hovered = self.native_sidebar.hovered_section.as_ref() == Some(&hover_key);
        let count = |key: &str| group.summary[key].as_u64().unwrap_or_default() > 0;
        let status = super::status::folded_status_indicator(
            count("workingCount"),
            count("attentionCount"),
            count("backgroundWorkCount"),
            count("questionCount"),
            count("approvalCount"),
            scale,
            titlebar_background(),
        );
        let busy = [
            "workingCount",
            "attentionCount",
            "backgroundWorkCount",
            "questionCount",
            "approvalCount",
        ]
        .iter()
        .any(|key| count(key));
        let fold_id = group.group_id.clone();
        let stop_key = format!("gbot:{}", group.group_id);
        let mut tooltip = if look.title.is_empty() {
            look.name.clone()
        } else {
            format!("{} · {}", look.name, look.title)
        };
        if look.is_lead {
            tooltip.push_str(" · Leads its department");
        }
        if look.computer_use {
            tooltip.push_str(" · May use the screen and apps");
        }
        let computer_chip = look.computer.clone();
        let owner = super::menus::MenuOwner::default();
        let menu_owner = owner.clone();
        h_flex()
            .id(format!("native-sidebar-gbot-{}", group.group_id))
            .on_prepaint(super::menus::record_menu_owner(&owner))
            .role(gpui::Role::Button)
            .aria_label(look.name.clone())
            .aria_expanded(!group.collapsed)
            .mx(px(3.0 * scale))
            .h(px(32.0 * scale))
            .pl(px(5.0 * scale))
            .pr(px(6.0 * scale))
            .gap(px(7.0 * scale))
            .rounded(px(5.0 * scale))
            .text_color(appearance.foreground)
            .when(look.paused, |row| row.opacity(0.55))
            .hover(|row| row.bg(appearance.hover))
            .child(
                div()
                    .size(px(16.0 * scale))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(if hovered {
                        titlebar_svg_icon(
                            if group.collapsed {
                                COMMAND_ICON_CHEVRON_RIGHT
                            } else {
                                COMMAND_ICON_CHEVRON_DOWN
                            },
                            12.0 * scale,
                            appearance.muted,
                        )
                        .into_any_element()
                    } else {
                        svg()
                            .size(px(15.0 * scale))
                            .path(format!("titlebar/{}.svg", look.icon))
                            .text_color(look.color)
                            .into_any_element()
                    }),
            )
            .child(div().min_w_0().truncate().child(look.name.clone()))
            // Its department's lead: a small crown after the name.
            .when(look.is_lead, |row| {
                row.child(
                    svg()
                        .flex_shrink_0()
                        .size(px(12.0 * scale))
                        .path("titlebar/coordinator-crown.svg")
                        .text_color(appearance.muted),
                )
            })
            .child(div().flex_1())
            // Where its chats run when that isn't this computer: the computer's or box's name.
            .when_some(computer_chip, |row, (kind, name)| {
                row.child(
                    h_flex()
                        .flex_shrink_1()
                        .min_w_0()
                        .max_w(px(96.0 * scale))
                        .gap(px(3.0 * scale))
                        .items_center()
                        .text_size(px(11.0 * scale))
                        .text_color(appearance.muted)
                        .child(
                            svg()
                                .flex_shrink_0()
                                .size(px(11.0 * scale))
                                .path(if kind == "cloudBox" {
                                    "titlebar/cloud.svg"
                                } else {
                                    "titlebar/device-laptop.svg"
                                })
                                .text_color(appearance.muted),
                        )
                        .child(div().min_w_0().truncate().child(name)),
                )
            })
            .when(look.paused, |row| {
                row.child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(11.0 * scale))
                        .text_color(appearance.muted)
                        .child("Paused"),
                )
            })
            .when(busy, |row| row.child(status))
            .tooltip(move |window, cx| {
                super::tooltips::sidebar_style_tooltip(tooltip.clone(), window, cx)
            })
            .on_hover(cx.listener(move |app, hovered, _, cx| {
                if *hovered {
                    app.native_sidebar.hovered_section = Some(hover_key.clone());
                } else if app.native_sidebar.hovered_section.as_ref() == Some(&hover_key) {
                    app.native_sidebar.hovered_section = None;
                }
                cx.notify();
            }))
            .on_click(cx.listener(move |app, _, _, cx| {
                cx.stop_propagation();
                app.dispatch_native_sidebar_ui(
                    json!({"type": "toggleGroup", "groupId": fold_id}),
                    cx,
                );
            }))
            .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                cx.stop_propagation();
                Self::show_native_sidebar_context_menu(
                    &menu,
                    menu_owner.get(),
                    event.position,
                    scale,
                    window,
                    cx,
                );
            })
            .map(|row| {
                self.sidebar_focus_stop(
                    row,
                    stop_key,
                    super::focus_order::SidebarStopPlace::List,
                    appearance,
                    cx,
                )
            })
            .into_any_element()
    }

    /// A bot with no open chat: a "New Chat" row that opens one, like an empty project's New Session.
    fn render_native_gbot_empty(
        &self,
        group: &NativeSidebarGroup,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let Some(look) = group.gbot() else {
            return div().into_any_element();
        };
        let command = gbots_command(
            "newChat",
            json!({ "botId": look.bot_id, "name": look.name, "machineId": group.machine_id() }),
        );
        div()
            .id(format!("native-sidebar-gbot-empty-{}", group.group_id))
            .h(px(34.0 * scale))
            .mx(px(3.0 * scale))
            .pl(px(26.0 * scale))
            .flex()
            .items_center()
            .rounded(px(5.0 * scale))
            .text_color(appearance.muted)
            .hover(|row| row.bg(appearance.hover))
            .child("New Chat")
            .on_click(cx.listener(move |app, _, window, cx| {
                cx.stop_propagation();
                app.run_native_sidebar_gbots_command(&command, window, cx);
            }))
            .map(|row| {
                self.sidebar_focus_stop(
                    row,
                    format!("gbot-empty:{}", group.group_id),
                    super::focus_order::SidebarStopPlace::List,
                    appearance,
                    cx,
                )
            })
            .into_any_element()
    }

    /// Runs `{ type: "gbots", op, botId?, name?, machineId? }` from the GBots rows and menus.
    pub(crate) fn run_native_sidebar_gbots_command(
        &mut self,
        command: &Value,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let text = |key: &str| {
            command
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let machine = Some(text("machineId"))
            .filter(|machine| !machine.is_empty())
            .unwrap_or_else(|| ghostex_gx_core::LOCAL_MACHINE_ID.to_string());
        let bot_id = text("botId");
        match text("op").as_str() {
            "newGbot" => self.open_gpui_new_gbot_modal(cx),
            "importTeam" => self.open_gpui_gbot_team_import(cx),
            "exportTeam" => self.open_gpui_gbot_team_export(cx),
            "settings" => self.open_gpui_gbot_settings_modal(bot_id, machine, cx),
            "department" => self.open_gpui_gbot_settings_modal_on(
                bot_id,
                machine,
                crate::app::window::GbotSettingsTab::Department,
                cx,
            ),
            "computer" => self.open_gpui_gbot_settings_modal_on(
                bot_id,
                machine,
                crate::app::window::GbotSettingsTab::Computer,
                cx,
            ),
            "removeDepartment" => self.remove_native_sidebar_gbots_department(
                machine,
                text("departmentId"),
                text("name"),
                window,
                cx,
            ),
            "memory" => self.open_gpui_gbot_settings_modal_on(
                bot_id,
                machine,
                crate::app::window::GbotSettingsTab::Memory,
                cx,
            ),
            "newChat" => self.start_native_sidebar_gbot_chat(&machine, bot_id, cx),
            #[cfg(not(target_arch = "wasm32"))]
            "call" => self.call_gbot(&machine, bot_id, text("name"), window, cx),
            "remove" => {
                let name = text("name");
                let answer = window.prompt(
                    gpui::PromptLevel::Warning,
                    &format!("Remove {name}?"),
                    Some("Its chats close and its folder goes to the Trash."),
                    &["Cancel", "Remove GBot"],
                    cx,
                );
                cx.spawn(async move |app, cx| {
                    if answer.await == Ok(1) {
                        let _ = app.update(cx, |app, cx| {
                            app.call_native_sidebar_gbots(
                                &machine,
                                json!({ "op": "remove", "bot": bot_id }),
                                "GBot not removed",
                                |_, _, _| {},
                                cx,
                            );
                        });
                    }
                })
                .detach();
            }
            _ => {}
        }
    }

    /// New Chat: opens another chat with the bot and focuses it.
    fn start_native_sidebar_gbot_chat(
        &mut self,
        machine: &str,
        bot_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let machine_for_row = machine.to_string();
        self.call_native_sidebar_gbots(
            machine,
            json!({ "op": "newChat", "bot": bot_id }),
            "Chat not opened",
            move |app, result, cx| {
                let chat = &result["chat"];
                if let (Some(project_id), Some(session_id)) =
                    (chat["projectId"].as_str(), chat["sessionId"].as_str())
                {
                    let machine = if machine_for_row == ghostex_gx_core::LOCAL_MACHINE_ID {
                        ghostex_gx_core::MachineId::Local
                    } else {
                        ghostex_gx_core::MachineId::Remote(machine_for_row.clone())
                    };
                    let row_id = ghostex_gx_core::SessionKey {
                        machine,
                        project_id: project_id.to_string(),
                        session_id: session_id.to_string(),
                    }
                    .to_sidebar_session_id();
                    app.dispatch_native_sidebar_ui(
                        json!({ "type": "selectSession", "sessionId": row_id, "mode": "focus" }),
                        cx,
                    );
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        let _ = app.react_to_native_sidebar_session_click(&row_id, cx);
                    }
                }
            },
            cx,
        );
    }

    /// Calls `/api/gbots` on `machine` and hands the answer to `done`; a refusal shows as a toast
    /// titled `failed` with gxserver's sentence.
    pub(crate) fn call_native_sidebar_gbots(
        &mut self,
        machine: &str,
        params: Value,
        failed: &'static str,
        done: impl FnOnce(&mut Self, &Value, &mut gpui::Context<Self>) + 'static,
        cx: &mut gpui::Context<Self>,
    ) {
        let target = if machine.is_empty() || machine == ghostex_gx_core::LOCAL_MACHINE_ID {
            None
        } else {
            match self.gpui_remote_gxserver_request_target(machine) {
                Some(target) => Some(target),
                None => {
                    self.dispatch_gpui_app_modal_toast(
                        "warning",
                        "That computer is not connected",
                        "",
                        cx,
                    );
                    return;
                }
            }
        };
        cx.spawn(async move |this, cx| {
            let answer = crate::app::gx_store::gx_rpc(target, "/api/gbots", params).await;
            let _ = this.update(cx, |app, cx| match answer {
                Ok(result) => done(app, &result, cx),
                Err(error) => {
                    app.dispatch_gpui_app_modal_toast("warning", failed, &error.message, cx)
                }
            });
        })
        .detach();
    }
}
