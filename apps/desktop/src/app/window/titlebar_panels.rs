mod resources;

// C1 wave-2 extraction: the titlebar popup/reading/tips/resources panel window entities moved verbatim out of main.rs (pure
// move, no logic changes; items made pub(crate) so main.rs and sibling
// modules can still reach them). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use super::resources_style::*;
use crate::app::helpers::*;
use crate::app::titlebar::actions_menu_rows::{StartMenuPick, on_start_menu_picks};
use crate::app::titlebar::resources_clean_ram_prompt::gpui_resources_clean_ram_prompt;
use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiTitlebarPopupKind {
    AccountUsage(ExtensionId),
    Actions,
    BrowserActions(BrowserPaneId),
    BrowserDeviceView(BrowserPaneId),
    /// The chat header's Share popover (Ghostex Collab, window/collab_share.rs).
    CollabShare,
    ContextMenu,
    /// The header's jobs button's popover (window/jobs_popover.rs).
    Jobs,
    Extensions,
    Git,
    Help,
    Notifications,
    OpenTargets,
    Resources,
    RemoteSites,
    SessionLinks,
    Updates,
}

impl GpuiTitlebarPopupKind {
    pub(crate) fn diagnostic_label(self) -> &'static str {
        match self {
            Self::AccountUsage(_) => "accountUsage",
            Self::Actions => "actions",
            Self::BrowserActions(_) => "browserActions",
            Self::BrowserDeviceView(_) => "browserDeviceView",
            Self::CollabShare => "collabShare",
            Self::Jobs => "jobs",
            Self::ContextMenu => "contextMenu",
            Self::Extensions => "extensions",
            Self::Git => "git",
            Self::Help => "help",
            Self::Notifications => "notifications",
            Self::OpenTargets => "openTargets",
            Self::Resources => "resources",
            Self::RemoteSites => "remoteSites",
            Self::SessionLinks => "sessionLinks",
            Self::Updates => "updates",
        }
    }
}

pub(crate) struct GpuiTitlebarPopupState {
    pub(crate) kind: GpuiTitlebarPopupKind,
    pub(crate) trigger_bounds: Bounds<Pixels>,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct GpuiTitlebarPopupAnchorState {
    pub(crate) bounds: Bounds<Pixels>,
    pub(crate) trigger_bounds_captured: bool,
}

#[derive(Clone)]
pub(crate) enum GpuiTitlebarPopupContent {
    AccountUsage(Entity<super::account_usage::AccountUsagePanel>),
    Menu(Entity<PopupMenu>),
    Reading(Entity<GpuiTitlebarReadingPanel>),
    RemoteSites(Entity<crate::app::window::remote_sites::RemoteSitesPanel>),
    CollabShare(Entity<crate::app::window::collab_share::CollabSharePanel>),
    Jobs(Entity<crate::app::window::jobs_popover::JobsPopoverPanel>),
    SessionLinks(Entity<crate::app::native_sidebar::links_list::SessionLinksList>),
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
            GpuiTitlebarPopupKind::BrowserDeviceView(_) => "Ghostex Browser Page Size",
            GpuiTitlebarPopupKind::CollabShare => "Ghostex Collab",
            GpuiTitlebarPopupKind::Jobs => "Ghostex Jobs",
            GpuiTitlebarPopupKind::ContextMenu => "Ghostex Menu",
            GpuiTitlebarPopupKind::Extensions => "Ghostex Extensions",
            GpuiTitlebarPopupKind::Git => "Ghostex Commit",
            GpuiTitlebarPopupKind::Help => "Ghostex Help",
            GpuiTitlebarPopupKind::Notifications => "Ghostex Notifications",
            GpuiTitlebarPopupKind::OpenTargets => "Ghostex Open",
            GpuiTitlebarPopupKind::Resources => "Ghostex Resources",
            GpuiTitlebarPopupKind::RemoteSites => "Ghostex Remote Sites",
            GpuiTitlebarPopupKind::SessionLinks => "Ghostex Session Links",
            GpuiTitlebarPopupKind::Updates => "Ghostex Updates",
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
        // A failed update's reason is often taller than the Updates list's rows, so its tooltip
        // draws in the tooltip host window rather than cut off inside the list (frosted_host.rs).
        if kind == GpuiTitlebarPopupKind::Updates {
            crate::app::window::frosted_host::host_window_tooltips_always(window, cx);
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
                GpuiTitlebarPopupContent::CollabShare(_) => None,
                GpuiTitlebarPopupContent::Jobs(_) => None,
                GpuiTitlebarPopupContent::SessionLinks(_) => None,
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
                        window.dispatch_action(Box::new(*action), cx);
                    });
                },
            ))
            .on_action(
                cx.listener(|this, action: &SetBrowserDeviceView, _window, cx| {
                    this.update_main_window(cx, |_, window, cx| {
                        window.dispatch_action(Box::new(*action), cx);
                    });
                }),
            )
            .on_action(
                cx.listener(|this, action: &SetBrowserPageAppearance, _window, cx| {
                    this.update_main_window(cx, |_, window, cx| {
                        window.dispatch_action(Box::new(*action), cx);
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
            // The Start menu's rows (project Actions, Global Actions, Configure), registered by the
            // menu's own file so no row can be left out (actions_menu_rows.rs).
            .map(|this| {
                on_start_menu_picks(
                    this,
                    cx.listener(|this, pick: &StartMenuPick, _window, cx| {
                        let pick = pick.clone();
                        this.update_main_window(cx, move |app, window, cx| {
                            app.run_start_menu_pick(pick, window, cx);
                        });
                    }),
                )
            })
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
                cx.listener(|this, action: &RunGpuiTitlebarUpdates, window, cx| {
                    let update_all = action.update_all;
                    this.update_main_window(cx, move |app, main_window, cx| {
                        app.run_gpui_titlebar_updates(update_all, Some(main_window), cx);
                    });
                    // Update all keeps the list open so its rows show their spinners.
                    if !update_all {
                        this.close_from_popup_window(window, cx);
                    }
                }),
            )
            .on_action(
                cx.listener(|this, action: &RunGpuiTitlebarUpdateItem, _window, cx| {
                    let id = action.id.clone();
                    this.update_main_window(cx, move |app, main_window, cx| {
                        app.run_gpui_titlebar_update_item(&id, main_window, cx);
                    });
                }),
            )
            .on_action(cx.listener(
                |this, action: &DismissGpuiTitlebarUpdateFailure, _window, cx| {
                    let (id, error) = (action.id.clone(), action.error.clone());
                    this.update_main_window(cx, move |app, _main_window, cx| {
                        app.dismiss_update_failure(&id, &error, cx);
                    });
                },
            ))
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
                GpuiTitlebarPopupContent::CollabShare(panel) => panel.clone().into_any_element(),
                GpuiTitlebarPopupContent::Jobs(panel) => panel.clone().into_any_element(),
                GpuiTitlebarPopupContent::SessionLinks(panel) => panel.clone().into_any_element(),
            })
    }
}

pub(crate) enum GpuiTitlebarReadingPanelState {
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
        /// The hovered row's key (`GpuiNotificationFeedItem::key`), unique across computers.
        hovered_id: Option<String>,
        /// The header's "..." menu (Mark all read, Clear all) is open; closed each time the panel opens.
        actions_menu_open: bool,
    },
}

pub(crate) struct GpuiTitlebarReadingPanel {
    pub(super) main_app: gpui::WeakEntity<GhostexGpuiApp>,
    pub(super) scroll_handle: ScrollHandle,
    pub(super) state: GpuiTitlebarReadingPanelState,
}

impl GpuiTitlebarReadingPanel {
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
        match self.state {
            GpuiTitlebarReadingPanelState::Resources { .. } => {
                resource_panel_frame().child(self.render_resources(cx))
            }
            GpuiTitlebarReadingPanelState::Notifications { .. } => {
                return super::popup_frame::popup_open_frame(
                    resource_panel_frame(),
                    self.render_notifications(cx),
                    false,
                    cx.reduce_motion(),
                );
            }
        }
        .into_any_element()
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
