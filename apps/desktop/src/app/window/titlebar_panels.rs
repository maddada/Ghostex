mod resources;
mod tips;

// C1 wave-2 extraction: the titlebar popup/reading/tips/resources panel window entities moved verbatim out of main.rs (pure
// move, no logic changes; items made pub(crate) so main.rs and sibling
// modules can still reach them). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use super::resources_style::*;
use crate::app::helpers::*;
use crate::app::titlebar::resources_clean_ram_prompt::gpui_resources_clean_ram_prompt;
use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiTitlebarPopupKind {
    AccountUsage(ExtensionId),
    Actions,
    BrowserActions(BrowserPaneId),
    ContextMenu,
    Extensions,
    Git,
    Help,
    More,
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
            Self::More => "more",
            Self::Notifications => "notifications",
            Self::OpenTargets => "openTargets",
            Self::Resources => "resources",
            Self::RemoteSites => "remoteSites",
            Self::Tips => "tips",
        }
    }
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
        // CDXC:Titlebar 2026-09-23 WHY:
        // Borderless Windows popups still need a native title so accessibility tools can identify their own window without activating the main window behind them.
        #[cfg(target_os = "windows")]
        window.set_window_title(match kind {
            GpuiTitlebarPopupKind::AccountUsage(_) => "Ghostex Account Usage",
            GpuiTitlebarPopupKind::Actions => "Ghostex Start",
            GpuiTitlebarPopupKind::BrowserActions(_) => "Ghostex Browser Actions",
            GpuiTitlebarPopupKind::ContextMenu => "Ghostex Menu",
            GpuiTitlebarPopupKind::Extensions => "Ghostex Extensions",
            GpuiTitlebarPopupKind::Git => "Ghostex Commit",
            GpuiTitlebarPopupKind::Help => "Ghostex Help",
            GpuiTitlebarPopupKind::More => "Ghostex More",
            GpuiTitlebarPopupKind::Notifications => "Ghostex Notifications",
            GpuiTitlebarPopupKind::OpenTargets => "Ghostex Open",
            GpuiTitlebarPopupKind::Resources => "Ghostex Resources",
            GpuiTitlebarPopupKind::RemoteSites => "Ghostex Remote Sites",
            GpuiTitlebarPopupKind::Tips => "Ghostex Tips",
        });
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
                _dismiss_subscription: dismiss_subscription,
            }
        })
    }

    pub(crate) fn replace_menu(
        &mut self,
        menu: Entity<PopupMenu>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        menu.focus_handle(cx).focus(window, cx);
        self._dismiss_subscription = Some(cx.subscribe_in(
            &menu,
            window,
            |this, _, _: &DismissEvent, window, cx| this.close_from_popup_window(window, cx),
        ));
        self.content = GpuiTitlebarPopupContent::Menu(menu);
        cx.notify();
    }

    fn close_from_popup_window(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let kind = self.kind;
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
}

impl Render for GpuiTitlebarPopupWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _profile = crate::profiling::span(crate::profiling::Metric::PopupRender);
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
            /*
            The ⋯ window closes first and the row's own panel opens on the next
            effect cycle: opening it inline would make the main app close this
            window while this window is mid-update.
            */
            .on_action(
                cx.listener(|this, action: &OpenGpuiTitlebarMoreMenuItem, window, cx| {
                    let Some(item) =
                        crate::app::titlebar::more_menu::GpuiTitlebarMoreMenuItem::from_index(
                            action.item_index,
                        )
                    else {
                        return;
                    };
                    this.update_main_window(cx, move |_, main_window, cx| {
                        cx.defer_in(main_window, move |app, window, cx| {
                            app.open_titlebar_more_menu_item(item, window, cx);
                        });
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
                        gpui_copy_to_clipboard(ClipboardItem::new_string(branch), cx);
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

    pub(crate) fn update_tips_sidebar_agent_ids(
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

    pub(crate) fn update_tips_runtime_status(
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
            chrome_ink().opacity(0.62).into(),
        ))
        .child(label)
        .into_any_element()
}
