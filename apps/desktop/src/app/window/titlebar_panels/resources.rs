use super::*;

/// Width of one action button slot in a resource row; three slots are reserved, right-aligned, before the metric chips.
const RESOURCE_ACTION_SLOT_WIDTH: f32 = 24.0;

impl GpuiTitlebarReadingPanel {
    pub(super) fn render_resources(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let GpuiTitlebarReadingPanelState::Resources { snapshot, .. } = &self.state else {
            unreachable!();
        };
        v_flex()
            .size_full()
            .overflow_hidden()
            .child(self.render_resources_header(snapshot, cx))
            .when(snapshot.session_inventory_error.is_some(), |this| {
                this.child(div().p(px(10.0)).text_size(px(12.0)).child(
                    "Session ownership could not be loaded from gxserver. Some terminal rows are unavailable; reopen Resources to retry."
                ))
            })
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
            clean_ram_copied,
            info_open,
            ..
        } = &self.state
        else {
            unreachable!();
        };
        let clean_ram_copied = *clean_ram_copied;
        resource_header()
            .child(
                resource_heading()
                    .child(titlebar_svg_icon(
                        TITLEBAR_ICON_DEVICE_DESKTOP,
                        18.0,
                        chrome_ink().opacity(0.96).into(),
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
            .child(self.render_resource_text_button(
                "gpui-resources-sleep-inactive",
                COMMAND_ICON_MOON,
                "Sleep Inactive",
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
            /*
            CDXC:Resources 2026-09-16 DECISION:
            User: keep Clean RAM, and on click "show a toast telling the user that they need to paste this into an agent session to reduce ram use".
            This keeps the 2026-09-04 wrench button in Sleep All's place that copies a diagnosis prompt, and adds the toast so the clipboard copy is not silent.
            */
            .child(self.render_resource_text_button(
                "gpui-resources-clean-ram",
                "titlebar/tool.svg",
                if clean_ram_copied {
                    "Copied"
                } else {
                    "Clean RAM"
                },
                "Clean RAM",
                true,
                cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.copy_clean_ram_prompt(cx);
                }),
            ))
            .child(
                h_flex()
                    .flex_shrink_0()
                    .h_full()
                    .items_center()
                    .gap(px(12.0))
                    .border_l_1()
                    .border_color(chrome_ink().opacity(0.12))
                    .px(px(12.0))
                    .text_size(px(12.0))
                    .text_color(chrome_ink().opacity(0.72))
                    .child(
                        h_flex()
                            .gap(px(5.0))
                            .child(titlebar_svg_icon(
                                "titlebar/cpu.svg",
                                13.0,
                                chrome_ink().opacity(0.62).into(),
                            ))
                            .child(format_gpui_resource_cpu_compact(snapshot.total_cpu)),
                    )
                    .child(
                        h_flex()
                            .gap(px(5.0))
                            .child(titlebar_svg_icon(
                                TITLEBAR_ICON_DEVICE_DESKTOP,
                                13.0,
                                chrome_ink().opacity(0.62).into(),
                            ))
                            .child(format_gpui_resource_memory_compact(
                                snapshot.total_memory_mb,
                            )),
                    ),
            )
            .into_any_element()
    }

    fn copy_clean_ram_prompt(&mut self, cx: &mut gpui::Context<Self>) {
        let prompt = {
            let GpuiTitlebarReadingPanelState::Resources { snapshot, .. } = &self.state else {
                return;
            };
            gpui_resources_clean_ram_prompt(snapshot)
        };
        gpui_copy_to_clipboard(ClipboardItem::new_string(prompt), cx);
        if let GpuiTitlebarReadingPanelState::Resources {
            clean_ram_copied, ..
        } = &mut self.state
        {
            *clean_ram_copied = true;
        }
        cx.notify();
        let _ = self.main_app.update_in(cx, |app, _window, cx| {
            app.upsert_gpui_app_toast(
                GpuiAppToast {
                    id: "gpui-resources-clean-ram-copied".to_string(),
                    level: GpuiAppToastLevel::Info,
                    title: "Clean RAM prompt copied".to_string(),
                    description: Some(
                        "Paste it into an agent session to reduce RAM use.".to_string(),
                    ),
                    copy_text: None,
                    loading: false,
                    persistent: false,
                    duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                    action: None,
                    epoch: 0,
                },
                cx,
            );
        });
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(2))
                .await;
            let _ = this.update(cx, |this, cx| {
                if let GpuiTitlebarReadingPanelState::Resources {
                    clean_ram_copied, ..
                } = &mut this.state
                {
                    *clean_ram_copied = false;
                    cx.notify();
                }
            });
        })
        .detach();
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
            .border_color(chrome_ink().opacity(0.12))
            .when(active, |this| this.bg(chrome_ink().opacity(0.14)))
            .when(enabled, |this| {
                this.cursor_pointer()
                    .hover(|this| this.bg(chrome_ink().opacity(0.14)))
                    .on_mouse_down(MouseButton::Left, listener)
            })
            .when(!enabled, |this| this.opacity(0.45))
            .child(titlebar_svg_icon(
                icon,
                TITLEBAR_POPUP_READING_HEADER_BUTTON_ICON_SIZE,
                chrome_ink().opacity(0.82).into(),
            ))
            .into_any_element()
    }

    fn render_resource_text_button(
        &self,
        id: &'static str,
        icon: &'static str,
        label: &'static str,
        width_label: &'static str,
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
            .border_color(chrome_ink().opacity(0.12))
            .px(px(15.0))
            .text_size(px(TITLEBAR_POPUP_READING_HEADER_BUTTON_TEXT_SIZE))
            .font_weight(FontWeight::NORMAL)
            .text_color(chrome_ink().opacity(if enabled { 0.78 } else { 0.30 }))
            .when(enabled, |this| {
                this.cursor_pointer()
                    .hover(|this| this.bg(chrome_ink().opacity(0.14)))
                    .on_mouse_down(MouseButton::Left, listener)
            })
            .when(!enabled, |this| this.opacity(0.55))
            .child(titlebar_svg_icon(
                icon,
                TITLEBAR_POPUP_READING_HEADER_BUTTON_ICON_SIZE,
                chrome_ink()
                    .opacity(if enabled { 0.78 } else { 0.30 })
                    .into(),
            ))
            .child(
                div()
                    .relative()
                    .whitespace_nowrap()
                    .child(
                        div()
                            .whitespace_nowrap()
                            .when(label != width_label, |this| this.opacity(0.0))
                            .child(width_label),
                    )
                    .when(label != width_label, |this| {
                        this.child(
                            div()
                                .absolute()
                                .inset_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .whitespace_nowrap()
                                .child(label),
                        )
                    }),
            )
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
                // Hosted in the view panel this page can be as narrow as
                // `WORKAREA_VIEW_PANEL_MIN_WIDTH`, where a fixed 620px note started off the left
                // edge of the frame and had its first characters clipped away.
                .left(px(12.0))
                .right(px(12.0))
                .max_w(px(620.0))
                .gap(px(10.0))
                .rounded(px(RESOURCE_CARD_RADIUS))
                .border_1()
                .border_color(chrome_ink().opacity(0.14))
                .bg(chrome_color(0x3a3a3a, 0xe5e5e5))
                .p(px(10.0))
                .text_size(px(12.0))
                .line_height(px(16.2))
                .text_color(chrome_ink().opacity(0.62))
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
        for (label, rows) in snapshot.session_sections().chain([
            ("CODE IDE", snapshot.code_rows.as_slice()),
            ("BROWSER TABS", snapshot.browser_rows.as_slice()),
            ("ORPHANED / DETACHED", snapshot.orphan_rows.as_slice()),
        ]) {
            if rows.is_empty() {
                continue;
            }
            sections.push(self.render_resource_section(label.to_uppercase(), rows, base_index, cx));
            base_index += rows.len();
        }
        if sections.is_empty() {
            sections.push(
                div()
                    .p(px(4.0))
                    .py(px(10.0))
                    .text_size(px(12.0))
                    .text_color(chrome_ink().opacity(0.54))
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
        let section_key = format!("resource-section-{label}");
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
                .when(action_label == "Sleep Project", |this| this.mr(px(-2.0)))
                .h(px(22.0))
                .items_center()
                .justify_center()
                .rounded(px(RESOURCE_CONTROL_RADIUS))
                .border_1()
                .border_color(if action_label == "Quit" {
                    chrome_color(0xf87171, 0xc23b3b).opacity(0.40)
                } else {
                    chrome_ink().opacity(0.13)
                })
                // The destructive chip has to read as destructive on white too, where the
                // dark-mode reds flattened into pale pink.
                .bg(if action_label == "Quit" {
                    chrome_color(0xdc2626, 0xdc2626).opacity(if chrome_uses_light_appearance() {
                        0.10
                    } else {
                        0.18
                    })
                } else {
                    chrome_ink().opacity(0.08)
                })
                .px(px(8.0))
                .text_size(px(11.0))
                .text_color(chrome_ink().opacity(0.86))
                .cursor_pointer()
                .hover(move |this| {
                    this.bg(if action_label == "Quit" {
                        rgb(0xdc2626).opacity(if chrome_uses_light_appearance() {
                            0.18
                        } else {
                            0.28
                        })
                    } else {
                        chrome_ink().opacity(0.14)
                    })
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        if action_label == "Stop Servers" {
                            gpui_terminate_native_resource_processes(
                                rows_for_action
                                    .iter()
                                    .flat_map(|row| row.termination_targets.iter().cloned())
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
                                    app.sleep_gpui_titlebar_resource_session(&session_id, cx);
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
                                            app.sleep_titlebar_view(TitlebarMode::Source, cx);
                                        });
                                    }
                                    GpuiNativeResourceAction::Orphan => {
                                        gpui_terminate_native_resource_processes(
                                            row.termination_targets.clone(),
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
                resource_section_heading()
                    // CDXC:Resources 2026-09-09 DECISION:
                    // User: move Sleep Project up by 4px and right by 2px to leave a gap above the project's session rows and align its right edge.
                    .when(action_label == Some("Sleep Project"), |this| {
                        this.mt(px(-4.0)).h(px(28.0)).pb(px(4.0))
                    })
                    .id(format!(
                        "gpui-titlebar-resource-section-heading-{base_index}"
                    ))
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
                    .child(
                        div()
                            .when(action_label == Some("Sleep Project"), |this| {
                                this.mt(px(8.0))
                            })
                            .child(label),
                    )
                    .child(if hovered {
                        section_action.unwrap_or_else(|| div().into_any_element())
                    } else {
                        h_flex()
                            .ml_auto()
                            .when(action_label == Some("Sleep Project"), |this| {
                                this.mt(px(8.0))
                            })
                            .gap(px(10.0))
                            .text_color(chrome_ink().opacity(0.52))
                            .child(
                                h_flex()
                                    .gap(px(4.0))
                                    .child(titlebar_svg_icon(
                                        "titlebar/cpu.svg",
                                        12.0,
                                        chrome_ink().opacity(0.52).into(),
                                    ))
                                    .child(format_gpui_resource_cpu_compact(cpu)),
                            )
                            .child(
                                h_flex()
                                    .gap(px(4.0))
                                    .child(titlebar_svg_icon(
                                        TITLEBAR_ICON_DEVICE_DESKTOP,
                                        12.0,
                                        chrome_ink().opacity(0.52).into(),
                                    ))
                                    .child(format_gpui_resource_memory_compact(memory)),
                            )
                            .child(
                                div()
                                    .text_color(chrome_ink().opacity(0.38))
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
        let key = match &row.session_id {
            Some(session_id) => format!("resource-session-{session_id}"),
            None => format!("resource-{}-{:?}", row.label, row.pids),
        };
        let (collapsed, pending_action) = match &self.state {
            GpuiTitlebarReadingPanelState::Resources {
                expanded_keys,
                pending_actions,
                ..
            } => (
                !expanded_keys.contains(&key),
                pending_actions.get(&key).copied(),
            ),
            _ => (true, None),
        };
        let quitting = pending_action.is_some();
        let expandable = !row.children.is_empty();
        let session_id = row.session_id.clone();
        let url = row.url.clone();
        let action = row.action.clone();
        let action_row = row.clone();
        let resource_detail = pending_action.unwrap_or(&row.detail).to_string();
        let resource_name = if matches!(action, GpuiNativeResourceAction::Server) {
            if let Some(main_url) = row.url.clone() {
                resource_name_text()
                    .id(format!("gpui-titlebar-resource-link-{row_index}"))
                    .cursor_pointer()
                    .hover(|this| this.text_color(chrome_color(0x9dd7f6, 0x286282).opacity(0.98)))
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
                resource_name_text()
                    .child(row.label.clone())
                    .into_any_element()
            }
        } else {
            resource_name_text()
                .child(row.label.clone())
                .into_any_element()
        };
        let avatar = if let Some(agent_icon) = row.agent_icon
            && let Some(icon_path) = workspace_tab_agent_icon_path(agent_icon)
        {
            svg()
                .path(icon_path)
                .size(px(15.0))
                // The page's surface is white in light mode, where several agents' white accent
                // would leave a blank square; `chrome_agent_icon_color` turns those two to ink.
                .text_color(chrome_agent_icon_color(agent_icon))
                .into_any_element()
        } else {
            titlebar_svg_icon(row.icon_path, 15.0, chrome_ink().opacity(0.82).into())
                .into_any_element()
        };
        let primary_action = if let Some(session_id) = session_id {
            let focus_session_id = session_id.clone();
            Some(self.render_resource_square_action(
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
            ))
        } else if matches!(action, GpuiNativeResourceAction::Server) {
            if let Some(url) = url {
                Some(self.render_resource_square_action(
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
                ))
            } else {
                None
            }
        } else {
            None
        };
        let secondary_action = if matches!(action, GpuiNativeResourceAction::None) {
            None
        } else {
            let key_for_action = key.clone();
            Some(self.render_resource_square_action(
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
            ))
        };
        // CDXC:Resources 2026-09-08 DECISION:
        // User: add a Close session button to the right of the existing Sleep button.
        let close_action = if matches!(action, GpuiNativeResourceAction::Session) {
            row.session_id.clone().map(|session_id| {
                let key = key.clone();
                self.render_resource_square_action(
                    format!("gpui-titlebar-resource-close-{row_index}"),
                    COMMAND_ICON_XMARK,
                    cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        if let GpuiTitlebarReadingPanelState::Resources {
                            pending_actions, ..
                        } = &this.state
                            && pending_actions.contains_key(&key)
                        {
                            return;
                        }
                        let closed = this
                            .main_app
                            .update_in(cx, |app, _window, cx| {
                                app.close_gpui_titlebar_resource_session(&session_id, cx)
                            })
                            .unwrap_or(false);
                        if closed {
                            if let GpuiTitlebarReadingPanelState::Resources {
                                pending_actions,
                                expanded_keys,
                                snapshot,
                                ..
                            } = &mut this.state
                            {
                                snapshot.remove_closed_session(&session_id);
                                pending_actions.remove(&key);
                                expanded_keys.remove(&key);
                            }
                            cx.notify();
                        }
                    }),
                )
            })
        } else {
            None
        };
        // CDXC:Resources 2026-11-05 DECISION:
        // User: the orphaned rows' x sat left of the session rows' x. The actions pack against the CPU
        // chip in one fixed three-slot column, so the last action of every row (x, minus, or close)
        // lines up across all sections instead of each action keeping its own fixed slot.
        let action_slots = h_flex()
            .flex_shrink_0()
            .w(px(RESOURCE_ACTION_SLOT_WIDTH * 3.0))
            .items_center()
            .justify_end()
            .children(
                [primary_action, secondary_action, close_action]
                    .into_iter()
                    .flatten()
                    .map(|action| {
                        div()
                            .flex_shrink_0()
                            .flex()
                            .w(px(RESOURCE_ACTION_SLOT_WIDTH))
                            .items_center()
                            .justify_center()
                            .child(action)
                    }),
            );
        let row_toggle_key = key.clone();
        resource_row_frame()
            .id(format!("gpui-titlebar-resource-{row_index}"))
            .when(quitting, |this| this.opacity(0.30))
            .child(
                resource_row_content()
                    .when(expandable, |this| {
                        this.on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event: &MouseDownEvent, _window, cx| {
                                if let GpuiTitlebarReadingPanelState::Resources {
                                    expanded_keys,
                                    ..
                                } = &mut this.state
                                {
                                    if !expanded_keys.remove(&row_toggle_key) {
                                        expanded_keys.insert(row_toggle_key.clone());
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
                                                if let GpuiTitlebarReadingPanelState::Resources { expanded_keys, .. } = &mut this.state {
                                                    if !expanded_keys.remove(&key) {
                                                        expanded_keys.insert(key.clone());
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
                                            chrome_ink().opacity(0.55).into(),
                                        )
                                    })),
                            )
                            .child(
                                resource_avatar_tile()
                                    .child(avatar),
                            )
                            .child(
                                v_flex()
                                    .min_w_0()
                                    .flex_1()
                                    .gap(px(2.0))
                                    .child(resource_name)
                                    .child(
                                        resource_detail_text()
                                            .child(resource_detail),
                                    ),
                            ),
                    )
                    .child(action_slots)
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
                                        .text_color(chrome_ink().opacity(0.58))
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
        resource_square_button(id)
            .on_mouse_down(MouseButton::Left, listener)
            .child(titlebar_svg_icon(
                icon,
                12.0,
                chrome_ink().opacity(0.90).into(),
            ))
            .into_any_element()
    }
}
