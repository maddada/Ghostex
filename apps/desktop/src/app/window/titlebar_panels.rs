// C1 wave-2 extraction: the titlebar popup/reading/tips/resources panel window entities moved verbatim out of main.rs (pure
// move, no logic changes; items made pub(crate) so main.rs and sibling
// modules can still reach them). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use crate::app::helpers::*;
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
    Actions,
    Git,
    OpenTargets,
    Resources,
    Tips,
}

impl GpuiTitlebarPopupKind {
    pub(crate) fn diagnostic_label(self) -> &'static str {
        match self {
            Self::Actions => "actions",
            Self::Git => "git",
            Self::OpenTargets => "openTargets",
            Self::Resources => "resources",
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
    Menu(Entity<PopupMenu>),
    Reading(Entity<GpuiTitlebarReadingPanel>),
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
            GpuiTitlebarPopupContent::Menu(_) => "menu",
            GpuiTitlebarPopupContent::Reading(_) => "reading",
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
}

impl Render for GpuiTitlebarPopupWindow {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
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
                this.close_from_popup_window(window, cx);
            }))
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
                cx.listener(|this, _: &OpenGpuiDaemonSessionsModal, _window, cx| {
                    this.update_main_window(cx, |app, window, cx| {
                        app.open_gpui_app_modal_from_titlebar(
                            GpuiAppModalKind::DaemonSessions,
                            window,
                            cx,
                        );
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
                GpuiTitlebarPopupContent::Menu(menu) => menu.clone().into_any_element(),
                GpuiTitlebarPopupContent::Reading(panel) => panel.clone().into_any_element(),
            })
    }
}

pub(crate) enum GpuiTitlebarReadingPanelState {
    Tips {
        agent_hook_status: Option<serde_json::Value>,
        cli_status: Option<serde_json::Value>,
        live_agent_ids: HashSet<String>,
        read_ids: HashSet<String>,
    },
    Resources {
        collapsed_keys: HashSet<String>,
        hovered_sections: HashSet<String>,
        info_open: bool,
        quitting_keys: HashSet<String>,
        snapshot: GpuiNativeResourcesSnapshot,
    },
}

pub(crate) struct GpuiTitlebarReadingPanel {
    main_app: gpui::WeakEntity<GhostexGpuiApp>,
    scroll_handle: ScrollHandle,
    state: GpuiTitlebarReadingPanelState,
}

#[derive(Clone, Copy)]
pub(crate) enum GpuiNativeTitlebarNoticeTarget {
    AgentHooks,
    DebuggingMode,
    GhostexCli,
    SessionPersistence,
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
    ) -> Self {
        Self {
            main_app,
            scroll_handle: ScrollHandle::new(),
            state: GpuiTitlebarReadingPanelState::Tips {
                agent_hook_status,
                cli_status,
                live_agent_ids,
                read_ids: gpui_titlebar_tips_read_ids_from_settings(),
            },
        }
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

    pub(crate) fn resources(
        main_app: gpui::WeakEntity<GhostexGpuiApp>,
        snapshot: GpuiNativeResourcesSnapshot,
    ) -> Self {
        let collapsed_keys = snapshot
            .server_rows
            .iter()
            .chain(snapshot.session_rows.iter())
            .chain(snapshot.code_rows.iter())
            .chain(snapshot.browser_rows.iter())
            .chain(snapshot.orphan_rows.iter())
            .enumerate()
            .filter(|(_, row)| !row.children.is_empty())
            .map(|(index, _)| format!("resource-{index}"))
            .collect();
        Self {
            main_app,
            scroll_handle: ScrollHandle::new(),
            state: GpuiTitlebarReadingPanelState::Resources {
                collapsed_keys,
                hovered_sections: HashSet::new(),
                info_open: false,
                quitting_keys: HashSet::new(),
                snapshot,
            },
        }
    }

    fn close_popup(&self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let _ = self.main_app.update_in(cx, |app, _main_window, cx| {
            app.clear_gpui_titlebar_popup_from_window(
                match self.state {
                    GpuiTitlebarReadingPanelState::Tips { .. } => GpuiTitlebarPopupKind::Tips,
                    GpuiTitlebarReadingPanelState::Resources { .. } => {
                        GpuiTitlebarPopupKind::Resources
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
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if matches!(row.action, GpuiNativeResourceAction::None) {
            return;
        }
        if let GpuiTitlebarReadingPanelState::Resources { quitting_keys, .. } = &mut self.state {
            quitting_keys.insert(key);
        }
        match row.action {
            GpuiNativeResourceAction::Session => {
                if let Some(session_id) = row.session_id {
                    let _ = self.main_app.update_in(cx, move |app, _window, cx| {
                        let Some(shell_session_id) =
                            app.gpui_titlebar_resource_shell_session_id(&session_id)
                        else {
                            return;
                        };
                        let Some(pane_id) =
                            app.agents_workspace.pane_id_for_session(shell_session_id)
                        else {
                            return;
                        };
                        app.sleep_agents_tabs_for_scope(
                            pane_id,
                            shell_session_id,
                            AgentsWorkspaceTabSleepScope::Sleep,
                            cx,
                        );
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
                    if app.stop_source_code_server_runtime(cx) {
                        app.project_editor_shell
                            .mark_mode_sleeping(TitlebarMode::Source);
                        app.refresh_project_workarea_runtime_cef_surfaces_from_runtime_state(cx);
                        app.persist_shell_layout_state();
                    }
                });
            }
            GpuiNativeResourceAction::Server => {
                gpui_terminate_native_resource_pids(row.pids, "INT");
            }
            GpuiNativeResourceAction::Orphan => {
                gpui_terminate_native_resource_pids(row.pids, "TERM");
            }
            GpuiNativeResourceAction::None => {}
        }
        window.request_animation_frame();
        cx.notify();
    }

    fn render_tips_header(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let actions = [
            ("Docs", "titlebar/book.svg"),
            ("Video", "titlebar/star-filled.svg"),
            ("Setup", "titlebar/tool.svg"),
            ("Updates", "titlebar/history.svg"),
        ];
        h_flex()
            .h(px(TITLEBAR_POPUP_READING_HEADER_HEIGHT))
            .flex_shrink_0()
            .items_stretch()
            .border_b_1()
            .border_color(rgb(0xffffff).opacity(0.12))
            .child(
                h_flex()
                    .min_w_0()
                    .flex_1()
                    .items_center()
                    .gap(px(8.0))
                    .pl(px(12.0))
                    .text_size(px(14.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(0xffffff).opacity(0.96))
                    .child(titlebar_svg_icon(
                        TITLEBAR_ICON_INFO,
                        18.0,
                        rgb(0xffffff).opacity(0.96).into(),
                    ))
                    .child("Tips"),
            )
            .children(
                actions
                    .into_iter()
                    .enumerate()
                    .map(|(action_index, (label, icon))| {
                        h_flex()
                            .id(format!("gpui-titlebar-tips-header-action-{action_index}"))
                            .h_full()
                            .w(px(99.4))
                            .flex_shrink_0()
                            .items_center()
                            .justify_center()
                            .gap(px(6.0))
                            .border_l_1()
                            .border_color(rgb(0xffffff).opacity(0.12))
                            .px(px(15.0))
                            .text_size(px(TITLEBAR_POPUP_READING_HEADER_BUTTON_TEXT_SIZE))
                            .font_weight(FontWeight::NORMAL)
                            .text_color(rgb(0xffffff).opacity(0.78))
                            .cursor_pointer()
                            .hover(|this| {
                                this.bg(rgb(0xffffff).opacity(0.14))
                                    .text_color(rgb(0xffffff).opacity(0.94))
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                                    window.prevent_default();
                                    cx.stop_propagation();
                                    this.run_tip_header_action(action_index, window, cx);
                                }),
                            )
                            .child(titlebar_svg_icon(
                                icon,
                                TITLEBAR_POPUP_READING_HEADER_BUTTON_ICON_SIZE,
                                rgb(0xffffff).opacity(0.78).into(),
                            ))
                            .child(label)
                    }),
            )
            .into_any_element()
    }

    fn render_tips_section_heading(title: &'static str) -> AnyElement {
        h_flex()
            .h(px(24.0))
            .items_center()
            .px(px(2.0))
            .pt(px(4.0))
            .pb(px(7.0))
            .text_size(px(11.0))
            .font_weight(FontWeight::BOLD)
            .text_color(rgb(0xffffff).opacity(0.62))
            .child(title)
            .into_any_element()
    }

    fn render_tip_row(
        &self,
        tip_index: usize,
        read: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let tip = GPUI_NATIVE_TITLEBAR_TIPS[tip_index];
        let actionable = matches!(
            tip.id,
            "use-ghostex-computer-use-skill"
                | "use-ghostex-browser-use-skill"
                | "use-ghostex-embedded-browser-use-skill"
                | "use-ghostex-auto-rename-session-skill"
                | "recommend-faster-chrome-devtools-skill"
        );
        let detail = h_flex()
            .id(format!("gpui-titlebar-tip-detail-{tip_index}"))
            .min_w_0()
            .flex_1()
            .items_start()
            .gap(px(10.0))
            .when(actionable, |this| {
                this.cursor_pointer().on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.open_tip_action(tip_index, window, cx);
                    }),
                )
            })
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .size(px(28.0))
                    .items_center()
                    .justify_center()
                    .bg(rgb(0xffffff).opacity(0.10))
                    .child(titlebar_svg_icon(
                        tip.icon_path,
                        16.0,
                        rgb(0xffffff).opacity(0.84).into(),
                    )),
            )
            .child(
                v_flex()
                    .min_w_0()
                    .flex_1()
                    .gap(px(7.0))
                    .child(
                        div()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(0xffffff).opacity(0.94))
                            .child(tip.title),
                    )
                    .child(
                        div()
                            .max_h(px(33.0))
                            .overflow_hidden()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .line_height(px(16.2))
                            .text_color(rgb(0xffffff).opacity(0.58))
                            .child(tip.body),
                    ),
            );
        h_flex()
            .id(format!("gpui-titlebar-tip-row-{tip_index}"))
            .min_h(px(72.0))
            .items_start()
            .gap(px(10.0))
            .border_1()
            .border_color(rgb(0xffffff).opacity(0.10))
            .bg(rgb(0xffffff).opacity(0.025))
            .p(px(8.0))
            .pt(px(9.0))
            .when(read, |this| this.opacity(0.72))
            .when(actionable, |this| {
                this.hover(|this| {
                    this.bg(rgb(0xffffff).opacity(0.05))
                        .border_color(rgb(0xffffff).opacity(0.18))
                })
            })
            .child(detail)
            .child(
                div()
                    .id(format!("gpui-titlebar-tip-read-{tip_index}"))
                    .flex_shrink_0()
                    .flex()
                    .size(px(24.0))
                    .self_end()
                    .items_center()
                    .justify_center()
                    .text_color(if read {
                        rgb(0xffffff).opacity(0.46)
                    } else {
                        rgb(0xffffff).opacity(0.90)
                    })
                    .when(!read, |this| {
                        this.cursor_pointer()
                            .border_1()
                            .border_color(rgb(0xffffff).opacity(0.16))
                            .bg(rgb(0xffffff).opacity(0.14))
                            .hover(|this| this.bg(rgb(0xffffff).opacity(0.20)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                                    window.prevent_default();
                                    cx.stop_propagation();
                                    this.mark_tip_read(tip_index, cx);
                                }),
                            )
                    })
                    .child(titlebar_svg_icon(
                        "titlebar/check.svg",
                        15.0,
                        if read {
                            rgb(0xffffff).opacity(0.46).into()
                        } else {
                            rgb(0xffffff).opacity(0.90).into()
                        },
                    )),
            )
            .into_any_element()
    }

    fn render_notice_row(
        &self,
        notice_index: usize,
        notice: &GpuiNativeTitlebarNotice,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let target = notice.target;
        h_flex()
            .id(format!("gpui-titlebar-tip-notice-{notice_index}"))
            .min_h(px(72.0))
            .items_start()
            .gap(px(10.0))
            .border_1()
            .border_color(rgb(0xffffff).opacity(0.10))
            .bg(rgb(0xffffff).opacity(0.025))
            .p(px(8.0))
            .pt(px(9.0))
            .cursor_pointer()
            .hover(|this| {
                this.bg(rgb(0xf59e0b).opacity(0.06))
                    .border_color(rgb(0xf59e0b).opacity(0.34))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.open_notice_settings(target, window, cx);
                }),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .size(px(28.0))
                    .items_center()
                    .justify_center()
                    .bg(rgb(0xf59e0b).opacity(0.14))
                    .child(titlebar_svg_icon(
                        "titlebar/alert-triangle.svg",
                        16.0,
                        rgb(0xfbbf24).opacity(0.95).into(),
                    )),
            )
            .child(
                v_flex()
                    .min_w_0()
                    .flex_1()
                    .gap(px(7.0))
                    .child(
                        div()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(0xffffff).opacity(0.94))
                            .child(notice.title.clone()),
                    )
                    .child(
                        div()
                            .max_h(px(49.0))
                            .overflow_hidden()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .line_height(px(16.2))
                            .text_color(rgb(0xffffff).opacity(0.58))
                            .child(notice.body.clone()),
                    ),
            )
            .into_any_element()
    }

    fn missing_agent_hooks_notice(
        status: &serde_json::Value,
        live_agent_ids: &HashSet<String>,
    ) -> Option<GpuiNativeTitlebarNotice> {
        if status
            .get("errorMessage")
            .and_then(serde_json::Value::as_str)
            .is_some()
        {
            return None;
        }
        let mut outdated = Vec::new();
        let mut missing = Vec::new();
        for agent in status
            .get("agents")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
        {
            if agent
                .get("cliInstalled")
                .and_then(serde_json::Value::as_bool)
                != Some(true)
            {
                continue;
            }
            let hook_status = agent
                .get("status")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            if matches!(hook_status, "installed" | "notRequired" | "cliMissing") {
                continue;
            }
            let Some(agent_id) = agent.get("agentId").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let Some(default_agent) = gpui_default_sidebar_agent_by_id(agent_id) else {
                continue;
            };
            let entry = (
                default_agent.agent_id.to_string(),
                default_agent.name.to_string(),
            );
            if hook_status == "updateRequired" {
                outdated.push(entry);
            } else {
                missing.push(entry);
            }
        }
        outdated.sort_by_key(|(agent_id, _)| !live_agent_ids.contains(agent_id));
        missing.sort_by_key(|(agent_id, _)| !live_agent_ids.contains(agent_id));
        let names = outdated
            .iter()
            .chain(missing.iter())
            .map(|(_, name)| name.clone())
            .collect::<Vec<_>>();
        if names.is_empty() {
            return None;
        }
        let formatted_agents = match names.as_slice() {
            [name] => name.clone(),
            [first, second] => format!("{first} and {second}"),
            _ => format!(
                "{}, and {}",
                names[..names.len() - 1].join(", "),
                names.last().map(String::as_str).unwrap_or_default()
            ),
        };
        let has_outdated = !outdated.is_empty();
        let has_missing = !missing.is_empty();
        let (action_label, action_verb) = match (has_outdated, has_missing) {
            (true, true) => ("install or update", "installed or updated"),
            (true, false) => ("update", "updated"),
            (false, true) => ("install", "installed"),
            (false, false) => return None,
        };
        Some(GpuiNativeTitlebarNotice {
            body: format!(
                "Open Settings > Integrations to {action_label} agent hooks for {formatted_agents}. Automatic session renaming, In Progress/Needs Attention status, and sleeping or resuming agent sessions will not work correctly until hooks are {action_verb}."
            ),
            target: GpuiNativeTitlebarNoticeTarget::AgentHooks,
            title: "Warning: Agent hooks aren't installed for agent CLIs".to_string(),
        })
    }

    fn render_tips(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let GpuiTitlebarReadingPanelState::Tips {
            agent_hook_status,
            cli_status,
            live_agent_ids,
            read_ids,
        } = &self.state
        else {
            unreachable!();
        };
        let settings = shared_settings::shared_sidebar_settings_snapshot();
        let mut notices = Vec::new();
        if cli_status.as_ref().is_some_and(|status| {
            status.get("installed").and_then(serde_json::Value::as_bool) != Some(true)
                || status.get("gxUsable").and_then(serde_json::Value::as_bool) != Some(true)
        }) {
            notices.push(GpuiNativeTitlebarNotice {
                body: "Install or repair the CLI to use ghostex/gx in any terminal, attach mobile clients, and install Browser/Computer/Orchestration agent skills.".to_string(),
                target: GpuiNativeTitlebarNoticeTarget::GhostexCli,
                title: "Ghostex CLI is not accessible".to_string(),
            });
        }
        if gpui_titlebar_session_persistence_provider_from_settings(settings.object()) == "off" {
            notices.push(GpuiNativeTitlebarNotice {
                body: "Android and iOS attach can have issues while Session Persistence is Off. Enable zmx persistence so mobile clients reconnect to durable terminal sessions.".to_string(),
                target: GpuiNativeTitlebarNoticeTarget::SessionPersistence,
                title: "Mobile attach needs persistence".to_string(),
            });
        }
        if settings.debugging_mode() {
            notices.push(GpuiNativeTitlebarNotice {
                body: "Ghostex is showing debug UI controls and allowing enabled Diagnostic disk logging scenarios to write routine logs.".to_string(),
                target: GpuiNativeTitlebarNoticeTarget::DebuggingMode,
                title: "Debug mode is on".to_string(),
            });
        }
        if let Some(notice) = agent_hook_status
            .as_ref()
            .and_then(|status| Self::missing_agent_hooks_notice(status, live_agent_ids))
        {
            notices.push(notice);
        }
        let has_notices = !notices.is_empty();
        let unread = GPUI_NATIVE_TITLEBAR_TIPS
            .iter()
            .enumerate()
            .filter(|(_, tip)| !read_ids.contains(tip.id))
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let read = GPUI_NATIVE_TITLEBAR_TIPS
            .iter()
            .enumerate()
            .filter(|(_, tip)| read_ids.contains(tip.id))
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let has_unread = !unread.is_empty();
        let mut body = v_flex().w_full().p(px(10.0)).pt(px(8.0));
        if !notices.is_empty() {
            body = body
                .child(Self::render_tips_section_heading("NOTICES"))
                .child(
                    v_flex().w_full().mt(px(5.0)).gap(px(7.0)).children(
                        notices
                            .iter()
                            .enumerate()
                            .map(|(index, notice)| self.render_notice_row(index, notice, cx)),
                    ),
                );
        }
        if has_unread {
            body = body
                .when(has_notices, |this| this.mt(px(10.0)))
                .child(Self::render_tips_section_heading("UNREAD"))
                .child(
                    v_flex().w_full().gap(px(7.0)).children(
                        unread
                            .into_iter()
                            .map(|index| self.render_tip_row(index, false, cx)),
                    ),
                );
        }
        body = body.child(
            v_flex()
                .w_full()
                .when(has_notices || has_unread, |this| this.mt(px(10.0)))
                .child(Self::render_tips_section_heading("READ"))
                .child(if read.is_empty() {
                    div()
                        .p(px(4.0))
                        .py(px(10.0))
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(0xffffff).opacity(0.54))
                        .child("No read tips yet.")
                        .into_any_element()
                } else {
                    v_flex()
                        .w_full()
                        .gap(px(7.0))
                        .children(
                            read.into_iter()
                                .map(|index| self.render_tip_row(index, true, cx)),
                        )
                        .into_any_element()
                }),
        );
        v_flex()
            .size_full()
            .overflow_hidden()
            .bg(titlebar_popup_menu_background())
            .child(self.render_tips_header(cx))
            .child(
                div()
                    .relative()
                    .w_full()
                    .min_h_0()
                    .flex_1()
                    .child(
                        div()
                            .id("ghostex-gpui-titlebar-tips-scroll-area")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll_handle)
                            .child(body),
                    )
                    .child(
                        Scrollbar::vertical(&self.scroll_handle)
                            .thickness(px(TITLEBAR_DROPDOWN_SCROLLBAR_WIDTH)),
                    ),
            )
            .into_any_element()
    }

    fn render_resources(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let GpuiTitlebarReadingPanelState::Resources { snapshot, .. } = &self.state else {
            unreachable!();
        };
        v_flex()
            .size_full()
            .overflow_hidden()
            .bg(titlebar_popup_menu_background())
            .child(self.render_resources_header(snapshot, cx))
            .child(
                div()
                    .relative()
                    .w_full()
                    .min_h_0()
                    .flex_1()
                    .child(
                        v_flex()
                            .id("ghostex-gpui-titlebar-resources-scroll-area")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll_handle)
                            .p(px(10.0))
                            .pt(px(8.0))
                            .children(self.render_resource_sections(snapshot, cx)),
                    )
                    .child(
                        Scrollbar::vertical(&self.scroll_handle)
                            .thickness(px(TITLEBAR_DROPDOWN_SCROLLBAR_WIDTH)),
                    ),
            )
            .children(self.render_resources_info_popover(cx))
            .into_any_element()
    }

    fn render_resources_header(
        &self,
        snapshot: &GpuiNativeResourcesSnapshot,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let GpuiTitlebarReadingPanelState::Resources {
            collapsed_keys,
            info_open,
            ..
        } = &self.state
        else {
            unreachable!();
        };
        let collapse_target_keys = snapshot
            .server_rows
            .iter()
            .chain(snapshot.session_rows.iter())
            .chain(snapshot.code_rows.iter())
            .chain(snapshot.browser_rows.iter())
            .chain(snapshot.orphan_rows.iter())
            .enumerate()
            .filter(|(_, row)| !row.children.is_empty())
            .map(|(index, _)| format!("resource-{index}"))
            .collect::<Vec<_>>();
        let all_collapsed = !collapse_target_keys.is_empty()
            && collapse_target_keys
                .iter()
                .all(|key| collapsed_keys.contains(key));
        h_flex()
            .relative()
            .h(px(TITLEBAR_POPUP_READING_HEADER_HEIGHT))
            .flex_shrink_0()
            .items_stretch()
            .border_b_1()
            .border_color(rgb(0xffffff).opacity(0.12))
            .child(
                h_flex()
                    .min_w_0()
                    .flex_1()
                    .items_center()
                    .gap(px(8.0))
                    .pl(px(12.0))
                    .text_size(px(14.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(0xffffff).opacity(0.96))
                    .child(titlebar_svg_icon(
                        TITLEBAR_ICON_DEVICE_DESKTOP,
                        18.0,
                        rgb(0xffffff).opacity(0.96).into(),
                    ))
                    .child("Resources"),
            )
            .child(self.render_resource_icon_button(
                "gpui-resources-info",
                TITLEBAR_ICON_INFO,
                *info_open,
                true,
                cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    if let GpuiTitlebarReadingPanelState::Resources { info_open, .. } =
                        &mut this.state
                    {
                        *info_open = !*info_open;
                        cx.notify();
                    }
                }),
            ))
            .child(self.render_resource_icon_button(
                "gpui-resources-collapse-all",
                if all_collapsed {
                    "titlebar/arrows-diagonal-expand.svg"
                } else {
                    "titlebar/arrows-diagonal-minimize.svg"
                },
                false,
                !collapse_target_keys.is_empty(),
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    if let GpuiTitlebarReadingPanelState::Resources { collapsed_keys, .. } =
                        &mut this.state
                    {
                        if all_collapsed {
                            collapsed_keys.clear();
                        } else {
                            collapsed_keys.clear();
                            collapsed_keys.extend(collapse_target_keys.iter().cloned());
                        }
                        cx.notify();
                    }
                }),
            ))
            .child(self.render_resource_text_button(
                "gpui-resources-sleep-inactive",
                COMMAND_ICON_MOON,
                "Sleep Inactive",
                snapshot.inactive_terminal_sleep_count > 0,
                cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    let _ = this.main_app.update_in(cx, |app, _window, cx| {
                        app.dispatch_gpui_workspace_sleep_inactive_sessions(cx);
                    });
                }),
            ))
            .when(snapshot.persistent_session_mode, |this| {
                this.child(self.render_resource_text_button(
                    "gpui-resources-sleep-all",
                    COMMAND_ICON_MOON,
                    "Sleep All",
                    snapshot.sleep_all_session_count > 0,
                    cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        let _ = this.main_app.update_in(cx, |app, _window, cx| {
                            app.dispatch_gpui_workspace_sleep_all_daemon_sessions(cx);
                        });
                    }),
                ))
            })
            .child(
                h_flex()
                    .flex_shrink_0()
                    .h_full()
                    .items_center()
                    .gap(px(12.0))
                    .border_l_1()
                    .border_color(rgb(0xffffff).opacity(0.12))
                    .px(px(12.0))
                    .text_size(px(12.0))
                    .text_color(rgb(0xffffff).opacity(0.72))
                    .child(
                        h_flex()
                            .gap(px(5.0))
                            .child(titlebar_svg_icon(
                                "titlebar/cpu.svg",
                                13.0,
                                rgb(0xffffff).opacity(0.62).into(),
                            ))
                            .child(format_gpui_resource_cpu_compact(snapshot.total_cpu)),
                    )
                    .child(
                        h_flex()
                            .gap(px(5.0))
                            .child(titlebar_svg_icon(
                                TITLEBAR_ICON_DEVICE_DESKTOP,
                                13.0,
                                rgb(0xffffff).opacity(0.62).into(),
                            ))
                            .child(format_gpui_resource_memory_compact(
                                snapshot.total_memory_mb,
                            )),
                    ),
            )
            .into_any_element()
    }

    fn render_resource_icon_button(
        &self,
        id: &'static str,
        icon: &'static str,
        active: bool,
        enabled: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        h_flex()
            .id(id)
            .flex_shrink_0()
            .h_full()
            .w(px(TITLEBAR_POPUP_READING_HEADER_HEIGHT))
            .items_center()
            .justify_center()
            .border_l_1()
            .border_color(rgb(0xffffff).opacity(0.12))
            .when(active, |this| this.bg(rgb(0xffffff).opacity(0.14)))
            .when(enabled, |this| {
                this.cursor_pointer()
                    .hover(|this| this.bg(rgb(0xffffff).opacity(0.14)))
                    .on_mouse_down(MouseButton::Left, listener)
            })
            .when(!enabled, |this| this.opacity(0.45))
            .child(titlebar_svg_icon(
                icon,
                TITLEBAR_POPUP_READING_HEADER_BUTTON_ICON_SIZE,
                rgb(0xffffff).opacity(0.82).into(),
            ))
            .into_any_element()
    }

    fn render_resource_text_button(
        &self,
        id: &'static str,
        icon: &'static str,
        label: &'static str,
        enabled: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        h_flex()
            .id(id)
            .flex_shrink_0()
            .h_full()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .border_l_1()
            .border_color(rgb(0xffffff).opacity(0.12))
            .px(px(15.0))
            .text_size(px(TITLEBAR_POPUP_READING_HEADER_BUTTON_TEXT_SIZE))
            .font_weight(FontWeight::NORMAL)
            .text_color(rgb(0xffffff).opacity(if enabled { 0.78 } else { 0.30 }))
            .when(enabled, |this| {
                this.cursor_pointer()
                    .hover(|this| this.bg(rgb(0xffffff).opacity(0.14)))
                    .on_mouse_down(MouseButton::Left, listener)
            })
            .when(!enabled, |this| this.opacity(0.55))
            .child(titlebar_svg_icon(
                icon,
                TITLEBAR_POPUP_READING_HEADER_BUTTON_ICON_SIZE,
                rgb(0xffffff)
                    .opacity(if enabled { 0.78 } else { 0.30 })
                    .into(),
            ))
            .child(label)
            .into_any_element()
    }

    fn render_resources_info_popover(&self, _cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        let GpuiTitlebarReadingPanelState::Resources { info_open, .. } = &self.state else {
            return None;
        };
        if !*info_open {
            return None;
        }
        Some(
            v_flex()
                .absolute()
                .top(px(TITLEBAR_POPUP_READING_HEADER_HEIGHT + 9.0))
                .right(px(12.0))
                .w(px(620.0))
                .gap(px(10.0))
                .border_1()
                .border_color(rgb(0xffffff).opacity(0.14))
                .bg(rgb(0x3a3a3a))
                .p(px(10.0))
                .text_size(px(12.0))
                .line_height(px(16.2))
                .text_color(rgb(0xffffff).opacity(0.62))
                .child("This app uses native Ghostty terminals as they're lighter on CPU & RAM than electron/web terminals.")
                .child("The RAM use you see here is the lowest possible for the Agent CLI that you're using.")
                .child("Keep in mind that each CLI uses more/less RAM based on a lot of factors.")
                .child("You can easily sleep all inactive terminals here (Auto-sleep can be configured in settings).")
                .into_any_element(),
        )
    }

    fn render_resource_sections(
        &self,
        snapshot: &GpuiNativeResourcesSnapshot,
        cx: &mut gpui::Context<Self>,
    ) -> Vec<AnyElement> {
        let mut sections = Vec::new();
        let mut base_index = 0;
        for (label, rows) in [
            ("DEV SERVERS".to_string(), &snapshot.server_rows),
            (
                snapshot.project_label.to_uppercase(),
                &snapshot.session_rows,
            ),
            ("CODE IDE".to_string(), &snapshot.code_rows),
            ("BROWSER TABS".to_string(), &snapshot.browser_rows),
            ("ORPHANED / DETACHED".to_string(), &snapshot.orphan_rows),
        ] {
            if rows.is_empty() {
                continue;
            }
            sections.push(self.render_resource_section(label, rows, base_index, cx));
            base_index += rows.len();
        }
        if sections.is_empty() {
            sections.push(
                div()
                    .p(px(4.0))
                    .py(px(10.0))
                    .text_size(px(12.0))
                    .text_color(rgb(0xffffff).opacity(0.54))
                    .child("No grouped sessions matched running processes.")
                    .into_any_element(),
            );
        }
        sections
    }

    fn render_resource_section(
        &self,
        label: String,
        rows: &[GpuiNativeResourceRow],
        base_index: usize,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let cpu = rows.iter().map(|row| row.cpu).sum::<f64>();
        let memory = rows.iter().map(|row| row.memory_mb).sum::<f64>();
        let section_key = label.clone();
        let hovered = match &self.state {
            GpuiTitlebarReadingPanelState::Resources {
                hovered_sections, ..
            } => hovered_sections.contains(&section_key),
            _ => false,
        };
        let action_label = if rows
            .iter()
            .any(|row| matches!(row.action, GpuiNativeResourceAction::Session))
        {
            Some("Sleep Project")
        } else if rows
            .iter()
            .any(|row| matches!(row.action, GpuiNativeResourceAction::Server))
        {
            Some("Stop Servers")
        } else if rows.iter().any(|row| {
            matches!(
                row.action,
                GpuiNativeResourceAction::Browser(_)
                    | GpuiNativeResourceAction::Code
                    | GpuiNativeResourceAction::Orphan
            )
        }) {
            Some("Quit")
        } else {
            None
        };
        let rows_for_action = rows.to_vec();
        let section_action = action_label.map(|action_label| {
            h_flex()
                .id(format!(
                    "gpui-titlebar-resource-section-action-{base_index}"
                ))
                .ml_auto()
                .h(px(22.0))
                .items_center()
                .justify_center()
                .border_1()
                .border_color(if action_label == "Quit" {
                    rgb(0xf87171).opacity(0.28)
                } else {
                    rgb(0xffffff).opacity(0.13)
                })
                .bg(if action_label == "Quit" {
                    rgb(0xdc2626).opacity(0.18)
                } else {
                    rgb(0xffffff).opacity(0.08)
                })
                .px(px(8.0))
                .text_size(px(11.0))
                .text_color(rgb(0xffffff).opacity(0.86))
                .cursor_pointer()
                .hover(move |this| {
                    this.bg(if action_label == "Quit" {
                        rgb(0xdc2626).opacity(0.28)
                    } else {
                        rgb(0xffffff).opacity(0.14)
                    })
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        if action_label == "Stop Servers" {
                            gpui_terminate_native_resource_pids(
                                rows_for_action
                                    .iter()
                                    .flat_map(|row| row.pids.iter().copied())
                                    .collect(),
                                "INT",
                            );
                        } else if action_label == "Sleep Project" {
                            let session_ids = rows_for_action
                                .iter()
                                .filter_map(|row| row.session_id.clone())
                                .collect::<Vec<_>>();
                            let _ = this.main_app.update_in(cx, move |app, _window, cx| {
                                for session_id in session_ids {
                                    let Some(shell_session_id) =
                                        app.gpui_titlebar_resource_shell_session_id(&session_id)
                                    else {
                                        continue;
                                    };
                                    let Some(pane_id) =
                                        app.agents_workspace.pane_id_for_session(shell_session_id)
                                    else {
                                        continue;
                                    };
                                    app.sleep_agents_tabs_for_scope(
                                        pane_id,
                                        shell_session_id,
                                        AgentsWorkspaceTabSleepScope::Sleep,
                                        cx,
                                    );
                                }
                            });
                        } else {
                            for row in &rows_for_action {
                                match row.action {
                                    GpuiNativeResourceAction::Browser(tab_id) => {
                                        let _ = this.main_app.update_in(
                                            cx,
                                            move |app, main_window, cx| {
                                                app.close_browser_tab_model(
                                                    tab_id,
                                                    main_window,
                                                    cx,
                                                );
                                            },
                                        );
                                    }
                                    GpuiNativeResourceAction::Code => {
                                        let _ = this.main_app.update_in(cx, |app, _window, cx| {
                                            app.stop_source_code_server_runtime(cx);
                                        });
                                    }
                                    GpuiNativeResourceAction::Orphan => {
                                        gpui_terminate_native_resource_pids(
                                            row.pids.clone(),
                                            "TERM",
                                        );
                                    }
                                    _ => {}
                                }
                            }
                        }
                        cx.notify();
                    }),
                )
                .child(action_label)
                .into_any_element()
        });
        v_flex()
            .w_full()
            .when(base_index > 0, |this| this.mt(px(8.0)))
            .child(
                h_flex()
                    .id(format!(
                        "gpui-titlebar-resource-section-heading-{base_index}"
                    ))
                    .h(px(24.0))
                    .items_center()
                    .gap(px(6.0))
                    .px(px(2.0))
                    .text_size(px(11.0))
                    .text_color(rgb(0xffffff).opacity(0.62))
                    .on_hover(cx.listener(move |this, hovered, _window, cx| {
                        if let GpuiTitlebarReadingPanelState::Resources {
                            hovered_sections, ..
                        } = &mut this.state
                        {
                            let changed = if *hovered {
                                hovered_sections.insert(section_key.clone())
                            } else {
                                hovered_sections.remove(&section_key)
                            };
                            if changed {
                                cx.notify();
                            }
                        }
                    }))
                    .child(label)
                    .child(if hovered {
                        section_action.unwrap_or_else(|| div().into_any_element())
                    } else {
                        h_flex()
                            .ml_auto()
                            .gap(px(10.0))
                            .text_color(rgb(0xffffff).opacity(0.52))
                            .child(
                                h_flex()
                                    .gap(px(4.0))
                                    .child(titlebar_svg_icon(
                                        "titlebar/cpu.svg",
                                        12.0,
                                        rgb(0xffffff).opacity(0.52).into(),
                                    ))
                                    .child(format_gpui_resource_cpu_compact(cpu)),
                            )
                            .child(
                                h_flex()
                                    .gap(px(4.0))
                                    .child(titlebar_svg_icon(
                                        TITLEBAR_ICON_DEVICE_DESKTOP,
                                        12.0,
                                        rgb(0xffffff).opacity(0.52).into(),
                                    ))
                                    .child(format_gpui_resource_memory_compact(memory)),
                            )
                            .child(
                                div()
                                    .text_color(rgb(0xffffff).opacity(0.38))
                                    .child(format!("{}", rows.len())),
                            )
                            .into_any_element()
                    }),
            )
            .child(
                v_flex().w_full().gap(px(7.0)).children(
                    rows.iter()
                        .cloned()
                        .enumerate()
                        .map(|(index, row)| self.render_resource_row(row, base_index + index, cx)),
                ),
            )
            .into_any_element()
    }

    fn render_resource_row(
        &self,
        row: GpuiNativeResourceRow,
        row_index: usize,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let key = format!("resource-{row_index}");
        let (collapsed, quitting) = match &self.state {
            GpuiTitlebarReadingPanelState::Resources {
                collapsed_keys,
                quitting_keys,
                ..
            } => (collapsed_keys.contains(&key), quitting_keys.contains(&key)),
            _ => (true, false),
        };
        let expandable = !row.children.is_empty();
        let session_id = row.session_id.clone();
        let url = row.url.clone();
        let action = row.action.clone();
        let action_row = row.clone();
        let resource_detail = if quitting {
            match &action {
                GpuiNativeResourceAction::Session => "Sleeping...".to_string(),
                GpuiNativeResourceAction::Server => "Stopping...".to_string(),
                GpuiNativeResourceAction::Browser(_)
                | GpuiNativeResourceAction::Code
                | GpuiNativeResourceAction::Orphan => "Quitting...".to_string(),
                GpuiNativeResourceAction::None => row.detail.clone(),
            }
        } else {
            row.detail.clone()
        };
        let resource_name = if matches!(action, GpuiNativeResourceAction::Server) {
            if let Some(main_url) = row.url.clone() {
                div()
                    .id(format!("gpui-titlebar-resource-link-{row_index}"))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(px(13.0))
                    .text_color(rgb(0xffffff).opacity(0.94))
                    .cursor_pointer()
                    .hover(|this| this.text_color(rgb(0x9dd7f6).opacity(0.98)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            let main_url = main_url.clone();
                            let _ = this.main_app.update_in(cx, move |app, main_window, cx| {
                                let settings = shared_settings::shared_sidebar_settings_snapshot();
                                if !settings.web_links_open_in_app() {
                                    let _ = gpui_spawn_os_open(std::ffi::OsStr::new(&main_url));
                                } else {
                                    app.open_gpui_browser_action_url(main_url, main_window, cx);
                                }
                            });
                        }),
                    )
                    .child(row.label.clone())
                    .into_any_element()
            } else {
                div()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(px(13.0))
                    .text_color(rgb(0xffffff).opacity(0.94))
                    .child(row.label.clone())
                    .into_any_element()
            }
        } else {
            div()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(px(13.0))
                .text_color(rgb(0xffffff).opacity(0.94))
                .child(row.label.clone())
                .into_any_element()
        };
        let avatar = if let Some(agent_icon) = row.agent_icon
            && let Some(icon_path) = workspace_tab_agent_icon_path(agent_icon)
        {
            svg()
                .path(icon_path)
                .size(px(15.0))
                .text_color(rgb(workspace_tab_agent_icon_accent_color(agent_icon)))
                .into_any_element()
        } else {
            titlebar_svg_icon(row.icon_path, 15.0, rgb(0xffffff).opacity(0.82).into())
                .into_any_element()
        };
        let primary_action = if let Some(session_id) = session_id {
            let focus_session_id = session_id.clone();
            self.render_resource_square_action(
                format!("gpui-titlebar-resource-focus-{row_index}"),
                "titlebar/focus-2.svg",
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    let _ = this.main_app.update_in(cx, |app, _window, cx| {
                        app.focus_gpui_titlebar_resource_session(&focus_session_id, cx);
                    });
                    this.close_popup(window, cx);
                }),
            )
        } else if matches!(action, GpuiNativeResourceAction::Server) {
            if let Some(url) = url {
                self.render_resource_square_action(
                    format!("gpui-titlebar-resource-open-{row_index}"),
                    "titlebar/focus-2.svg",
                    cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        let url = url.clone();
                        let _ = this.main_app.update_in(cx, move |app, main_window, cx| {
                            app.open_gpui_browser_action_url(url, main_window, cx);
                        });
                    }),
                )
            } else {
                div().size(px(22.0)).flex_shrink_0().into_any_element()
            }
        } else {
            div().size(px(22.0)).flex_shrink_0().into_any_element()
        };
        let secondary_action = if matches!(action, GpuiNativeResourceAction::None) {
            div().size(px(22.0)).flex_shrink_0().into_any_element()
        } else {
            let key_for_action = key.clone();
            self.render_resource_square_action(
                format!("gpui-titlebar-resource-secondary-{row_index}"),
                match action {
                    GpuiNativeResourceAction::Session => COMMAND_ICON_MOON,
                    GpuiNativeResourceAction::Server => "titlebar/square-minus.svg",
                    _ => COMMAND_ICON_XMARK,
                },
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.run_resource_secondary_action(
                        key_for_action.clone(),
                        action_row.clone(),
                        window,
                        cx,
                    );
                }),
            )
        };
        let primary_action = div()
            .flex_shrink_0()
            .flex()
            .w(px(24.0))
            .items_center()
            .justify_center()
            .child(primary_action);
        let secondary_action = div()
            .flex_shrink_0()
            .flex()
            .w(px(24.0))
            .items_center()
            .justify_center()
            .child(secondary_action);
        let row_toggle_key = key.clone();
        v_flex()
            .id(format!("gpui-titlebar-resource-{row_index}"))
            .w_full()
            .overflow_hidden()
            .border_1()
            .border_color(rgb(0xffffff).opacity(0.10))
            .bg(rgb(0xffffff).opacity(0.025))
            .when(quitting, |this| this.opacity(0.30))
            .child(
                h_flex()
                    .min_h(px(44.0))
                    .items_center()
                    .gap(px(8.0))
                    .p(px(8.0))
                    .py(px(7.0))
                    .when(expandable, |this| {
                        this.on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event: &MouseDownEvent, _window, cx| {
                                if let GpuiTitlebarReadingPanelState::Resources {
                                    collapsed_keys,
                                    ..
                                } = &mut this.state
                                {
                                    if !collapsed_keys.remove(&row_toggle_key) {
                                        collapsed_keys.insert(row_toggle_key.clone());
                                    }
                                    cx.notify();
                                }
                            }),
                        )
                    })
                    .child(
                        h_flex()
                            .min_w_0()
                            .flex_1()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .id(format!("gpui-titlebar-resource-collapse-{row_index}"))
                                    .flex_shrink_0()
                                    .flex()
                                    .size(px(20.0))
                                    .items_center()
                                    .justify_center()
                                    .when(expandable, |this| {
                                        let key = key.clone();
                                        this.cursor_pointer().on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                                                window.prevent_default();
                                                cx.stop_propagation();
                                                if let GpuiTitlebarReadingPanelState::Resources { collapsed_keys, .. } = &mut this.state {
                                                    if !collapsed_keys.remove(&key) {
                                                        collapsed_keys.insert(key.clone());
                                                    }
                                                    cx.notify();
                                                }
                                            }),
                                        )
                                    })
                                    .children(expandable.then(|| {
                                        titlebar_svg_icon(
                                            if collapsed {
                                                BROWSER_ICON_CHEVRON_RIGHT
                                            } else {
                                                TITLEBAR_ICON_CHEVRON_DOWN
                                            },
                                            12.0,
                                            rgb(0xffffff).opacity(0.55).into(),
                                        )
                                    })),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .flex()
                                    .size(px(28.0))
                                    .items_center()
                                    .justify_center()
                                    .bg(rgb(0xffffff).opacity(0.10))
                                    .child(avatar),
                            )
                            .child(
                                v_flex()
                                    .min_w_0()
                                    .flex_1()
                                    .gap(px(2.0))
                                    .child(resource_name)
                                    .child(
                                        div()
                                            .overflow_hidden()
                                            .whitespace_nowrap()
                                            .text_ellipsis()
                                            .text_size(px(12.0))
                                            .text_color(rgb(0xffffff).opacity(0.58))
                                            .child(resource_detail),
                                    ),
                            ),
                    )
                    .child(primary_action)
                    .child(secondary_action)
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .w(px(200.0))
                            .gap(px(8.0))
                            .child(resource_metric_chip(
                                "titlebar/cpu.svg",
                                format_gpui_resource_cpu_compact(row.cpu),
                                86.0,
                            ))
                            .child(resource_metric_chip(
                                TITLEBAR_ICON_DEVICE_DESKTOP,
                                format_gpui_resource_memory_compact(row.memory_mb),
                                106.0,
                            )),
                    ),
            )
            .when(expandable && !collapsed, |this| {
                this.child(
                    v_flex()
                        .w_full()
                        .pb(px(8.0))
                        .pr(px(8.0))
                        .pl(px(64.0))
                        .children(row.children.into_iter().map(|child| {
                            h_flex()
                                .min_h(px(24.0))
                                .items_center()
                                .gap(px(8.0))
                                .child(
                                    h_flex()
                                        .min_w_0()
                                        .flex_1()
                                        .text_size(px(12.0))
                                        .text_color(rgb(0xffffff).opacity(0.58))
                                        .child(child.label)
                                        .child(
                                            div()
                                                .ml(px(4.0))
                                                .child(format!("pid {}", child.pid)),
                                        ),
                                )
                                .child(resource_metric_chip(
                                    "titlebar/cpu.svg",
                                    format_gpui_resource_cpu_compact(child.cpu),
                                    86.0,
                                ))
                                .child(resource_metric_chip(
                                    TITLEBAR_ICON_DEVICE_DESKTOP,
                                    format_gpui_resource_memory_compact(child.memory_mb),
                                    106.0,
                                ))
                        })),
                )
            })
            .into_any_element()
    }

    fn render_resource_square_action(
        &self,
        id: String,
        icon: &'static str,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        h_flex()
            .id(id)
            .flex_shrink_0()
            .size(px(22.0))
            .items_center()
            .justify_center()
            .border_1()
            .border_color(rgb(0xffffff).opacity(0.16))
            .bg(rgb(0xffffff).opacity(0.14))
            .cursor_pointer()
            .hover(|this| this.bg(rgb(0xffffff).opacity(0.20)))
            .on_mouse_down(MouseButton::Left, listener)
            .child(titlebar_svg_icon(
                icon,
                12.0,
                rgb(0xffffff).opacity(0.90).into(),
            ))
            .into_any_element()
    }
}

impl Render for GpuiTitlebarReadingPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .size_full()
            .overflow_hidden()
            .border_1()
            .border_color(titlebar_popup_menu_border_color())
            .bg(titlebar_popup_menu_background())
            .child(match self.state {
                GpuiTitlebarReadingPanelState::Tips { .. } => self.render_tips(cx),
                GpuiTitlebarReadingPanelState::Resources { .. } => self.render_resources(cx),
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
    h_flex()
        .flex_shrink_0()
        .w(px(width))
        .h(px(24.0))
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .border_1()
        .border_color(rgb(0xffffff).opacity(0.105))
        .bg(rgb(0xffffff).opacity(0.055))
        .text_size(px(12.0))
        .text_color(rgb(0xffffff).opacity(0.88))
        .child(titlebar_svg_icon(
            icon,
            12.0,
            rgb(0xffffff).opacity(0.62).into(),
        ))
        .child(label)
        .into_any_element()
}

pub(crate) fn gpui_terminate_native_resource_pids(pids: Vec<u32>, signal: &'static str) {
    let pids = pids
        .into_iter()
        .filter(|pid| *pid > 1)
        .map(|pid| pid.to_string())
        .collect::<Vec<_>>();
    if pids.is_empty() {
        return;
    }
    thread::spawn(move || {
        let mut command = Command::new("/bin/kill");
        command.arg(format!("-{signal}")).args(pids);
        let _ = command.status();
    });
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
        CDXC:GPUITitlebarTips 2026-06-24-23:17:
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
        CDXC:GPUIResourcesTitlebar 2026-07-08:
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
        SOURCE_CODE_SERVER_EDITOR_PORT, project_state_update
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
    CDXC:GPUITitlebarNativeHostRunProcess 2026-07-08:
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
