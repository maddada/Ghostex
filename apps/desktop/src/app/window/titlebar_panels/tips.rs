use super::*;

impl GpuiTitlebarReadingPanel {
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
        sidebar_agent_ids: Option<&HashSet<String>>,
    ) -> Option<GpuiNativeTitlebarNotice> {
        if status
            .get("errorMessage")
            .and_then(serde_json::Value::as_str)
            .is_some()
        {
            return None;
        }
        // Until the sidebar HUD read answers, no agent is known to be in use, so nothing is warned about; see gpui_sidebar_default_agent_ids_from_hud_agents.
        let sidebar_agent_ids = sidebar_agent_ids?;
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
            if !sidebar_agent_ids.contains(default_agent.agent_id) {
                continue;
            }
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
                "Open Settings > Agents to {action_label} agent hooks for {formatted_agents}. Automatic session renaming, In Progress/Needs Attention status, and sleeping or resuming agent sessions will not work correctly until hooks are {action_verb}."
            ),
            target: GpuiNativeTitlebarNoticeTarget::AgentHooks,
            title: "Warning: Agent hooks aren't installed for agent CLIs".to_string(),
        })
    }

    pub(super) fn render_tips(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let GpuiTitlebarReadingPanelState::Tips {
            agent_hook_status,
            cli_status,
            live_agent_ids,
            read_ids,
            sidebar_agent_ids,
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
        if settings.debugging_mode() {
            notices.push(GpuiNativeTitlebarNotice {
                body: "Ghostex is showing debug UI controls and allowing enabled Diagnostic disk logging scenarios to write routine logs.".to_string(),
                target: GpuiNativeTitlebarNoticeTarget::DebuggingMode,
                title: "Debug mode is on".to_string(),
            });
        }
        if let Some(notice) = agent_hook_status.as_ref().and_then(|status| {
            Self::missing_agent_hooks_notice(status, live_agent_ids, sidebar_agent_ids.as_ref())
        }) {
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
}
