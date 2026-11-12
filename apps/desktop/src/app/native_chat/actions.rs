use super::{state::NativeChatView, transcript::text};
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

/// More actions row icons, ported from `HOST_ACTION_ICONS` and the Sleep moon in the React chat's
/// `session-chat-composer-actions.tsx`.
fn host_action_icon(id: &str) -> Option<&'static str> {
    Some(match id {
        "delayedActions" => "titlebar/clock-check.svg",
        "fork" => "titlebar/git-fork.svg",
        "fullReload" => "titlebar/refresh.svg",
        "rename" => "titlebar/pencil.svg",
        "sleep" => "titlebar/moon.svg",
        "switchAccount" => "titlebar/switch-horizontal.svg",
        "viewPlaybook" => "titlebar/book.svg",
        _ => return None,
    })
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
                "label":"Stop reading","iconPath":"titlebar/player-stop.svg",
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
                "label":"Files and links","iconPath":"titlebar/link.svg",
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
                "label":"Stash prompt","iconPath":"titlebar/stack-push.svg",
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

    pub(crate) fn show_actions(
        &mut self,
        position: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.chat_menu_toggled_shut(MORE_ACTIONS_TRIGGER, cx) {
            return;
        }
        /*
        CDXC:SessionChat 2026-11-12 DECISION:
        User picked layout A of the "phone chat menu cleanup" page (Q2723) for the chat box's More actions, on the phone and here, then which rows go (Q2725). The first level holds the rows a chat uses (the folded chat-box controls, Files and links, Side chat, View, and Stop reading while this chat is read), then Agent (the session's actions with their hotkeys, then Delayed actions, View playbook, Session note and Branches) and More (Stash prompt, Skills, and the folded Context window and Options). Auto-read moved into View. Close After Done and Handoff / Export left (gx-chat-core `composer/host_actions.rs` no longer offers them here; the sidebar's session menu and the hotkeys keep them). Interrupt & Send and Compact & Send stay in Send's right-click menu. Every row, submenu and value row has its own icon. The phone's sheet (apps/mobile/app/src/chat/native/composer/menus.ts) adds its phone-only rows: Search conversation, the voice call, Prompt editor, Catch me up, Project and Kill session.
        */
        let mut rows = Vec::new();
        for (id, action, label, icon) in super::toolbar::COMPOSER_CONTROLS {
            // An overflowed control folds into this menu only when the host gave it a handler; a
            // folded Session note joins the Agent submenu below.
            if id != "note"
                && self.composer_control_overflowed(id)
                && self.composer_control_available(id)
            {
                let (label, icon) = if id == "maximize" && self.maximized_window.is_some() {
                    ("Exit maximize", "titlebar/minimize.svg")
                } else {
                    (label, icon)
                };
                let mut row = json!({"label":label,"iconPath":icon,"command":{"type":"composerHost","action":action}});
                if id == "maximize" {
                    row["hotkeyAction"] = json!("toggleChatComposerMaximized");
                }
                rows.push(row);
            }
        }
        rows.extend(self.files_links_menu_row());
        // Side chat: the core offers it only for agents that take `/btw` (composer/side_chat.rs).
        if let Some(prefix) = self.snapshot["sideChat"].as_str() {
            rows.push(json!({"label":"Side chat","iconPath":"titlebar/message-circle.svg","checked":self.draft.starts_with(prefix),"command":{"type":"toggleSideChat","text":self.draft}}));
        }
        rows.push(self.view_modes_row());
        rows.extend(self.stop_reading_row());
        let host_row = |action: &Value| {
            json!({
                "label":action["label"],"hotkeyAction":action["hotkey"],
                "iconPath":host_action_icon(action["id"].as_str().unwrap_or_default()),
                "command":{"type":"host","action":action["id"]},
            })
        };
        let actions: Vec<_> = self.snapshot["hostActions"]
            .as_array()
            .into_iter()
            .flatten()
            .collect();
        // Agent: every host action the core offers here (gx-chat-core `composer/host_actions.rs`),
        // the session's own first, then the ones about this chat.
        const AGENT_FIRST: [&str; 5] = ["rename", "sleep", "fork", "fullReload", "switchAccount"];
        let ordered = AGENT_FIRST
            .iter()
            .flat_map(|id| actions.iter().filter(move |action| action["id"] == *id))
            .chain(
                actions
                    .iter()
                    .filter(|action| !AGENT_FIRST.contains(&text(action, "id").as_str())),
            );
        let mut session_rows = Vec::new();
        let mut chat_rows = Vec::new();
        for action in ordered {
            let into = if AGENT_FIRST.contains(&text(action, "id").as_str()) {
                &mut session_rows
            } else {
                &mut chat_rows
            };
            if action["id"] == "switchAccount" {
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
                    into.push(json!({"label":"Switch Account","iconPath":host_action_icon("switchAccount"),"openOnHover":false,"children":[{"accounts":self.snapshot["accountPanel"]}]}));
                    continue;
                }
                let accounts: Vec<_> = self.snapshot["switchableAgents"].as_array().into_iter().flatten().map(|account| json!({
                    "label":account["name"],"icon":account["icon"],
                    "command":{"type":"host","action":"switchAccount","agentId":account["agentId"]},
                })).collect();
                if !accounts.is_empty() {
                    into.push(json!({"label":"Switch Account","iconPath":host_action_icon("switchAccount"),"openOnHover":false,"children":accounts}));
                }
            } else {
                into.push(host_row(action));
            }
        }
        /*
        CDXC:SessionChat 2026-10-29 DECISION:
        User, on the terminal view's More actions menu matching the chat's (Q729, "Yes, add it"): the chat's More actions menu gets a Session note row after Close After Done, opening the same note dialog the terminal row opens, with the same "1" badge when a note exists, on the computer and the phone (apps/mobile/app/src/chat/native/composer/menus.ts). The note toolbar button is never offered (the core's `composerActions.note` is off), so this row is the only way in unless the control folded here itself. Layout A (2026-11-12, above) puts it in the Agent submenu.
        */
        // Only while Session notes is on (Settings > Extensions; CDXC:SessionNotes 2026-10-30).
        let note_folded =
            self.composer_control_overflowed("note") && self.composer_control_available("note");
        if note_folded {
            chat_rows.push(json!({
                "label":"Session note","iconPath":"titlebar/note.svg","checked":self.snapshot["note"]["open"],
                "command":{"type":"composerHost","action":"sessionNote"},
            }));
        } else if crate::app::session_notes::session_notes_on()
            && self.composer_control_available("files")
        {
            let mut row = json!({
                "label":"Session note","iconPath":"titlebar/note.svg","hotkeyAction":"sessionNote",
                "command":{"type":"host","action":"sessionNote"},
            });
            if self.snapshot["composerChrome"]["notePresence"] == true {
                row["badge"] = json!("1");
            }
            chat_rows.push(row);
        }
        if let Some(mut branches) = self.fork_branches_row() {
            // Each fork gets the Branches glyph before its lifecycle dot.
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
            chat_rows.push(branches);
        }
        let mut agent_rows = session_rows;
        if !chat_rows.is_empty() {
            if !agent_rows.is_empty() {
                agent_rows.push(json!({"separator":true}));
            }
            agent_rows.extend(chat_rows);
        }

        // More: Stash prompt, Skills, then the option settings when they overflow.
        let mut more_rows = Vec::new();
        more_rows.extend(self.stash_menu_row());
        more_rows.push(self.skills_row());
        if self.snapshot["composerOverflow"]["optionsOverflowed"] == true {
            if self.snapshot["contextMeter"].is_object() {
                more_rows.push(json!({"label":"Context window","iconPath":"titlebar/circle-half.svg","detail":self.snapshot["contextMeter"]["percentage"],"openOnHover":false,"children":self.context_menu_rows()}));
            }
            if self.snapshot["optionLabels"]["showOptions"] == true
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
                more_rows.push(json!({
                    "label": self.snapshot["optionLabels"]["optionsTitle"].as_str().unwrap_or("Options"),
                    "iconPath":"titlebar/adjustments-horizontal.svg",
                    "detail":self.snapshot["optionLabels"]["options"],
                    "disabled":self.snapshot["optionLabels"]["options"].as_str().is_none_or(str::is_empty),
                    "openOnHover":false,
                    "children":options,
                }));
            }
        }

        rows.push(json!({"separator":true}));
        if !agent_rows.is_empty() {
            rows.push(json!({"label":"Agent","iconPath":"titlebar/robot.svg","openOnHover":false,"children":agent_rows}));
        }
        rows.push(json!({"label":"More","iconPath":"titlebar/dots.svg","openOnHover":false,"children":more_rows}));
        // Lined up with the More actions button, not the point it was clicked at.
        self.menu_side = Some(super::option_menu::MenuSide::Right);
        let trigger = super::toolbar::button_trigger(
            self.composer_button_bounds.more_actions.get(),
            position,
        );
        self.show_chat_menu(rows, trigger, 240.0, window, cx);
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
            cx.emit(super::state::NativeChatEvent::Host(open));
        } else if action.command["type"]
            == ghostex_gx_chat_core::extras::autopilot::OPEN_SETTINGS_COMMAND
        {
            // The Autopilot pill's menu: Settings > Autopilot.
            cx.emit(super::state::NativeChatEvent::Host(
                json!({"type":"open","modal":"settings","initialTab":"autopilot"}),
            ));
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
            self.append_to_draft(action.command["text"].as_str().unwrap_or_default(), cx);
        } else if action.command["type"] == "replyToSelection" {
            self.open_reply_popover(&action.command, window, cx);
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
