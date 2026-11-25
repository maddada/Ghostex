use super::state::NativeChatView;
use gpui::{Context, Window};
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct NativeChatAction {
    pub(crate) command: Value,
}

/// Toggle keys in `menu_toggle.rs` for the menus this file opens.
pub(super) const MORE_ACTIONS_TRIGGER: &str = "chat-more-actions";
const SEND_ACTIONS_TRIGGER: &str = "chat-send-actions";
const SWITCH_ACCOUNT_TRIGGER: &str = "chat-notice-switch-account";

/// A row of the chat box ⋯ under the one session menu's name and icon for its place (gx-core
/// sidebar_menu/layout.rs).
fn named_row(place: &str, mut row: Value) -> Value {
    if let Some(entry) = ghostex_gx_core::session_menu_layout::entry(place)
        && !entry.label.is_empty()
    {
        row["label"] = json!(entry.label);
        row["iconPath"] = json!(format!("titlebar/{}.svg", entry.icon));
        if entry.danger {
            row["danger"] = json!(true);
        }
    }
    row
}

/// One of the sidebar menu's rows for this session (gx-core `MenuItem` JSON) as a chat menu row:
/// its command goes back to the sidebar's dispatcher (`sidebarMenu`, handled in `handle_action`).
fn sidebar_row_to_chat(item: &Value) -> Value {
    if item["separator"] == true {
        return json!({"separator": true});
    }
    if item["heading"] == true {
        return json!({"heading": true, "label": item["label"]});
    }
    let mut row = json!({ "label": item["label"] });
    if let Some(icon) = item["icon"].as_str() {
        row["iconPath"] = json!(
            crate::app::helpers::gpui_sidebar_command_icon_asset_path(Some(icon)).to_string()
        );
    }
    if let Some(hotkey) = item["hotkey"].as_str() {
        row["hotkeyAction"] = json!(hotkey);
    }
    for flag in ["checked", "disabled", "danger"] {
        if item[flag] == true {
            row[flag] = json!(true);
        }
    }
    // A tag's own colour, and what a row says after its name (Link to's linked ids).
    if let Some(color) = item["iconColor"].as_str() {
        row["iconColor"] = json!(color);
    }
    if let Some(suffix) = item["suffix"].as_str().filter(|suffix| !suffix.is_empty()) {
        row["detail"] = json!(suffix);
    }
    if let Some(children) = item["children"].as_array() {
        row["children"] = Value::Array(children.iter().map(sidebar_row_to_chat).collect());
        row["openOnHover"] = json!(false);
    } else if let Some(command) = item.get("command") {
        row["command"] = json!({"type": "sidebarMenu", "command": command});
    }
    row
}

impl NativeChatView {
    /// Send's own right-click menu.
    ///
    /// CDXC:SessionChat 2026-09-18 WHY:
    /// It is the chat's own popup, not the app shell's native menu, because
    /// React drew it as one more `ghostex-session-chat-popup`
    /// (`session-chat-composer.tsx`).
    pub(crate) fn show_send_actions(
        &mut self,
        position: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.chat_menu_toggled_shut(SEND_ACTIONS_TRIGGER, cx) {
            return;
        }
        let enabled = self.composer_ready && !self.draft.trim().is_empty();
        let can_queue = enabled && self.snapshot["queue"]["capabilities"]["canQueue"] == true;
        let rows = vec![
            json!({
                "label":"Interrupt & Send","iconPath":"titlebar/arrow-up.svg",
                "detail":super::keyboard::chat_chord_label("ctrl+enter"),
                "disabled":!enabled || self.snapshot["working"] != true,
                "command":{"type":"submit","mode":"interrupt"},
            }),
            json!({
                "label":"Compact & Send","iconPath":"titlebar/arrows-diagonal-minimize.svg",
                "detail":super::keyboard::chat_chord_label("alt+enter"),
                "disabled":!can_queue,
                "command":{"type":"submit","mode":"compact"},
            }),
        ];
        self.menu_side = Some(super::option_menu::MenuSide::Right);
        let trigger =
            super::toolbar::button_trigger(self.composer_button_bounds.send.get(), position);
        self.show_chat_menu(rows, trigger, 240.0, window, cx);
    }

    /// The View row: one choice of Summary, Normal and Detailed (gx-chat-core
    /// transcript/view_level.rs has the user's words); picking one turns the others off. Summary
    /// shows only where this chat has it (`composer_control_available("summary")`), and is the
    /// only Summary control besides its hotkey and the strip's chip (toolbar.rs `COMPOSER_CONTROLS`).
    /// The phone's View row holds the same three (apps/mobile/app/src/chat/native/composer/menus.ts).
    ///
    /// CDXC:SessionChat 2026-11-02 SEE-ALSO: supersedes 2026-09-27's three independent checks and 2026-10-29's Simple, Detailed or Verbose level with a Summary switch on top.
    fn view_modes_row(&self) -> Value {
        let summary_available = self.composer_control_available("summary");
        let level = self.snapshot["viewLevel"].as_str().unwrap_or("normal");
        let mut modes: Vec<Value> = [
            ("summary", "Summary", "titlebar/list-check.svg"),
            ("normal", "Normal", "titlebar/leaf.svg"),
            ("detailed", "Detailed", "titlebar/list-details.svg"),
        ]
        .into_iter()
        .filter(|(value, _, _)| *value != "summary" || summary_available)
        .map(|(value, label, icon)| {
            let mut row = json!({"label":label,"iconPath":icon,"checked":level == value,"command":{"type":"setDetailLevel","level":value}});
            if value == "summary" {
                row["hotkeyAction"] = json!("toggleChatSummaryMode");
            }
            row
        })
        .collect();
        // Quiet View, for any chat, under the three levels (gx-chat-core
        // `transcript/quiet_view.rs` has the user's words): the chat remembers its own choice.
        let quiet = &self.snapshot["quietView"];
        if quiet.is_object() {
            let on = quiet["on"] == true;
            let mut row = json!({
                "label":"Quiet View","iconPath":"titlebar/eye-off.svg","checked":on,
                "command":{"type":"setQuietView","on":!on},
            });
            if level == "summary" {
                row["description"] = json!("Summary view folds more, so it wins while it is on");
            }
            modes.push(json!({"separator":true}));
            modes.push(row);
        }
        // Auto-read, under the view settings since layout A (`show_actions`).
        if let Some(auto_read) = self.auto_read_row() {
            modes.push(json!({"separator":true}));
            modes.push(auto_read);
        }
        let detail = match level {
            "summary" => "Summary",
            "detailed" => "Detailed",
            _ => "Normal",
        };
        json!({"label":"View","iconPath":"titlebar/eye.svg","detail":detail,"openOnHover":false,"children":modes})
    }

    /// Read aloud's Auto-read check, in View, and Stop reading, a first-level row while this chat is
    /// being read. Neither shows while Read aloud is off or this computer has no voice
    /// (`readAloud.available`).
    ///
    /// CDXC:VoiceMode 2026-10-20 SEE-ALSO: the user's decision that Auto-read is a switch in this menu, on the phone and the desktop together, is in packages/gx-chat-core/src/read_aloud/actions.rs; the phone's rows are in apps/mobile/app/src/chat/native/composer/menus.ts. Layout A (2026-11-12, `show_actions`) moved Auto-read into View on both.
    fn auto_read_row(&self) -> Option<Value> {
        let read_aloud = &self.snapshot["readAloud"];
        if read_aloud["available"] != true {
            return None;
        }
        let auto = read_aloud["autoRead"] == true;
        let mut auto_row = json!({
            "label":"Auto-read","iconPath":"titlebar/volume.svg","checked":auto,
            "command":{"type":"readAloudAuto","on":!auto},
        });
        // Auto-read is on but waits: said under the row, so a silent chat is not a mystery. The
        // phone's ⋯ menu says the same words (apps/mobile/app/src/chat/native/composer/menus.ts).
        let held = match read_aloud["autoReadHeld"].as_str() {
            Some("voiceConversation") => Some("Quiet during the voice call"),
            Some("dictation") => Some("Quiet while you dictate"),
            _ => None,
        };
        if let Some(held) = held {
            auto_row["description"] = json!(held);
        }
        Some(auto_row)
    }

    fn stop_reading_row(&self) -> Option<Value> {
        let read_aloud = &self.snapshot["readAloud"];
        (read_aloud["available"] == true && read_aloud["speaking"] == true).then(|| {
            json!({
                "iconPath":"titlebar/player-stop.svg",
                "hotkeyAction":"readAloudStop","command":{"type":"readAloudStop"},
            })
        })
    }

    /// Files and links, always in this menu with its hotkey beside it.
    ///
    /// CDXC:FilesAndLinks 2026-10-15 DECISION:
    /// User: "move files & links inside the 3 dots pls and make it open a pop up menu not push the chat transcript please". It is a More actions row, no longer a toolbar button, and it opens a floating menu above the chat box (files_links.rs) instead of the card that pushed the transcript up. The phone puts the same row in its More actions sheet (apps/mobile/app/src/chat/native/composer/menus.ts).
    fn files_links_menu_row(&self) -> Option<Value> {
        self.composer_control_available("files").then(|| {
            json!({
                "iconPath":"titlebar/link.svg",
                "hotkeyAction":"toggleChatFilesLinks",
                "checked":self.snapshot["filesLinks"]["open"] == true,
                "command":{"type":"composerHost","action":"filesLinks"},
            })
        })
    }

    /// Stash prompt, a More actions row since its toolbar button went (toolbar.rs): it stashes the draft, or opens the stashed prompts when the chat box is empty. The stash count the button's badge showed is the row's `badge`, a filled circle after the label (option_menu/render.rs).
    ///
    /// CDXC:SessionChat 2026-10-22 DECISION:
    /// User: "pls make stash prompt 1 line and have a circle with number inside it for the stashed prompts (show this circle after the 'Stash Prompt' text". The row is one line with the count in a circle right after its label, not a "1 stashed" subtitle, and no circle when nothing is stashed. The phone's More actions sheet draws the same `badge` (apps/mobile/app/src/chat/native/composer/menus.ts, MenuSheet.tsx). Supersedes the 2026-10-20 note that made the count the subtitle.
    fn stash_menu_row(&self) -> Option<Value> {
        self.composer_control_available("stash").then(|| {
            json!({
                "iconPath":"titlebar/stack-push.svg",
                "hotkeyAction":"stashPrompt",
                "badge":self.snapshot["composerChrome"]["stashBadge"],
                "command":{"type":"composerHost","action":"stashPrompt"},
            })
        })
    }

    /// The Skills submenu: the Ghostex skills the agent has installed, each putting its pill in the
    /// chat box, then Configure / Install more (Settings > Integrations > Agent skills).
    fn skills_row(&self) -> Value {
        let mut skills: Vec<Value> = self.snapshot["ghostexSkills"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|skill| {
                json!({
                    "label":skill["name"],"iconPath":"titlebar/wand.svg","description":skill["description"],
                    "command":{"type":"insertSkill","name":skill["name"]},
                })
            })
            .collect();
        if skills.is_empty() {
            skills.push(json!({"label":"No Ghostex skills installed","iconPath":"titlebar/puzzle.svg","disabled":true}));
        }
        skills.push(json!({"separator":true}));
        skills.push(json!({"label":"Configure / Install more","iconPath":"titlebar/settings.svg","command":{"type":"openSkillsSettings"}}));
        json!({"label":"Skills","iconPath":"titlebar/sparkles.svg","openOnHover":false,"children":skills})
    }

    /// The chat box ⋯: the one session menu (gx-core sidebar_menu/layout.rs) as the chat box fills
    /// it. The shared rows the chat core has no action for (Coordinator, Mark Thread Done, Copy,
    /// Tag As, Close After Done, Handoff / Export, Link to, Make Coordinator, Postpone By, Close)
    /// are the sidebar menu's own rows for this session, read from the store, so both menus run
    /// the same commands; the store is read after this click, never while the app is mid-update.
    pub(crate) fn show_actions(
        &mut self,
        position: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.chat_menu_toggled_shut(MORE_ACTIONS_TRIGGER, cx) {
            return;
        }
        let Some(app) = self.config.app.clone() else {
            self.open_actions(Vec::new(), position, window, cx);
            return;
        };
        let key = self.links_session_key();
        cx.spawn_in(window, async move |chat, cx| {
            let places = app
                .update(cx, |app, _| app.gx_store_session_menu_places(&key))
                .ok()
                .flatten()
                .unwrap_or_default();
            let _ = chat.update_in(cx, |chat, window, cx| {
                chat.open_actions(places, position, window, cx)
            });
        })
        .detach();
    }

    fn open_actions(
        &mut self,
        places: Vec<(String, Vec<Value>)>,
        position: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use ghostex_gx_core::session_menu_layout::{
            LayoutNode, MORE, SessionMenuSpot, layout, tidy_separators,
        };
        let shared: std::collections::HashMap<String, Vec<Value>> = places.into_iter().collect();
        let is_separator = |row: &Value| row["separator"] == true;
        let mut rows = Vec::new();
        for node in layout(SessionMenuSpot::ChatBox) {
            match node {
                LayoutNode::Separator => rows.push(json!({"separator":true})),
                LayoutNode::Row(place) => rows.extend(self.chat_box_place(place, &shared)),
                LayoutNode::Tiles(places) => {
                    let tiles: Vec<Value> = places
                        .iter()
                        .flat_map(|place| self.chat_box_place(place, &shared))
                        .filter(|tile| tile.get("command").is_some())
                        .collect();
                    if !tiles.is_empty() {
                        rows.push(json!({ "quick": tiles }));
                    }
                }
                LayoutNode::More(children) => {
                    let more: Vec<Value> = children
                        .iter()
                        .flat_map(|child| match child {
                            LayoutNode::Row(place) => self.chat_box_place(place, &shared),
                            _ => vec![json!({"separator":true})],
                        })
                        .collect();
                    let more = tidy_separators(more, is_separator);
                    if !more.is_empty() {
                        rows.push(json!({
                            "label": MORE.label,
                            "iconPath": format!("titlebar/{}.svg", MORE.icon),
                            "openOnHover": false,
                            "children": more,
                        }));
                    }
                }
                // Only the project menu's layout has named submenus.
                LayoutNode::Submenu(..) => {}
            }
        }
        let rows = tidy_separators(rows, is_separator);
        // Lined up with the More actions button, not the point it was clicked at.
        self.menu_side = Some(super::option_menu::MenuSide::Right);
        let trigger = super::toolbar::button_trigger(
            self.composer_button_bounds.more_actions.get(),
            position,
        );
        self.show_chat_menu(rows, trigger, 240.0, window, cx);
    }

    /// What the chat box shows at one place of the one session menu.
    fn chat_box_place(
        &self,
        place: &str,
        shared: &std::collections::HashMap<String, Vec<Value>>,
    ) -> Vec<Value> {
        let named = |row: Value| -> Value { named_row(place, row) };
        match place {
            // The chat core's own session actions, with their hotkeys: the core offers each only
            // where this chat's agent can do it (Fork only for agents gxserver can fork).
            "rename" | "sleep" | "fork" | "fullReload" | "delayedSend" => {
                let id = if place == "delayedSend" {
                    "delayedActions"
                } else {
                    place
                };
                self.host_action(id)
                    .map(|action| {
                        named(json!({
                            "hotkeyAction": action["hotkey"],
                            "command": {"type":"host","action":id},
                        }))
                    })
                    .into_iter()
                    .collect()
            }
            "switchAccount" => self.switch_account_row().into_iter().collect(),
            "note" => self.note_row().map(named).into_iter().collect(),
            "filesLinks" => self.files_links_menu_row().map(named).into_iter().collect(),
            // Side chat: the core offers it only for agents that take `/btw` (composer/side_chat.rs).
            "sideChat" => self.snapshot["sideChat"]
                .as_str()
                .map(|prefix| {
                    named(json!({
                        "checked": self.draft.starts_with(prefix),
                        "command": {"type":"toggleSideChat","text":self.draft},
                    }))
                })
                .into_iter()
                .collect(),
            "chatToolbar" => self.folded_toolbar_rows(),
            "switchToNative" => self.switch_to_native_menu_row().into_iter().collect(),
            "view" => vec![self.view_modes_row()],
            "stopReading" => self.stop_reading_row().map(named).into_iter().collect(),
            "branches" => self.branches_row().into_iter().collect(),
            "stashPrompt" => self.stash_menu_row().map(named).into_iter().collect(),
            "skills" => vec![self.skills_row()],
            "chatOptions" => self.folded_option_rows(),
            _ => match shared.get(place) {
                Some(items) => items.iter().map(sidebar_row_to_chat).collect(),
                None => self.unlisted_place(place, shared.is_empty()),
            },
        }
    }

    /// A shared place the store had no row for. A coordinator's chat still offers View Playbook
    /// from the chat core, and a session no list draws (a filter hides another computer's row,
    /// or the list is not ready) still offers Close, so those never vanish with the sidebar row.
    fn unlisted_place(&self, place: &str, unlisted: bool) -> Vec<Value> {
        match place {
            "coordinator" => self
                .host_action("viewPlaybook")
                .map(|action| {
                    json!({
                        "label": action["label"],
                        "iconPath": "titlebar/book.svg",
                        "command": {"type":"host","action":"viewPlaybook"},
                    })
                })
                .into_iter()
                .collect(),
            "close" if unlisted => {
                let session_id = self.links_session_key().to_sidebar_session_id();
                vec![named_row(
                    "close",
                    json!({
                        "command": {"type":"sidebarMenu","command":{"type":"command","message":{
                            "type":"closeSession","sessionId":session_id,"closeSource":"sidebarMenu",
                        }}},
                    }),
                )]
            }
            _ => Vec::new(),
        }
    }

    /// Manual "Switch to Native" (or "Cancel Switch to Native" while queued) row in `•••`
    /// (`Q4768`, `Q4770`, `Q4771`, `Q4772`): stays available even after the strip chip is
    /// dismissed or on coordinator threads, and hides quietly on older remote servers.
    fn switch_to_native_menu_row(&self) -> Option<Value> {
        let offer = self.native_switch_offer()?;
        if offer["showMenuItem"] != true {
            return None;
        }
        if offer["queued"] == true {
            return Some(json!({
                "label": "Cancel Switch to Native",
                "iconPath": "titlebar/clock.svg",
                "detail": "When this reply ends",
                "command": { "type": "host", "action": "cancelSwitchToNative" },
            }));
        }
        Some(json!({
            "label": "Switch to Native",
            "iconPath": "titlebar/bolt.svg",
            "info": ghostex_gx_chat_core::extras::native_switch::SWITCH_TO_NATIVE_INFO,
            "command": {
                "type": "host",
                "action": "switchToNative",
                "family": offer["family"],
                "agentName": offer["agentName"],
                "nativeDefault": offer["nativeDefault"],
                "working": self.snapshot["working"] == true,
                "source": "more_menu",
            },
        }))
    }

    fn host_action(&self, id: &str) -> Option<&Value> {
        self.snapshot["hostActions"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|action| action["id"] == id)
    }

    /// The chat box controls that overflowed its toolbar fold in here, on top. A folded Session
    /// Note is the menu's Note row instead.
    fn folded_toolbar_rows(&self) -> Vec<Value> {
        let mut rows = Vec::new();
        for (id, action, label, icon) in super::toolbar::COMPOSER_CONTROLS {
            // An overflowed control folds into this menu only when the host gave it a handler.
            if id != "note"
                && self.composer_control_overflowed(id)
                && self.composer_control_available(id)
            {
                let (label, icon) = if id == "maximize" && self.maximized_window.is_some() {
                    ("Exit maximize", "titlebar/minimize.svg")
                } else if id == "terminal" && self.snapshot["nonCliMode"] == true {
                    ("Switch to Terminal-based", icon)
                } else {
                    (label, icon)
                };
                let mut row = json!({"label":label,"iconPath":icon,"command":{"type":"composerHost","action":action}});
                if id == "terminal" && self.snapshot["nonCliMode"] == true {
                    row["info"] = json!(
                        ghostex_gx_chat_core::extras::native_switch::SWITCH_TO_TERMINAL_BASED_INFO
                    );
                }
                if id == "maximize" {
                    row["hotkeyAction"] = json!("toggleChatComposerMaximized");
                }
                // Off the row, a terminal problem shows here: the reason under Terminal View
                // and Copy Report after it (CDXC:Experience 2026-11-15 in gx-chat-core
                // composer/row.rs).
                let issue = self.snapshot["composerRow"]["terminalIssue"].as_str();
                if id == "terminal"
                    && let Some(issue) = issue
                {
                    row["description"] = json!(issue);
                }
                rows.push(row);
                if id == "terminal" && issue.is_some() {
                    rows.push(json!({"label":"Copy Report","iconPath":"titlebar/copy.svg","command":{"type":"copyReport"}}));
                }
            }
        }
        rows
    }

    /// Switch Account, opened on click only.
    fn switch_account_row(&self) -> Option<Value> {
        self.host_action("switchAccount")?;
        let icon = format!(
            "titlebar/{}.svg",
            ghostex_gx_core::session_menu_layout::entry("switchAccount")?.icon
        );
        /*
        CDXC:AgentProviders 2026-09-18 DECISION:
        User: Switch Account in GPUI chat works like React chat. Claude and Codex
        sessions open the Accounts & limits panel (SessionAccountsPanel); other
        agents keep the daemon's switchable-agent rows. Either submenu opens on
        click only, per the 2026-09-12 decision below.
        */
        // CDXC:AgentProviders 2026-09-06 DECISION: User requested the Claude and Codex account controls under More actions > Switch Account as a submenu, replacing the standalone composer Accounts button and the old account row.
        // CDXC:AgentProviders 2026-09-12 DECISION: User: opening Switch Account under the chat's More actions requires a click; hovering must not open either version of the account submenu.
        if self.snapshot["accountPanel"].is_object() {
            return Some(named_row(
                "switchAccount",
                json!({"iconPath":icon,"openOnHover":false,"children":[{"accounts":self.snapshot["accountPanel"]}]}),
            ));
        }
        let accounts: Vec<_> = self.snapshot["switchableAgents"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|account| {
                json!({
                    "label":account["name"],"icon":account["icon"],
                    "command":{"type":"host","action":"switchAccount","agentId":account["agentId"]},
                })
            })
            .collect();
        (!accounts.is_empty()).then(|| {
            named_row(
                "switchAccount",
                json!({"iconPath":icon,"openOnHover":false,"children":accounts}),
            )
        })
    }

    /*
    CDXC:SessionChat 2026-10-29 DECISION:
    User, on the terminal view's More actions menu matching the chat's (Q729, "Yes, add it"): the chat's More actions menu gets a Session note row, opening the same note dialog the terminal row opens, with the same "1" badge when a note exists, on the computer and the phone (apps/mobile/app/src/chat/native/composer/menus.ts). The note toolbar button is never offered (the core's `composerActions.note` is off), so this row is the only way in unless the control folded here itself. The one session menu (2026-11-20, gx-core sidebar_menu/layout.rs) names it Note and puts it in More.
    */
    fn note_row(&self) -> Option<Value> {
        // Only while Session notes is on (Settings > Extensions; CDXC:SessionNotes 2026-10-30).
        let note_folded =
            self.composer_control_overflowed("note") && self.composer_control_available("note");
        if note_folded {
            return Some(json!({
                "checked":self.snapshot["note"]["open"],
                "command":{"type":"composerHost","action":"sessionNote"},
            }));
        }
        if !crate::app::session_notes::session_notes_on()
            || !self.composer_control_available("files")
        {
            return None;
        }
        let mut row = json!({"command":{"type":"host","action":"sessionNote"}});
        if self.snapshot["composerChrome"]["notePresence"] == true {
            row["badge"] = json!("1");
        }
        Some(row)
    }

    /// Branches: this conversation's forks, each with the Branches glyph before its lifecycle dot.
    fn branches_row(&self) -> Option<Value> {
        let mut branches = self.fork_branches_row()?;
        if let Some(children) = branches["children"].as_array_mut() {
            for child in children.iter_mut() {
                if child["heading"] != true
                    && child["separator"] != true
                    && child.get("iconPath").is_none()
                {
                    child["iconPath"] = json!("titlebar/git-branch.svg");
                }
            }
        }
        branches["openOnHover"] = json!(false);
        Some(named_row("branches", branches))
    }

    /// Context window and the option settings, when they overflow the chat box.
    fn folded_option_rows(&self) -> Vec<Value> {
        let mut rows = Vec::new();
        let ring_off_row = self.snapshot["composerRow"]["hideContextRing"] == true;
        if self.snapshot["composerOverflow"]["optionsOverflowed"] != true && !ring_off_row {
            return rows;
        }
        // CDXC:SessionDetails 2026-11-12 DECISION: where the chat has the Session details panel, its first
        // tab's Context card and its second tab carry this (Q2763, option A); the row stays for a chat
        // without one.
        if self.snapshot["contextMeter"].is_object() && !self.session_details_available() {
            rows.push(json!({"label":"Context window","iconPath":"titlebar/circle-half.svg","detail":self.snapshot["contextMeter"]["percentage"],"openOnHover":false,"children":self.context_menu_rows()}));
        }
        if self.snapshot["composerOverflow"]["optionsOverflowed"] == true
            && self.snapshot["optionLabels"]["showOptions"] == true
            && !self.snapshot["modelMenu"].is_object()
        {
            let options: Vec<Value> = self.snapshot["optionMenus"]["options"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|row| {
                    let mut row = row.clone();
                    if row["heading"] != true
                        && row["separator"] != true
                        && row.get("iconPath").is_none()
                    {
                        row["iconPath"] = json!("titlebar/brain.svg");
                    }
                    row
                })
                .collect();
            rows.push(json!({
                "label": self.snapshot["optionLabels"]["optionsTitle"].as_str().unwrap_or("Options"),
                "iconPath":"titlebar/adjustments-horizontal.svg",
                "detail":self.snapshot["optionLabels"]["options"],
                "disabled":self.snapshot["optionLabels"]["options"].as_str().is_none_or(str::is_empty),
                "openOnHover":false,
                "children":options,
            }));
        }
        rows
    }

    /// The notice card's Switch account button: the same panel as More actions > Switch Account.
    pub(crate) fn show_account_panel(
        &mut self,
        position: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.snapshot["accountPanel"].is_object() {
            if self.chat_menu_toggled_shut(SWITCH_ACCOUNT_TRIGGER, cx) {
                return;
            }
            self.show_chat_menu(
                vec![json!({"accounts":self.snapshot["accountPanel"]})],
                gpui::Bounds::new(position, gpui::size(gpui::px(0.0), gpui::px(0.0))),
                super::option_menu::ACCOUNT_PANEL_WIDTH,
                window,
                cx,
            );
        }
    }

    pub(crate) fn handle_action(
        &mut self,
        action: &NativeChatAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if action.command["type"] == "submit" {
            self.submit(
                action.command["mode"].as_str().unwrap_or("send"),
                window,
                cx,
            );
        } else if action.command["type"] == "composerHost" {
            self.perform_composer_action(
                action.command["action"].as_str().unwrap_or_default(),
                window.mouse_position(),
                window,
                cx,
            );
        } else if action.command["type"] == "openAccountsSettings" {
            let mut open = json!({"type":"open","modal":"settings","initialTab":"accounts"});
            if let Some(account) = action.command["reconnectAccountId"].as_str() {
                open["initialAccountsReconnect"] = json!(account);
            }
            // CDXC:AgentProviders 2026-11-13 DECISION: Q2779 B: a session on another computer shows
            // that computer's accounts, so its Reconnect opens Settings › Accounts already on that
            // machine (the page's machine picker), where the sign-in runs there and its page opens
            // here. Before, it opened this computer's accounts, and the same account id matched
            // this computer's working login.
            if self.config.machine_id != ghostex_gx_core::LOCAL_MACHINE_ID {
                open["initialMachineId"] = json!(self.config.machine_id);
            }
            cx.emit(super::state::NativeChatEvent::Host(open));
        } else if action.command["type"]
            == ghostex_gx_chat_core::extras::autopilot::OPEN_SETTINGS_COMMAND
        {
            // The Autopilot pill's menu: Settings > Autopilot.
            cx.emit(super::state::NativeChatEvent::Host(
                json!({"type":"open","modal":"settings","initialTab":"autopilot"}),
            ));
        } else if action.command["type"]
            == ghostex_gx_chat_core::extras::autopilot::OPEN_SUPERVISOR_COMMAND
        {
            // The Autopilot pill's "Open supervisor": the supervisor's session on this chat's
            // computer, opened as a session name's pill opens its session.
            cx.emit(super::state::NativeChatEvent::Host(json!({
                "type": "sessionChatHostAction",
                "action": "openAgentSession",
                "projectId": action.command["projectId"],
                "sessionId": action.command["sessionId"],
            })));
        } else if action.command["type"] == "openSkillsSettings" {
            cx.emit(super::state::NativeChatEvent::Host(json!({
                "type":"open","modal":"settings","initialTab":"integrations","initialSection":"agentSkills",
            })));
        } else if action.command["type"] == "insertSkill" {
            let selection = self
                .input
                .as_ref()
                .map(|input| input.read(cx).selected_range())
                .unwrap_or(self.draft.len()..self.draft.len());
            let start = self.draft[..selection.start].encode_utf16().count();
            let end = self.draft[..selection.end].encode_utf16().count();
            self.invoke(
                json!({"type":"insertSkill","name":action.command["name"],"text":self.draft,"start":start,"end":end}),
                cx,
            );
        } else if action.command["type"] == "saveAsBoost" {
            // Save as Boost on selected words (gx-chat-core `composer/transcript_menu.rs`): a new
            // Boost holding them, named in Settings > Boosts (tabs/boosts.rs).
            let text = action.command["text"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            // CDXC:Boosts 2026-11-20 WHY: the web page has no Settings ("Settings is not available
            // in the browser"), so there Save as Boost saves at once through the core, named by its
            // first words, as the phone does; Settings > Boosts on the computer renames it.
            #[cfg(ghostex_web)]
            self.invoke(json!({"type": "saveAsBoost", "text": text}), cx);
            #[cfg(not(ghostex_web))]
            self.open_boost_settings(Some("newBoost".to_string()), Some(text), cx);
        } else if action.command["type"] == ghostex_gx_chat_core::extras::boosts::PILL_COMMAND {
            // Expand, Edit Boost or Remove on a chat box Boost pill's right-click menu (boosts.rs).
            self.boost_pill_command(&action.command, cx);
        } else if action.command["type"] == "copyText" {
            // Copy Path, Copy URL and the transcript's Copy, with the feedback every other chat copy gives.
            if let Some(text) = action.command["text"].as_str() {
                crate::app::helpers::gpui_copy_to_clipboard(
                    gpui::ClipboardItem::new_string(text.to_owned()),
                    cx,
                );
            }
        } else if action.command["type"] == "imageViewer" {
            self.image_viewer_action(action.command["action"].as_str().unwrap_or_default(), cx);
        } else if action.command["type"] == "appendToDraft" {
            self.append_to_draft(
                action.command["text"].as_str().unwrap_or_default(),
                action.command["inline"] == true,
                cx,
            );
        } else if action.command["type"] == "replyToSelection" {
            self.open_reply_popover(&action.command, window, cx);
        } else if action.command["type"] == "sidebarMenu" {
            cx.emit(super::state::NativeChatEvent::Host(json!({
                "type": "sidebarMenuCommand",
                "command": action.command["command"],
            })));
        } else if action.command["type"] == "host" {
            self.host(
                action.command["action"].as_str().unwrap_or_default(),
                action.command.clone(),
                cx,
            );
        } else {
            self.invoke(action.command.clone(), cx);
        }
    }
}
