//! The header's Claude cloud button: shown while the work area shows a Claude cloud session, it
//! opens a menu of what only a cloud session does (gx-core `claude_cloud_menu`, which holds the
//! user's decision): its terminal and back, its claude.ai page and link, the account it runs on,
//! a copy on another account and Bring to This Computer.

use std::cell::Cell;
use std::time::Duration;

use ghostex_gx_core::{
    ClaudeCloudMenuAccount, ClaudeCloudMenuEntry, ClaudeCloudMenuPick, ClaudeCloudMenuRow,
    SessionKey,
};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Bounds, ClipboardItem, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, Pixels, StatefulInteractiveElement as _, Styled as _, div, px,
};
use gpui_component::ElementExt as _;
use serde_json::json;

use crate::app::actions::ClaudeCloudMenuPicked;
use crate::app::context_menu::GpuiContextMenu;
use crate::app::helpers::*;
use crate::app::new_thread_picker_lifecycle::new_thread_picker_accounts;
use crate::*;

const ICON_CLOUD: &str = "titlebar/cloud.svg";

thread_local! {
    /// Where the button was last painted, for its menu to hang from.
    static ANCHOR: Cell<Option<Bounds<Pixels>>> = const { Cell::new(None) };
}

/// The action id of a menu row (`ClaudeCloudMenuPicked::pick`).
fn claude_cloud_pick_id(pick: &ClaudeCloudMenuPick) -> &'static str {
    match pick {
        ClaudeCloudMenuPick::ShowTerminal => "terminal",
        ClaudeCloudMenuPick::ShowChat => "chat",
        ClaudeCloudMenuPick::OpenOnClaudeAi => "open",
        ClaudeCloudMenuPick::CopyLink => "copyLink",
        ClaudeCloudMenuPick::Account(_) => "account",
        ClaudeCloudMenuPick::CopyOnAnotherAccount => "copy",
        ClaudeCloudMenuPick::BringBack => "bringBack",
    }
}

impl GhostexGpuiApp {
    /// The cloud session the work area shows: its tab and its key on this computer.
    fn workarea_claude_cloud_session(
        &self,
    ) -> Option<(
        TerminalSessionId,
        SessionKey,
        ghostex_gx_protocol::PresentationClaudeCloud,
    )> {
        let shell = self.focused_agents_or_companion_shell_session_id()?;
        let key = self
            .workspace_terminal_session_key(shell)
            .filter(|key| key.machine.is_local())?;
        let cloud = self
            .gx_store
            .core
            .presentation()
            .session(&key)?
            .claude_cloud
            .clone()?;
        Some((shell, key, cloud))
    }

    pub(crate) fn render_workarea_header_claude_cloud_button(
        &self,
        compact: bool,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let (_, _, cloud) = self.workarea_claude_cloud_session()?;
        let text = titlebar_active_text_color();
        let tooltip = match cloud.account_name.as_deref() {
            Some(account) => format!("Claude cloud · {account}"),
            None => "Claude cloud".to_string(),
        };
        Some(
            div()
                .id("ghostex-gpui-workarea-header-claude-cloud")
                .role(gpui::Role::Button)
                .aria_label("Claude cloud")
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(5.0))
                .h(px(TITLEBAR_CONTROL_HEIGHT))
                .px(px(TITLEBAR_BUTTON_HORIZONTAL_PADDING))
                .rounded(px(TITLEBAR_BUTTON_RADIUS))
                .cursor_default()
                .when(cfg!(target_os = "windows"), |this| this.occlude())
                .hover(|this| this.bg(titlebar_button_hover_color()))
                .child(titlebar_svg_icon(ICON_CLOUD, 14.0, text))
                .when(!compact, |this| {
                    this.child(div().text_size(px(12.0)).text_color(text).child("Cloud"))
                })
                .child(titlebar_svg_icon(
                    TITLEBAR_ICON_CHEVRON_DOWN,
                    11.0,
                    titlebar_icon_color(),
                ))
                .tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.toggle_claude_cloud_menu(cx);
                    }),
                )
                .on_a11y_action(gpui::AccessibleAction::Click, {
                    let app = cx.entity().downgrade();
                    move |_, _, cx| {
                        let _ = app.update(cx, |this, cx| this.toggle_claude_cloud_menu(cx));
                    }
                })
                .on_prepaint(|bounds, _, _| ANCHOR.with(|anchor| anchor.set(Some(bounds))))
                .into_any_element(),
        )
    }

    /// Opens (or closes) the Claude cloud menu under the header button.
    pub(crate) fn toggle_claude_cloud_menu(&mut self, cx: &mut gpui::Context<Self>) {
        let (Some(main), Some(trigger)) =
            (self.main_window_handle, ANCHOR.with(|anchor| anchor.get()))
        else {
            return;
        };
        let Some((shell, key, cloud)) = self.workarea_claude_cloud_session() else {
            return;
        };
        let accounts: Vec<ClaudeCloudMenuAccount> = match &self.new_thread_picker_accounts {
            Some(state) => new_thread_picker_accounts(state)
                .into_iter()
                .filter(|account| account.provider == "claude")
                .map(|account| ClaudeCloudMenuAccount {
                    id: account.id,
                    name: account.name,
                    ready: account.ready,
                })
                .collect(),
            None => {
                // Read for the next opening; this one shows the account without the list.
                self.refresh_gpui_new_thread_picker_accounts(cx);
                Vec::new()
            }
        };
        let showing_terminal = !self.agents_chat_mode_sessions.contains(&shell);
        let action = |row: &ClaudeCloudMenuRow| -> Box<dyn gpui::Action> {
            Box::new(ClaudeCloudMenuPicked {
                shell_session_id: shell.0,
                project_id: key.project_id.clone(),
                session_id: key.session_id.clone(),
                pick: claude_cloud_pick_id(&row.pick),
                account_id: match &row.pick {
                    ClaudeCloudMenuPick::Account(id) => Some(id.clone()),
                    _ => None,
                },
            })
        };
        let mut menu = GpuiContextMenu::new();
        for entry in ghostex_gx_core::claude_cloud_menu(&cloud, showing_terminal, &accounts) {
            menu = match entry {
                ClaudeCloudMenuEntry::Separator => menu.separator(),
                ClaudeCloudMenuEntry::Row(row) => {
                    menu.menu_with_disabled(row.label.clone(), row.disabled, action(&row))
                }
                ClaudeCloudMenuEntry::Accounts { label, rows } if rows.is_empty() => {
                    menu.menu_with_disabled(label, true, Box::new(gpui::NoAction {}))
                }
                ClaudeCloudMenuEntry::Accounts { label, rows } => {
                    let mut submenu = GpuiContextMenu::new();
                    for row in &rows {
                        submenu = submenu.menu_with_check_and_disabled(
                            row.label.clone(),
                            row.checked,
                            row.disabled,
                            action(row),
                        );
                    }
                    menu.submenu_menu(label, submenu)
                }
            };
        }
        let app = cx.entity();
        cx.defer(move |cx| {
            let _ = main.update(cx, |_, window, cx| {
                menu.toggle_below_for_app(app, trigger, window, cx);
            });
        });
    }

    /// Runs a row of the Claude cloud menu.
    pub(crate) fn run_claude_cloud_menu_pick(
        &mut self,
        picked: &ClaudeCloudMenuPicked,
        cx: &mut gpui::Context<Self>,
    ) {
        let key = SessionKey::local(picked.project_id.clone(), picked.session_id.clone());
        let shell = TerminalSessionId(picked.shell_session_id);
        match picked.pick {
            // The same switch as the composer's Terminal View and the terminal bar's Chat View.
            "terminal" | "chat" => self.handoff_agents_session_chat_mode(shell, cx),
            // The sidebar row's own cloud actions (native_sidebar/claude_cloud.rs).
            action @ ("open" | "copy" | "bringBack") => self
                .run_native_sidebar_claude_cloud_action(
                    &json!({ "sessionId": key.to_sidebar_session_id(), "action": action }),
                    cx,
                ),
            "copyLink" => {
                let link = self
                    .gx_store
                    .core
                    .presentation()
                    .session(&key)
                    .and_then(|session| session.claude_cloud.as_ref()?.url.clone());
                if let Some(link) = link {
                    gpui_copy_to_clipboard(ClipboardItem::new_string(link), cx);
                    self.dispatch_gpui_workspace_action_toast(
                        "success",
                        "Copied the claude.ai link",
                        "",
                        cx,
                    );
                }
            }
            "account" => {
                let Some(account_id) = picked.account_id.clone() else {
                    return;
                };
                self.select_claude_cloud_account(key, account_id, cx);
            }
            _ => {}
        }
    }

    /// Runs a cloud session that has not started yet on another saved Claude account
    /// (server/src/claude_cloud/account_pick.rs); the row and the menu follow the session's record.
    fn select_claude_cloud_account(
        &mut self,
        key: SessionKey,
        account_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let params = json!({
            "operation": "select",
            "projectId": key.project_id,
            "sessionId": key.session_id,
            "accountId": account_id,
        });
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    gpui_gxserver_rpc_result("/api/agentAccounts", &params, Duration::from_secs(30))
                })
                .await;
            if let Err(error) = result {
                let _ = this.update(cx, |this, cx| {
                    this.dispatch_gpui_workspace_action_toast(
                        "warning",
                        "Could not change the session's account",
                        &error,
                        cx,
                    );
                });
            }
        })
        .detach();
    }
}
