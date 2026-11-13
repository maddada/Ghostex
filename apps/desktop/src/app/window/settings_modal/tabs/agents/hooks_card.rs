//! The Session resume hooks card at the bottom of the Agents page: what hooks do, Install all
//! (for the agents that are on, only while one lacks its hook), a ⋯ menu with Uninstall all, an
//! icon-only Refresh, the hook state folder, and whether a hook installs without asking when an
//! agent is turned on.
//!
//! CDXC:AgentHooks 2026-10-06 DECISION:
//! User: "ok implement the plan" for the Agents page redesign. The bulk hook tools leave the top of the agent list for this card at the bottom; the list keeps only a summary line (counting agents that are on) with Fix all. This replaces the 2026-08-28 roster toolbar ("quiet whole-set controls, a readiness chip and an info tooltip"), because a "3/23 hooks ready" count of every supported agent named agents the user never turned on.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{
    ButtonSize, ButtonVariant, RowSpec, SettingsDialogSpec, card_inset, dialog_footer,
    settings_button, settings_button_sized, settings_dialog, settings_icon, settings_icon_button,
    settings_section, toggle_field,
};
use super::super::super::palette::SettingsPalette;
use super::super::super::search::{TabSearch, should_show_setting};
use super::AgentsTab;
use super::icons;
use super::model::{HookStatus, any_hook_removable, hook_agent_id};
use super::turn_on::AUTO_INSTALL_HOOKS;

use gpui::{
    Anchor, AnchoredPositionMode, AnyElement, Bounds, Context, Div, InteractiveElement as _,
    IntoElement as _, MouseDownEvent, ParentElement as _, Pixels, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, anchored, deferred, div, point, px,
};
use gpui_component::{h_flex, v_flex};

impl AgentsTab {
    pub(super) fn render_hooks_card(
        &mut self,
        p: &SettingsPalette,
        search: &TabSearch,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let section = search.section(super::HOOKS_ANCHOR);
        // With a remote machine picked the card is that machine's (remote_hooks.rs): its agents
        // are the CLIs installed there, since turning agents on is this computer's setting.
        let remote = self.shown_machine.is_some();
        let (status, loading) = if remote {
            (self.remote_hook_status(), self.remote_hooks_loading)
        } else {
            (
                self.store
                    .read(cx)
                    .host_payload("agentHookStatus")
                    .map(HookStatus::parse),
                self.hook_status_loading,
            )
        };
        let mut on_hooks: Vec<String> = if remote {
            status
                .iter()
                .flat_map(|status| &status.agents)
                .filter(|item| !matches!(item.status.as_str(), "cliMissing" | "notRequired"))
                .map(|item| item.agent_id.clone())
                .collect()
        } else {
            // An agent Ghostex follows without its hook (`notRequired`, Use agent hooks off)
            // needs none, so it is neither counted nor installed.
            self.all_agents(cx)
                .iter()
                .filter(|agent| agent.enabled)
                .filter_map(hook_agent_id)
                .filter(|hook| {
                    status
                        .as_ref()
                        .and_then(|status| status.item(hook))
                        .is_none_or(|item| item.status != "notRequired")
                })
                .collect()
        };
        on_hooks.sort();
        on_hooks.dedup();
        let mut rows: Vec<AnyElement> = Vec::new();
        if should_show_setting(&section, "agentResumeHooks", None) {
            let ready = status.as_ref().map_or(0, |status| {
                on_hooks
                    .iter()
                    .filter(|hook| {
                        status
                            .item(hook)
                            .is_some_and(|item| item.status == "installed")
                    })
                    .count()
            });
            let summary = match &status {
                Some(status) if status.error_message.is_some() => {
                    "Ghostex could not check the hooks.".to_string()
                }
                Some(_) if remote && on_hooks.is_empty() => format!(
                    "No agent CLI with a hook is installed on {}.",
                    self.cli_machine_label()
                ),
                Some(_) if remote => format!(
                    "{ready} of the {} agent CLIs on {} have their hook.",
                    on_hooks.len(),
                    self.cli_machine_label()
                ),
                Some(_) => format!(
                    "{ready} of the {} agent CLIs you use have their hook.",
                    on_hooks.len()
                ),
                None if loading => "Checking hooks…".to_string(),
                None => "Hooks not checked yet.".to_string(),
            };
            let loading_reason: SharedString = "Hook status is being checked.".into();
            let removable = any_hook_removable(status.as_ref());
            let install_ids = on_hooks.clone();
            // CDXC:Settings 2026-10-10 DECISION:
            // User: "i dont like 3 buttons next to each other like this. and make the refresh button just refresh icon". The row is [Install all] (only while an agent that is on lacks its hook), a ⋯ menu holding Uninstall all, and an icon-only Refresh.
            let needs_install = !install_ids.is_empty()
                && status.as_ref().is_none_or(|status| {
                    status.error_message.is_some()
                        || install_ids.iter().any(|hook| {
                            status
                                .item(hook)
                                .is_none_or(|item| item.status != "installed")
                        })
                });
            let menu_open = self.hooks_menu_open && !loading && removable;
            let trigger_bounds = self.hooks_menu_trigger.clone();
            let menu_trigger = div()
                .flex_shrink_0()
                .on_children_prepainted(capture_child_bounds(trigger_bounds.clone(), 0))
                .child(settings_icon_button(
                    p,
                    "agents-hooks-more",
                    icons::DOTS,
                    16.0,
                    28.0,
                    ButtonVariant::Ghost,
                    Some(if loading {
                        loading_reason.clone()
                    } else if !removable {
                        "No Ghostex hooks are installed.".into()
                    } else {
                        "More".into()
                    }),
                    loading || !removable,
                    |page: &mut Self, _window, cx| {
                        page.hooks_menu_open = !page.hooks_menu_open;
                        cx.notify();
                    },
                    cx,
                ));
            let mut buttons = h_flex()
                .flex_shrink_0()
                .flex_wrap()
                .items_center()
                .justify_end()
                .gap(px(6.0));
            if needs_install {
                buttons = buttons.child(settings_button_sized(
                    p,
                    "agents-hooks-install-all",
                    "Install all",
                    Some(icons::DOWNLOAD),
                    ButtonVariant::Ghost,
                    ButtonSize::Sm,
                    loading,
                    Some(loading_reason.clone()),
                    move |page: &mut Self, _window, cx| {
                        page.install_hooks(Some(install_ids.clone()), cx);
                        cx.notify();
                    },
                    cx,
                ));
            }
            buttons = buttons.child(menu_trigger).child(settings_icon_button(
                p,
                "agents-hooks-refresh",
                icons::REFRESH,
                16.0,
                28.0,
                ButtonVariant::Ghost,
                Some(if loading {
                    loading_reason
                } else {
                    "Refresh".into()
                }),
                loading,
                |page: &mut Self, _window, cx| {
                    page.request_hook_status(cx);
                    cx.notify();
                },
                cx,
            ));
            if menu_open && let Some(bounds) = trigger_bounds.get() {
                let hook_folder = status
                    .as_ref()
                    .map(|status| status.hook_state_directory.clone())
                    .filter(|folder| !folder.is_empty());
                buttons = buttons.child(self.hooks_menu_popup(p, bounds, hook_folder, cx));
            }
            let small = |text: String| {
                div()
                    .min_w_0()
                    .text_size(px(12.5))
                    .line_height(px(18.0))
                    .text_color(hsla(p.muted))
                    .child(text)
            };
            let mut text = v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(14.0))
                        .line_height(px(20.0))
                        .text_color(hsla(p.foreground))
                        // CDXC:AgentHooks 2026-11-13 WHY: Claude Code, Codex and OpenCode need no
                        // hook (server/src/agent_hooks/hook_free.rs), so the card no longer says
                        // hooks are what make resume work; Use agent hooks moved to Debugging.
                        .child("Claude Code, Codex and OpenCode need no hook. Other agents use one to resume the exact conversation after sleep, reload or restart."),
                )
                .child(small(summary));
            if let Some(status) = &status {
                if let Some(message) = &status.error_message {
                    // CDXC:AgentHooks 2026-11-08 WHY: a machine on an older Ghostex showed that
                    // message twice, here in red under the same words at the top of the page (the
                    // remote tile audit, finding 6). The card says only what it means for hooks.
                    text = text.child(if message.contains("runs an older Ghostex") {
                        small(format!(
                            "Needs a newer Ghostex on {}.",
                            self.cli_machine_label()
                        ))
                    } else {
                        div()
                            .text_size(px(12.5))
                            .text_color(hsla(p.destructive))
                            .child(message.clone())
                    });
                }
                // The hook folder's path is not shown here (finding 7: a raw path in the card).
            }
            rows.push(card_inset(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(text)
                    .child(buttons),
            ));
        }
        if !remote && should_show_setting(&section, AUTO_INSTALL_HOOKS, None) {
            let checked = self.store.read(cx).values().bool(AUTO_INSTALL_HOOKS);
            rows.push(toggle_field(
                self,
                p,
                AUTO_INSTALL_HOOKS,
                RowSpec::new("Install the hook when I turn on an agent").description(
                    "Install an agent's session resume hook as soon as you turn it on, without asking.",
                ),
                checked,
                cx,
            ));
        }
        settings_section(p, "Session resume hooks", None, None, rows)
    }

    /// The ⋯ menu under its trigger: one row, Uninstall all.
    fn hooks_menu_popup(
        &mut self,
        p: &SettingsPalette,
        trigger: Bounds<Pixels>,
        hook_folder: Option<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = *p;
        let hover = p.popup_hover;
        // The folder the hooks keep their state in, once a line of the card ("Hook state: …").
        let copy_folder = hook_folder.map(|folder| {
            h_flex()
                .id("agents-hooks-copy-folder")
                .role(gpui::Role::MenuItem)
                .aria_label(SharedString::from("Copy hook folder"))
                .w_full()
                .min_h(px(32.0))
                .px(px(8.0))
                .py(px(6.0))
                .gap(px(8.0))
                .items_center()
                .rounded(px(6.0))
                .text_size(px(14.0))
                .line_height(px(20.0))
                .text_color(hsla(p.foreground))
                .cursor_pointer()
                .hover(move |this| this.bg(hsla(hover)))
                .on_press(cx, move |page, _window, cx| {
                    page.hooks_menu_open = false;
                    super::super::super::store::store_copy_to_clipboard(
                        &page.store,
                        folder.clone(),
                        cx,
                    );
                    cx.notify();
                })
                .child(settings_icon(
                    "modals/settings/copy.svg",
                    16.0,
                    p.foreground,
                ))
                .child("Copy hook folder")
        });
        let row = h_flex()
            .id("agents-hooks-uninstall-all")
            .role(gpui::Role::MenuItem)
            .aria_label(SharedString::from("Uninstall all"))
            .w_full()
            .min_h(px(32.0))
            .px(px(8.0))
            .py(px(6.0))
            .gap(px(8.0))
            .items_center()
            .rounded(px(6.0))
            .text_size(px(14.0))
            .line_height(px(20.0))
            .text_color(hsla(p.foreground))
            .cursor_pointer()
            .hover(move |this| this.bg(hsla(hover)))
            .on_press(cx, |page, window, cx| {
                page.hooks_menu_open = false;
                page.confirming_uninstall_hooks = true;
                page.confirm_focus.focus(window, cx);
                cx.notify();
            })
            .child(settings_icon(icons::TRASH, 16.0, p.foreground))
            .child("Uninstall all");
        deferred(
            anchored()
                .position_mode(AnchoredPositionMode::Window)
                .anchor(Anchor::TopRight)
                .position(point(
                    trigger.origin.x + trigger.size.width,
                    trigger.origin.y + trigger.size.height + px(4.0),
                ))
                .snap_to_window_with_margin(px(8.0))
                .child(
                    v_flex()
                        .id("agents-hooks-menu")
                        .occlude()
                        .w(px(176.0))
                        .p(px(4.0))
                        .rounded(px(MODAL_RADIUS_CONTROL))
                        .border_1()
                        .border_color(hsla(p.popup_border))
                        .bg(hsla(p.popup_background))
                        .shadow_md()
                        .font_family(MODAL_UI_FONT)
                        .on_mouse_down_out(cx.listener(
                            move |page, event: &MouseDownEvent, _window, cx| {
                                if trigger.contains(&event.position) {
                                    return;
                                }
                                page.hooks_menu_open = false;
                                cx.notify();
                            },
                        ))
                        .children(copy_folder)
                        .child(row),
                ),
        )
        .with_priority(1)
        .into_any_element()
    }

    /// The Uninstall hooks for all agents? confirmation, in the Skip permissions? dialog's style.
    ///
    /// CDXC:Settings 2026-10-10 DECISION:
    /// User (via the coordinator): "show a confirmation before Uninstall all (e.g. 'Uninstall hooks for all agents?' with Uninstall / Cancel), using the existing Settings confirm style ('Skip permissions?')".
    pub(super) fn render_uninstall_hooks_dialog(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let p = &p.nested_dialog();
        if !self.confirming_uninstall_hooks {
            return None;
        }
        let cancel = settings_button(
            p,
            "agents-hooks-uninstall-cancel",
            "Cancel",
            None,
            ButtonVariant::Outline,
            false,
            None,
            |page: &mut Self, _window, cx| {
                page.confirming_uninstall_hooks = false;
                cx.notify();
            },
            cx,
        );
        let confirm = settings_button(
            p,
            "agents-hooks-uninstall-confirm",
            "Uninstall",
            None,
            ButtonVariant::DestructiveDialog,
            false,
            None,
            |page: &mut Self, _window, cx| {
                page.confirming_uninstall_hooks = false;
                page.uninstall_hooks(None, cx);
                cx.notify();
            },
            cx,
        );
        let focus = self.confirm_focus.clone();
        Some(settings_dialog(
            p,
            SettingsDialogSpec::new(
                "agents-hooks-uninstall",
                match &self.shown_machine {
                    Some(_) => format!(
                        "Uninstall hooks for all agents on {}?",
                        self.cli_machine_label()
                    ),
                    None => "Uninstall hooks for all agents?".to_string(),
                },
            )
                .width(400.0)
                .spacing(20.0, 16.0)
                .description(
                    "Agents that use a hook will no longer resume the exact conversation after sleep, reload or restart until their hooks are installed again. Claude Code, Codex and OpenCode need none."
                        .into_any_element(),
                ),
            Vec::new(),
            Some(dialog_footer(vec![cancel, confirm])),
            Some(&focus),
            |page: &mut Self, _window, cx| {
                page.confirming_uninstall_hooks = false;
                cx.notify();
            },
            window,
            cx,
        ))
    }
}
