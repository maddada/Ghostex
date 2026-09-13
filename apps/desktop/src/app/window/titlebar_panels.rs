mod resources;
mod tips;

// C1 wave-2 extraction: the titlebar popup/reading/tips/resources panel window entities moved verbatim out of main.rs (pure
// move, no logic changes; items made pub(crate) so main.rs and sibling
// modules can still reach them). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use super::resources_style::*;
use crate::app::helpers::*;
use crate::app::titlebar::resources_clean_ram_prompt::gpui_resources_clean_ram_prompt;
use crate::*;

pub(crate) struct GpuiTitlebarAnchoredDropdownState {
    pub(crate) position: Point<Pixels>,
    pub(crate) trigger_bounds: Bounds<Pixels>,
    pub(crate) trigger_bounds_captured: bool,
}

impl Default for GpuiTitlebarAnchoredDropdownState {
    fn default() -> Self {
        Self {
            position: point(px(0.0), px(TITLEBAR_HEIGHT)),
            trigger_bounds: Bounds::default(),
            trigger_bounds_captured: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiTitlebarPopupKind {
    AccountUsage(ExtensionId),
    Actions,
    BrowserActions(BrowserPaneId),
    ContextMenu,
    Extensions,
    Git,
    Help,
    Notifications,
    OpenTargets,
    Resources,
    RemoteSites,
    Tips,
}

impl GpuiTitlebarPopupKind {
    pub(crate) fn diagnostic_label(self) -> &'static str {
        match self {
            Self::AccountUsage(_) => "accountUsage",
            Self::Actions => "actions",
            Self::BrowserActions(_) => "browserActions",
            Self::ContextMenu => "contextMenu",
            Self::Extensions => "extensions",
            Self::Git => "git",
            Self::Help => "help",
            Self::Notifications => "notifications",
            Self::OpenTargets => "openTargets",
            Self::Resources => "resources",
            Self::RemoteSites => "remoteSites",
            Self::Tips => "tips",
        }
    }
}

pub(crate) fn log_gpui_titlebar_popup_repro(event: &str, details: serde_json::Value) {
    #[cfg(target_os = "windows")]
    support_logs::append_repro(
        support_logs::GpuiSupportLog::TitlebarPopupRepro,
        event,
        details,
    );
    #[cfg(not(target_os = "windows"))]
    let _ = (event, details);
}

pub(crate) fn gpui_titlebar_popup_bounds_diagnostic(
    bounds: Option<Bounds<Pixels>>,
) -> serde_json::Value {
    bounds.map_or(serde_json::Value::Null, |bounds| {
        serde_json::json!({
            "height": bounds.size.height.as_f32(),
            "width": bounds.size.width.as_f32(),
            "x": bounds.origin.x.as_f32(),
            "y": bounds.origin.y.as_f32(),
        })
    })
}

pub(crate) fn log_gpui_titlebar_popup_mouse_down(
    kind: GpuiTitlebarPopupKind,
    button: &'static str,
    intent: &'static str,
    open_before: bool,
    trigger_bounds: Option<Bounds<Pixels>>,
    event: &MouseDownEvent,
    window: &Window,
) {
    log_gpui_titlebar_popup_repro(
        "gpui.titlebarPopup.buttonMouseDown",
        serde_json::json!({
            "button": button,
            "intent": intent,
            "kind": kind.diagnostic_label(),
            "mainWindowActive": window.is_window_active(),
            "openBefore": open_before,
            "pointerX": event.position.x.as_f32(),
            "pointerY": event.position.y.as_f32(),
            "triggerBounds": gpui_titlebar_popup_bounds_diagnostic(trigger_bounds),
        }),
    );
}

pub(crate) fn log_gpui_titlebar_popup_anchor(
    kind: GpuiTitlebarPopupKind,
    bounds: Bounds<Pixels>,
    first_capture: bool,
    moved: bool,
    window: &Window,
) {
    log_gpui_titlebar_popup_repro(
        "gpui.titlebarPopup.anchorPrepaint",
        serde_json::json!({
            "bounds": gpui_titlebar_popup_bounds_diagnostic(Some(bounds)),
            "firstCapture": first_capture,
            "kind": kind.diagnostic_label(),
            "mainWindowActive": window.is_window_active(),
            "moved": moved,
        }),
    );
}

pub(crate) struct GpuiTitlebarPopupState {
    pub(crate) kind: GpuiTitlebarPopupKind,
    pub(crate) trigger_bounds: Bounds<Pixels>,
}

#[derive(Clone, Copy)]
pub(crate) struct GpuiTitlebarPopupAnchorState {
    pub(crate) bounds: Bounds<Pixels>,
    pub(crate) trigger_bounds_captured: bool,
}

impl Default for GpuiTitlebarPopupAnchorState {
    fn default() -> Self {
        Self {
            bounds: Bounds::default(),
            trigger_bounds_captured: false,
        }
    }
}

#[derive(Clone)]
pub(crate) enum GpuiTitlebarPopupContent {
    AccountUsage(Entity<super::account_usage::AccountUsagePanel>),
    Menu(Entity<PopupMenu>),
    Reading(Entity<GpuiTitlebarReadingPanel>),
    RemoteSites(Entity<crate::app::window::remote_sites::RemoteSitesPanel>),
}

pub(crate) struct GpuiTitlebarPopupWindow {
    main_app: gpui::WeakEntity<GhostexGpuiApp>,
    kind: GpuiTitlebarPopupKind,
    content: GpuiTitlebarPopupContent,
    logged_first_render: bool,
    _dismiss_subscription: Option<gpui::Subscription>,
}

impl GpuiTitlebarPopupWindow {
    pub(crate) fn new(
        main_app: gpui::WeakEntity<GhostexGpuiApp>,
        kind: GpuiTitlebarPopupKind,
        content: GpuiTitlebarPopupContent,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        let content_kind = match &content {
            GpuiTitlebarPopupContent::AccountUsage(_) => "accountUsage",
            GpuiTitlebarPopupContent::Menu(_) => "menu",
            GpuiTitlebarPopupContent::Reading(_) => "reading",
            GpuiTitlebarPopupContent::RemoteSites(_) => "remoteSites",
        };
        log_gpui_titlebar_popup_repro(
            "gpui.titlebarPopup.windowConstructing",
            serde_json::json!({
                "contentKind": content_kind,
                "kind": kind.diagnostic_label(),
                "popupWindowActive": window.is_window_active(),
            }),
        );
        /*
        PopupMenu dispatches a clicked row's typed action through the focused
        element in its own window. This dropdown lives in a non-activating
        panel, so opening the OS window does not establish GPUI focus for the
        menu automatically. Focus the menu internally without activating the
        panel so mouse selections reach this popup root's action listeners.
        */
        if let GpuiTitlebarPopupContent::Menu(menu) = &content {
            menu.focus_handle(cx).focus(window, cx);
        }
        if let GpuiTitlebarPopupContent::AccountUsage(panel) = &content {
            panel.update(cx, |panel, cx| panel.focus(window, cx));
        }
        cx.new(|cx| {
            let dismiss_subscription = match &content {
                GpuiTitlebarPopupContent::Menu(menu) => Some(cx.subscribe_in(
                    menu,
                    window,
                    |this: &mut Self, _, _: &DismissEvent, window, cx| {
                        this.close_from_popup_window(window, cx);
                    },
                )),
                GpuiTitlebarPopupContent::Reading(_) => None,
                GpuiTitlebarPopupContent::RemoteSites(_) => None,
                GpuiTitlebarPopupContent::AccountUsage(_) => None,
            };
            Self {
                main_app,
                kind,
                content,
                logged_first_render: false,
                _dismiss_subscription: dismiss_subscription,
            }
        })
    }

    fn close_from_popup_window(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let kind = self.kind;
        log_gpui_titlebar_popup_repro(
            "gpui.titlebarPopup.popupDismissed",
            serde_json::json!({
                "kind": kind.diagnostic_label(),
                "popupWindowActive": window.is_window_active(),
            }),
        );
        let _ = self.main_app.update_in(cx, |app, _main_window, cx| {
            app.clear_gpui_titlebar_popup_from_window(kind, cx);
        });
        window.remove_window();
    }

    fn update_main_window(
        &self,
        cx: &mut gpui::Context<Self>,
        update: impl FnOnce(&mut GhostexGpuiApp, &mut Window, &mut gpui::Context<GhostexGpuiApp>),
    ) {
        let _ = self.main_app.update_in(cx, update);
    }

    pub(crate) fn dismiss_account_reset_menu(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        match &self.content {
            GpuiTitlebarPopupContent::AccountUsage(panel) => {
                panel.update(cx, |panel, cx| panel.dismiss_reset_menu(window, cx))
            }
            _ => false,
        }
    }

    pub(crate) fn update_account_usage(
        &mut self,
        account: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        if let GpuiTitlebarPopupContent::AccountUsage(panel) = &self.content {
            panel.update(cx, |panel, cx| panel.update_account(account, cx));
        }
    }

    pub(crate) fn update_tips_runtime_status(
        &mut self,
        payload: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        if let GpuiTitlebarPopupContent::Reading(panel) = &self.content {
            panel.update(cx, |panel, cx| {
                panel.update_tips_runtime_status(payload, cx);
            });
        }
    }

    pub(crate) fn update_notifications_feed(
        &mut self,
        feed: crate::notification_feed::GpuiNotificationFeedState,
        cx: &mut gpui::Context<Self>,
    ) {
        if let GpuiTitlebarPopupContent::Reading(panel) = &self.content {
            panel.update(cx, |panel, cx| {
                panel.update_notifications_feed(feed, cx);
            });
        }
    }

    pub(crate) fn update_tips_sidebar_agent_ids(
        &mut self,
        sidebar_agent_ids: HashSet<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        if let GpuiTitlebarPopupContent::Reading(panel) = &self.content {
            panel.update(cx, |panel, cx| {
                panel.update_tips_sidebar_agent_ids(sidebar_agent_ids, cx);
            });
        }
    }
}

impl Render for GpuiTitlebarPopupWindow {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _profile = crate::profiling::span(crate::profiling::Metric::PopupRender);
        if !self.logged_first_render {
            self.logged_first_render = true;
            log_gpui_titlebar_popup_repro(
                "gpui.titlebarPopup.firstRender",
                serde_json::json!({
                    "kind": self.kind.diagnostic_label(),
                    "popupWindowActive": window.is_window_active(),
                    "windowBounds": gpui_titlebar_popup_bounds_diagnostic(Some(window.bounds())),
                }),
            );
        }
        div()
            .id("ghostex-gpui-titlebar-popup-window")
            .size_full()
            .overflow_hidden()
            .key_context(TITLEBAR_DROPDOWN_KEY_CONTEXT)
            .on_action(cx.listener(|this, _: &TitlebarDropdownCancel, window, cx| {
                if this.dismiss_account_reset_menu(window, cx) {
                    cx.stop_propagation();
                    return;
                }
                this.close_from_popup_window(window, cx);
            }))
            .on_action(cx.listener(
                |this, action: &OpenBrowserPaneInExternalBrowser, _window, cx| {
                    this.update_main_window(cx, |_, window, cx| {
                        window.dispatch_action(Box::new(action.clone()), cx);
                    });
                },
            ))
            .on_action(
                cx.listener(|this, action: &SetBrowserPageAppearance, _window, cx| {
                    this.update_main_window(cx, |_, window, cx| {
                        window.dispatch_action(Box::new(action.clone()), cx);
                    });
                }),
            )
            .on_action(
                cx.listener(|this, _: &OpenGpuiOpenTargetsModal, _window, cx| {
                    this.update_main_window(cx, |app, window, cx| {
                        app.open_gpui_app_modal_from_titlebar(
                            GpuiAppModalKind::OpenTargets,
                            window,
                            cx,
                        );
                    });
                }),
            )
            .on_action(
                cx.listener(|this, action: &OpenGpuiWorkspaceInTarget, _window, cx| {
                    this.update_main_window(cx, |app, window, cx| {
                        app.open_active_project_with_open_target_index(
                            action.target_index as usize,
                            window,
                            cx,
                        );
                    });
                }),
            )
            .on_action(
                cx.listener(|this, action: &RunGpuiTitlebarAction, _window, cx| {
                    this.update_main_window(cx, |app, window, cx| {
                        app.run_gpui_titlebar_action_index(
                            action.action_index as usize,
                            window,
                            cx,
                        );
                    });
                }),
            )
            .on_action(
                cx.listener(|this, action: &LaunchGpuiExtension, window, cx| {
                    let extension_id = action.extension_id.clone();
                    this.update_main_window(cx, move |app, main_window, cx| {
                        let Some(trigger_bounds) = app
                            .titlebar_popup_menu
                            .as_ref()
                            .filter(|state| state.kind == GpuiTitlebarPopupKind::Extensions)
                            .map(|state| state.trigger_bounds)
                        else {
                            return;
                        };
                        app.launch_extension_from_titlebar(
                            &extension_id,
                            trigger_bounds,
                            main_window,
                            cx,
                        );
                    });
                    this.close_from_popup_window(window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, action: &ToggleGpuiExtensionPin, window, cx| {
                    let extension_id = action.extension_id.clone();
                    let pinned = action.pinned;
                    this.update_main_window(cx, move |app, _window, cx| {
                        app.update_extension_pin(extension_id, pinned, cx);
                    });
                    this.close_from_popup_window(window, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &BrowseGpuiExtensions, window, cx| {
                this.update_main_window(cx, |app, main_window, cx| {
                    app.open_gpui_settings_extensions_page(Some(main_window), cx);
                });
                this.close_from_popup_window(window, cx);
            }))
            .on_action(
                cx.listener(|this, action: &RunGpuiTitlebarGitMenuAction, _window, cx| {
                    this.update_main_window(cx, |app, _window, cx| {
                        app.run_gpui_titlebar_git_menu_row(action.row_index as usize, cx);
                    });
                }),
            )
            .on_action(
                cx.listener(|this, _: &CopyGpuiTitlebarGitBranch, _window, cx| {
                    this.update_main_window(cx, |app, _window, cx| {
                        let Some(branch) = app
                            .titlebar_git_menu_state
                            .as_ref()
                            .and_then(|state| state.branch.clone())
                        else {
                            return;
                        };
                        cx.write_to_clipboard(ClipboardItem::new_string(branch));
                    });
                }),
            )
            .on_action(
                cx.listener(|this, _: &OpenGpuiTitlebarGitCommitScreen, _window, cx| {
                    this.update_main_window(cx, |app, _window, cx| {
                        app.dispatch_gpui_titlebar_git_action_selector(
                            GpuiTitlebarGitMenuActionId::Commit.selector(),
                            cx,
                        );
                    });
                }),
            )
            .on_action(
                cx.listener(|this, _: &RunGpuiTitlebarGitRemoteSync, _window, cx| {
                    this.update_main_window(cx, |app, _window, cx| {
                        app.dispatch_gpui_titlebar_git_action_selector(
                            GpuiTitlebarGitMenuActionId::SyncRemote.selector(),
                            cx,
                        );
                    });
                }),
            )
            .on_action(
                cx.listener(|this, _: &ConfigureGpuiTitlebarActions, _window, cx| {
                    this.update_main_window(cx, |app, window, cx| {
                        app.open_gpui_settings_actions_modal_from_titlebar(window, cx);
                    });
                }),
            )
            .on_action(cx.listener(
                |this, action: &RunGpuiTitlebarTipsHeaderAction, _window, cx| {
                    this.update_main_window(cx, |app, window, cx| {
                        app.run_gpui_titlebar_tips_header_action(
                            action.action_index as usize,
                            window,
                            cx,
                        );
                    });
                },
            ))
            .on_action(
                cx.listener(|this, action: &RunGpuiTitlebarTip, _window, cx| {
                    this.update_main_window(cx, |app, window, cx| {
                        app.run_gpui_titlebar_tip(action.tip_index as usize, window, cx);
                    });
                }),
            )
            .on_action(
                cx.listener(|this, action: &RunGpuiTitlebarHelpQuestion, window, cx| {
                    let question_index = action.question_index as usize;
                    this.update_main_window(cx, move |app, _main_window, cx| {
                        app.run_gpui_titlebar_help_question(question_index, cx);
                    });
                    this.close_from_popup_window(window, cx);
                }),
            )
            .on_action(cx.listener(
                |this, action: &FocusGpuiTitlebarResourceSession, _window, cx| {
                    let session_id = action.session_id.clone();
                    this.update_main_window(cx, move |app, _window, cx| {
                        let _ = app.focus_gpui_titlebar_resource_session(&session_id, cx);
                    });
                },
            ))
            .on_action(
                cx.listener(|this, action: &OpenGpuiTitlebarResourceUrl, _window, cx| {
                    let url = action.url.clone();
                    this.update_main_window(cx, move |app, window, cx| {
                        app.open_gpui_browser_action_url(url, window, cx);
                    });
                }),
            )
            .on_action(
                cx.listener(|this, _: &SleepInactiveSessionsFromTitlebar, _window, cx| {
                    this.update_main_window(cx, |app, _window, cx| {
                        let _ = app.dispatch_gpui_workspace_sleep_inactive_sessions(cx);
                    });
                }),
            )
            .on_action(
                cx.listener(|this, _: &RestartGpuiGxserverFromTitlebar, _window, cx| {
                    this.update_main_window(cx, |app, _window, cx| {
                        app.stop_gpui_local_gxserver_from_titlebar(true, cx);
                    });
                }),
            )
            .child(match &self.content {
                GpuiTitlebarPopupContent::AccountUsage(panel) => panel.clone().into_any_element(),
                GpuiTitlebarPopupContent::Menu(menu) => menu.clone().into_any_element(),
                GpuiTitlebarPopupContent::Reading(panel) => panel.clone().into_any_element(),
                GpuiTitlebarPopupContent::RemoteSites(panel) => panel.clone().into_any_element(),
            })
    }
}

pub(crate) enum GpuiTitlebarReadingPanelState {
    Tips {
        agent_hook_status: Option<serde_json::Value>,
        cli_status: Option<serde_json::Value>,
        live_agent_ids: HashSet<String>,
        read_ids: HashSet<String>,
        /// Built-in agent ids the sidebar launchers use; `None` while the HUD read is still pending.
        sidebar_agent_ids: Option<HashSet<String>>,
    },
    Resources {
        /// Clean RAM just copied its prompt; the button reads "Copied" until
        /// the reset timer clears this.
        clean_ram_copied: bool,
        expanded_keys: HashSet<String>,
        hovered_sections: HashSet<String>,
        info_open: bool,
        pending_actions: HashMap<String, &'static str>,
        snapshot: GpuiNativeResourcesSnapshot,
    },
    Notifications {
        feed: crate::notification_feed::GpuiNotificationFeedState,
        hovered_id: Option<String>,
    },
}

pub(crate) struct GpuiTitlebarReadingPanel {
    pub(super) main_app: gpui::WeakEntity<GhostexGpuiApp>,
    pub(super) scroll_handle: ScrollHandle,
    pub(super) state: GpuiTitlebarReadingPanelState,
}

#[derive(Clone, Copy)]
pub(crate) enum GpuiNativeTitlebarNoticeTarget {
    AgentHooks,
    DebuggingMode,
    GhostexCli,
}

pub(crate) struct GpuiNativeTitlebarNotice {
    body: String,
    target: GpuiNativeTitlebarNoticeTarget,
    title: String,
}

impl GpuiTitlebarReadingPanel {
    pub(crate) fn tips(
        main_app: gpui::WeakEntity<GhostexGpuiApp>,
        cli_status: Option<serde_json::Value>,
        agent_hook_status: Option<serde_json::Value>,
        live_agent_ids: HashSet<String>,
        sidebar_agent_ids: Option<HashSet<String>>,
    ) -> Self {
        Self {
            main_app,
            scroll_handle: ScrollHandle::new(),
            state: GpuiTitlebarReadingPanelState::Tips {
                agent_hook_status,
                cli_status,
                live_agent_ids,
                read_ids: gpui_titlebar_tips_read_ids_from_settings(),
                sidebar_agent_ids,
            },
        }
    }

    fn update_tips_sidebar_agent_ids(
        &mut self,
        next_sidebar_agent_ids: HashSet<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let GpuiTitlebarReadingPanelState::Tips {
            sidebar_agent_ids, ..
        } = &mut self.state
        else {
            return;
        };
        *sidebar_agent_ids = Some(next_sidebar_agent_ids);
        cx.notify();
    }

    fn update_tips_runtime_status(
        &mut self,
        payload: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let GpuiTitlebarReadingPanelState::Tips {
            agent_hook_status,
            cli_status,
            ..
        } = &mut self.state
        else {
            return;
        };
        match payload.get("type").and_then(serde_json::Value::as_str) {
            Some("ghostexCliStatus") => *cli_status = Some(payload),
            Some("agentHookStatus") => *agent_hook_status = Some(payload),
            _ => return,
        }
        cx.notify();
    }

    /// CDXC:Resources 2026-09-07 DECISION:
    /// User: closing and reopening Resources must collapse all rows. Each new panel records only explicit expansions, so hidden sections cannot shift the initial collapse indexes.
    pub(crate) fn resources(
        main_app: gpui::WeakEntity<GhostexGpuiApp>,
        snapshot: GpuiNativeResourcesSnapshot,
    ) -> Self {
        Self {
            main_app,
            scroll_handle: ScrollHandle::new(),
            state: GpuiTitlebarReadingPanelState::Resources {
                clean_ram_copied: false,
                expanded_keys: HashSet::new(),
                hovered_sections: HashSet::new(),
                info_open: false,
                pending_actions: HashMap::new(),
                snapshot,
            },
        }
    }

    pub(super) fn close_popup(&self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let _ = self.main_app.update_in(cx, |app, _main_window, cx| {
            app.clear_gpui_titlebar_popup_from_window(
                match self.state {
                    GpuiTitlebarReadingPanelState::Tips { .. } => GpuiTitlebarPopupKind::Tips,
                    GpuiTitlebarReadingPanelState::Resources { .. } => {
                        GpuiTitlebarPopupKind::Resources
                    }
                    GpuiTitlebarReadingPanelState::Notifications { .. } => {
                        GpuiTitlebarPopupKind::Notifications
                    }
                },
                cx,
            );
        });
        window.remove_window();
    }

    fn run_tip_header_action(
        &self,
        action_index: usize,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let _ = self.main_app.update_in(cx, |app, main_window, cx| {
            app.run_gpui_titlebar_tips_header_action(action_index, main_window, cx);
        });
        self.close_popup(window, cx);
    }

    fn open_tip_action(
        &mut self,
        tip_index: usize,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let _ = self.main_app.update_in(cx, |app, main_window, cx| {
            app.run_gpui_titlebar_tip(tip_index, main_window, cx);
        });
        self.close_popup(window, cx);
    }

    fn mark_tip_read(&mut self, tip_index: usize, cx: &mut gpui::Context<Self>) {
        let Some(tip) = GPUI_NATIVE_TITLEBAR_TIPS.get(tip_index) else {
            return;
        };
        gpui_mark_titlebar_tip_read(tip.id);
        if let GpuiTitlebarReadingPanelState::Tips { read_ids, .. } = &mut self.state {
            read_ids.insert(tip.id.to_string());
        }
        let _ = self.main_app.update_in(cx, |app, _window, cx| {
            app.titlebar_tips_unread_count = gpui_titlebar_tips_unread_count_from_settings();
            cx.notify();
        });
        cx.notify();
    }

    fn open_notice_settings(
        &self,
        target: GpuiNativeTitlebarNoticeTarget,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let _ = self.main_app.update_in(cx, |app, main_window, cx| {
            app.open_gpui_titlebar_notice_settings(target, main_window, cx);
        });
        self.close_popup(window, cx);
    }

    fn run_resource_secondary_action(
        &mut self,
        key: String,
        row: GpuiNativeResourceRow,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if matches!(row.action, GpuiNativeResourceAction::None) {
            return;
        }
        if let GpuiTitlebarReadingPanelState::Resources {
            pending_actions, ..
        } = &mut self.state
        {
            if pending_actions.contains_key(&key) {
                return;
            }
            pending_actions.insert(
                key,
                match row.action {
                    GpuiNativeResourceAction::Session => "Sleeping...",
                    GpuiNativeResourceAction::Server => "Stopping...",
                    _ => "Quitting...",
                },
            );
        }
        match row.action {
            GpuiNativeResourceAction::Session => {
                if let Some(session_id) = row.session_id {
                    let _ = self.main_app.update_in(cx, move |app, _window, cx| {
                        app.sleep_gpui_titlebar_resource_session(&session_id, cx);
                    });
                }
            }
            GpuiNativeResourceAction::Browser(tab_id) => {
                let _ = self.main_app.update_in(cx, move |app, main_window, cx| {
                    app.close_browser_tab_model(tab_id, main_window, cx);
                });
            }
            GpuiNativeResourceAction::Code => {
                let _ = self.main_app.update_in(cx, |app, _window, cx| {
                    app.sleep_titlebar_view(TitlebarMode::Source, cx);
                });
            }
            GpuiNativeResourceAction::Server => {
                gpui_terminate_native_resource_processes(row.termination_targets, "INT");
            }
            GpuiNativeResourceAction::Orphan => {
                gpui_terminate_native_resource_processes(row.termination_targets, "TERM");
            }
            GpuiNativeResourceAction::None => {}
        }
        // CDXC:Resources 2026-09-13 WHY:
        // This runs in a click handler, outside rendering. request_animation_frame reads GPUI's empty rendered-view stack and aborts the app after stopping Code (or another resource).
        cx.notify();
    }
}

impl Render for GpuiTitlebarReadingPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        resource_panel_frame().child(match self.state {
            GpuiTitlebarReadingPanelState::Tips { .. } => self.render_tips(cx),
            GpuiTitlebarReadingPanelState::Resources { .. } => self.render_resources(cx),
            GpuiTitlebarReadingPanelState::Notifications { .. } => self.render_notifications(cx),
        })
    }
}

pub(crate) fn format_gpui_resource_cpu_compact(cpu: f64) -> String {
    format!("{:.0}%", cpu.max(0.0).trunc())
}

pub(crate) fn format_gpui_resource_memory_compact(memory_mb: f64) -> String {
    let memory_mb = memory_mb.max(0.0);
    if memory_mb >= 1024.0 {
        let gb = (memory_mb / 1024.0 * 10.0).round() / 10.0;
        if gb.fract() == 0.0 {
            format!("{gb:.0} GB")
        } else {
            format!("{gb:.1} GB")
        }
    } else {
        format!("{memory_mb:.0} MB")
    }
}

pub(crate) fn resource_metric_chip(icon: &'static str, label: String, width: f32) -> AnyElement {
    resource_metric(width)
        .child(titlebar_svg_icon(
            icon,
            12.0,
            rgb(0xffffff).opacity(0.62).into(),
        ))
        .child(label)
        .into_any_element()
}

pub(crate) struct GpuiTitlebarTipsPanel {
    pub(crate) surface: Entity<CefSurface>,
}

impl GpuiTitlebarTipsPanel {
    pub(crate) fn new(
        parent_ns_view: *mut std::ffi::c_void,
        url: String,
        event_handler: cef::AppModalHostBridgeEventHandler,
        cx: &mut gpui::Context<GhostexGpuiApp>,
    ) -> Result<Entity<Self>, String> {
        /*
        CDXC:Onboarding 2026-06-24-23:17:
        The GPUI Tips dropdown reuses the production React titlebar-host panel in a CEF surface owned by the app's anchored GPUI overlay. This keeps the content source aligned with macOS while avoiding AppKit/Swift dropdown windows, duplicated GPUI tips data, transparent overlays, hidden hit regions, and broad native hit-test routing.
        */
        let surface = CefSurface::try_new(
            TITLEBAR_TIPS_PANEL_ID.to_string(),
            parent_ns_view,
            url,
            TITLEBAR_TIPS_PANEL_CEF_PROFILE_ID.to_string(),
            CEF_DARK_PREPAINT_BACKGROUND_COLOR,
            false,
            titlebar_popup_menu_background(),
            None,
            true,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(cef::AppModalHostBridgeSurface::Titlebar),
            Some(event_handler),
            None,
            cx,
        )?;
        Ok(cx.new(move |_cx| Self { surface }))
    }

    pub(crate) fn set_visible(&mut self, visible: bool, cx: &mut gpui::Context<Self>) {
        self.surface.update(cx, |surface, _| {
            if visible {
                // Terminal host views appended since this reused panel was
                // created would otherwise sit above the dropdown.
                surface.order_front();
            }
            surface.set_visible(visible);
        });
    }

    pub(crate) fn dispatch_project_state_update(
        &mut self,
        project_state_update: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let script = format!(
            "(function(){{const update = {};const titlebar = window.__ghostex_TITLEBAR__;if (titlebar && typeof titlebar.setActiveProjectState === 'function'){{titlebar.setActiveProjectState(update);}}else{{window.__ghostex_PENDING_TITLEBAR_PROJECT_STATE__ = Object.assign({{}}, window.__ghostex_PENDING_TITLEBAR_PROJECT_STATE__ || {{}}, update);}}}})(); undefined;",
            project_state_update
        );
        self.surface.update(cx, |surface, _| {
            surface.execute_app_owned_script(&script);
        });
    }

    pub(crate) fn dispatch_native_host_event(
        &mut self,
        event: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let script = format!(
            "window.dispatchEvent(new CustomEvent('ghostex-native-host-event', {{ detail: {} }})); undefined;",
            event
        );
        self.surface.update(cx, |surface, _| {
            surface.execute_app_owned_script(&script);
        });
    }

    pub(crate) fn install_unread_count_probe(&mut self, cx: &mut gpui::Context<Self>) {
        let tip_ids = serde_json::to_string(TITLEBAR_TIP_IDS).expect("titlebar tip ids serialize");
        let storage_key = serde_json::to_string(TITLEBAR_TIPS_READ_STORAGE_KEY)
            .expect("titlebar tips storage key serializes");
        let script = format!(
            "(function(){{const tipIds={tip_ids};const storageKey={storage_key};const post=()=>{{let readIds=[];try{{const parsed=JSON.parse(localStorage.getItem(storageKey)||'[]');if(Array.isArray(parsed)){{readIds=parsed.filter((id)=>typeof id==='string'&&id.length>0);}}}}catch(_error){{readIds=[];}}const readSet=new Set(readIds);const unreadCount=tipIds.filter((id)=>!readSet.has(id)).length;const bridge=window.webkit&&window.webkit.messageHandlers&&window.webkit.messageHandlers.ghostexAppModalHost;if(bridge&&typeof bridge.postMessage==='function'){{bridge.postMessage({{type:'gpuiTitlebarTipsUnreadCount',unreadCount}});}}}};if(!window.__ghostexGpuiTitlebarTipsUnreadProbeInstalled){{window.__ghostexGpuiTitlebarTipsUnreadProbeInstalled=true;window.setInterval(post,750);window.addEventListener('storage',post);}}post();}})(); undefined;"
        );
        self.surface.update(cx, |surface, _| {
            surface.execute_app_owned_script(&script);
        });
    }
}

impl Render for GpuiTitlebarTipsPanel {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // Fill the anchored dropdown's content box so the native CEF child
        // view stays inset within the container's 1px border.
        div()
            .size_full()
            .overflow_hidden()
            .bg(titlebar_popup_menu_background())
            .child(self.surface.clone())
    }
}

pub(crate) struct GpuiTitlebarResourcesPanel {
    surface: Entity<CefSurface>,
}

impl GpuiTitlebarResourcesPanel {
    pub(crate) fn create_browser(
        parent_ns_view: *mut std::ffi::c_void,
        url: String,
        event_handler: cef::AppModalHostBridgeEventHandler,
    ) -> Result<Rc<CefBrowser>, String> {
        /*
        CDXC:Resources 2026-07-08:
        The Resources dropdown is the production React titlebar-host resources
        panel inside a CEF child view owned by the anchored GPUI overlay. It is
        created hidden, revealed only after the React ready event, and dropped
        on close so renderer polling and the CEF browser lifecycle stop together.
        */
        let browser = Rc::new(CefBrowser::new(
            parent_ns_view,
            &url,
            TITLEBAR_RESOURCES_PANEL_CEF_PROFILE_ID,
            CEF_DARK_PREPAINT_BACKGROUND_COLOR,
            false,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(cef::AppModalHostBridgeSurface::Titlebar),
            Some(event_handler),
            None,
            None,
            None,
        )?);
        browser.set_visible(false);
        Ok(browser)
    }

    pub(crate) fn from_browser(
        browser: Rc<CefBrowser>,
        cx: &mut gpui::Context<GhostexGpuiApp>,
    ) -> Entity<Self> {
        let surface = cx.new(move |cx| {
            CefSurface::from_browser(
                TITLEBAR_RESOURCES_PANEL_ID.to_string(),
                titlebar_popup_menu_background(),
                false,
                browser,
                cx,
            )
        });
        cx.new(move |_cx| Self { surface })
    }

    pub(crate) fn set_visible(&mut self, visible: bool, cx: &mut gpui::Context<Self>) {
        self.surface.update(cx, |surface, _| {
            if visible {
                // Terminal host views appended since this reused panel was
                // created would otherwise sit above the dropdown.
                surface.order_front();
            }
            surface.set_visible(visible);
        });
    }

    pub(crate) fn browser(&mut self, cx: &mut gpui::Context<Self>) -> Rc<CefBrowser> {
        self.surface.update(cx, |surface, _| surface.browser())
    }

    pub(crate) fn dispatch_native_host_event(
        &mut self,
        event: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let script = format!(
            "window.dispatchEvent(new CustomEvent('ghostex-native-host-event', {{ detail: {} }})); undefined;",
            event
        );
        self.surface.update(cx, |surface, _| {
            surface.execute_app_owned_script(&script);
        });
    }
}

impl Render for GpuiTitlebarResourcesPanel {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // Fill the anchored dropdown's content box so the native CEF child
        // view stays inset within the container's 1px border.
        div()
            .size_full()
            .overflow_hidden()
            .bg(titlebar_popup_menu_background())
            .child(self.surface.clone())
    }
}

pub(crate) fn gpui_titlebar_resources_project_state_update_script(
    project_state_update: serde_json::Value,
) -> String {
    format!(
        "(function(){{window.__ghostex_NATIVE_HOST__=Object.assign({{}},window.__ghostex_NATIVE_HOST__||{{}});window.__ghostex_NATIVE_HOST__.codeServerRuntime=Object.assign({{}},window.__ghostex_NATIVE_HOST__.codeServerRuntime||{{}});window.__ghostex_NATIVE_HOST__.codeServerRuntime.port={};const update={};const titlebar=window.__ghostex_TITLEBAR__;if(titlebar&&typeof titlebar.setActiveProjectState==='function'){{titlebar.setActiveProjectState(update);}}else{{window.__ghostex_PENDING_TITLEBAR_PROJECT_STATE__=Object.assign({{}},window.__ghostex_PENDING_TITLEBAR_PROJECT_STATE__||{{}},update);}}}})(); undefined;",
        source_code_server_editor_port(),
        project_state_update
    )
}

pub(crate) fn gpui_titlebar_resources_dispatch_project_state_update(
    cx: &mut gpui::Context<GhostexGpuiApp>,
    browser: Rc<CefBrowser>,
    project_state_update: serde_json::Value,
) {
    let foreground = cx.foreground_executor().clone();
    foreground
        .spawn(async move {
            gpui_titlebar_resources_dispatch_project_state_update_to_browser(
                browser,
                project_state_update,
            );
        })
        .detach();
}

pub(crate) fn gpui_titlebar_resources_dispatch_project_state_update_to_browser(
    browser: Rc<CefBrowser>,
    project_state_update: serde_json::Value,
) {
    let script = gpui_titlebar_resources_project_state_update_script(project_state_update);
    browser.execute_java_script_in_main_frame(&script);
}

#[derive(Clone, Debug)]
pub(crate) struct GpuiTitlebarNativeProcessRequest {
    request_id: String,
    executable: String,
    args: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct GpuiTitlebarNativeProcessResult {
    pub(crate) request_id: String,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) exit_code: i32,
}

impl GpuiTitlebarNativeProcessResult {
    pub(crate) fn rejected(request_id: String, error: String) -> Self {
        Self {
            request_id,
            stdout: String::new(),
            stderr: error,
            exit_code: GPUI_TITLEBAR_NATIVE_PROCESS_REJECTED_EXIT_CODE,
        }
    }
}

pub(crate) fn gpui_titlebar_native_process_request_from_message(
    message: &serde_json::Value,
) -> std::result::Result<GpuiTitlebarNativeProcessRequest, String> {
    if message.get("cwd").is_some() || message.get("env").is_some() {
        return Err("Rejected titlebar native process request: cwd/env are not supported.".into());
    }

    let request_id = message
        .get("requestId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|request_id| !request_id.is_empty())
        .filter(|request_id| {
            request_id.chars().count() <= GPUI_TITLEBAR_NATIVE_PROCESS_REQUEST_ID_MAX_CHARS
        })
        .ok_or_else(|| "Rejected titlebar native process request: invalid request id.".to_string())?
        .to_string();
    let executable = message
        .get("executable")
        .and_then(serde_json::Value::as_str)
        .filter(|executable| !executable.trim().is_empty())
        .ok_or_else(|| "Rejected titlebar native process request: invalid executable.".to_string())?
        .to_string();
    let args = message
        .get("args")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "Rejected titlebar native process request: invalid args.".to_string())?
        .iter()
        .map(|arg| {
            arg.as_str().map(str::to_string).ok_or_else(|| {
                "Rejected titlebar native process request: args must be strings.".to_string()
            })
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;

    /*
    CDXC:Titlebar 2026-07-08:
    The GPUI `ghostexNativeHost` runProcess bridge executes only the fixed ps/lsof/kill shapes issued by the shared Resources titlebar panel. Reject every other executable, cwd/env, signal, PID, and argument layout before spawning so first-party React cannot become an arbitrary process runner.
    */
    if !gpui_titlebar_native_process_request_is_allowed(&executable, &args) {
        return Err(
            "Rejected titlebar native process request: executable/arguments are not allowlisted."
                .into(),
        );
    }

    Ok(GpuiTitlebarNativeProcessRequest {
        request_id,
        executable,
        args,
    })
}

pub(crate) fn gpui_titlebar_native_process_request_is_allowed(
    executable: &str,
    args: &[String],
) -> bool {
    match executable {
        "/bin/ps" => gpui_titlebar_native_process_ps_args_are_allowed(args),
        "/usr/sbin/lsof" => gpui_titlebar_native_process_lsof_args_are_allowed(args),
        "/bin/kill" => gpui_titlebar_native_process_kill_args_are_allowed(args),
        _ => false,
    }
}

pub(crate) fn gpui_titlebar_native_process_ps_args_are_allowed(args: &[String]) -> bool {
    (args.len() == 2 && args[0] == "-axo" && args[1] == "pid=,ppid=,pcpu=,rss=,command=")
        || (args.len() == 4
            && args[0] == "-o"
            && args[1] == "command="
            && args[2] == "-p"
            && gpui_titlebar_native_process_arg_is_numeric_pid(&args[3]))
}

pub(crate) fn gpui_titlebar_native_process_lsof_args_are_allowed(args: &[String]) -> bool {
    (args.len() == 5
        && args[0] == "-nP"
        && args[1] == "-iTCP"
        && args[2] == "-sTCP:LISTEN"
        && args[3] == "-F"
        && args[4] == "pcn")
        || (args.len() == 8
            && args[0] == "-nP"
            && args[1] == "-a"
            && args[2] == "-d"
            && args[3] == "cwd"
            && args[4] == "-F"
            && args[5] == "pn"
            && args[6] == "-p"
            && gpui_titlebar_native_process_arg_is_pid_csv(&args[7]))
}

pub(crate) fn gpui_titlebar_native_process_kill_args_are_allowed(args: &[String]) -> bool {
    let Some((signal, pids)) = args.split_first() else {
        return false;
    };
    matches!(signal.as_str(), "-INT" | "-TERM" | "-KILL")
        && !pids.is_empty()
        && pids
            .iter()
            .all(|pid| gpui_titlebar_native_process_arg_is_numeric_pid(pid))
}

pub(crate) fn gpui_titlebar_native_process_arg_is_numeric_pid(value: &str) -> bool {
    if value.is_empty() || value.trim() != value || !value.chars().all(|ch| ch.is_ascii_digit()) {
        return false;
    }
    match value.parse::<u32>() {
        Ok(pid) => pid > 0,
        Err(_) => false,
    }
}

pub(crate) fn gpui_titlebar_native_process_arg_is_pid_csv(value: &str) -> bool {
    let mut saw_pid = false;
    for pid in value.split(',') {
        if !gpui_titlebar_native_process_arg_is_numeric_pid(pid) {
            return false;
        }
        saw_pid = true;
    }
    saw_pid
}

pub(crate) fn gpui_run_titlebar_native_process(
    request: GpuiTitlebarNativeProcessRequest,
) -> GpuiTitlebarNativeProcessResult {
    let output = Command::new(&request.executable)
        .args(&request.args)
        .stdin(Stdio::null())
        .output();
    match output {
        Ok(output) => GpuiTitlebarNativeProcessResult {
            request_id: request.request_id,
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        },
        Err(error) => GpuiTitlebarNativeProcessResult {
            request_id: request.request_id,
            stdout: String::new(),
            stderr: error.to_string(),
            exit_code: -1,
        },
    }
}
