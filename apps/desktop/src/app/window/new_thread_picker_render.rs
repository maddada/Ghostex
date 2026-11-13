//! The New Thread picker's drawing: the key hints, the search field, the rows of the agent,
//! account and project lists, and the `Render` impl that assembles them. A child of
//! `new_thread_picker.rs` (included with `#[path]`, so the preview binary builds it too), which is
//! why it reads the picker's private state directly.
use super::*;

impl GpuiNewThreadPickerWindow {
    /// `.new-thread-palette-hints kbd`: 16px chips tinted from the foreground.
    pub(super) fn render_kbd(&self, label: &'static str) -> impl IntoElement {
        let c = &self.colors;
        div()
            .flex()
            .h(px(16.0))
            .min_w(px(16.0))
            .px(px(4.0))
            .items_center()
            .justify_center()
            .rounded(px(4.0))
            .bg(hsla(rgba_of(c.foreground, 0.08)))
            .border_1()
            .border_color(hsla(rgba_of(c.foreground, 0.14)))
            .text_size(px(10.0))
            .line_height(px(14.0))
            .child(label)
    }

    pub(super) fn render_hint(
        &self,
        keys: &[&'static str],
        label: &'static str,
    ) -> impl IntoElement {
        h_flex()
            .items_center()
            .gap(px(4.0))
            .children(keys.iter().map(|key| self.render_kbd(key)))
            .child(label)
    }

    /// `.new-thread-palette-hints`: 11px muted at 80%, centered, 9px 16px 7px.
    /// CDXC:AgentLauncher 2026-09-16 DECISION:
    /// User: "center the controls please in this modal and add 3px margin from top/bottom of the controls": the key-hint chips sit centered under the search field with 3px more room above and below them than the React palette first had; the CSS twin carries the same values.
    pub(super) fn render_hints(&self) -> impl IntoElement {
        let in_accounts = self.scope.is_some();
        h_flex()
            .flex_shrink_0()
            .flex_wrap()
            .items_center()
            .justify_center()
            .gap(px(12.0))
            .px(px(16.0))
            .pt(px(9.0))
            .pb(px(7.0))
            .text_size(px(11.0))
            .line_height(px(16.0))
            .text_color(hsla(rgba_of(self.colors.muted, 0.8)))
            .child(self.render_hint(&["↑", "↓"], HINT_MOVE))
            .child(self.render_hint(&["↵"], HINT_START))
            // A box location has no account list for Tab to open.
            .when(in_accounts || !self.run_on.in_box(), |hints| {
                hints.child(if in_accounts {
                    self.render_hint(&["←"], HINT_BACK)
                } else {
                    self.render_hint(&["⇥"], HINT_ACCOUNTS)
                })
            })
            .children(self.run_on_key_hint())
            .child(self.render_hint(&["esc"], if in_accounts { HINT_BACK } else { HINT_CLOSE }))
    }

    /// `.new-thread-palette-scope`: the provider chip inside the search field
    /// while its accounts are listed; clicking it goes back.
    pub(super) fn render_scope_chip(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let c = self.colors;
        self.scope_agent().map(|agent| {
            h_flex()
                .id("ghostex-gpui-new-thread-picker-scope")
                .flex_shrink_0()
                .h(px(22.0))
                .items_center()
                .gap(px(5.0))
                .pl(px(4.0))
                .pr(px(7.0))
                .rounded(px(4.0))
                .bg(hsla(rgba_of(c.foreground, 0.08)))
                .hover(|this| this.bg(hsla(rgba_of(c.foreground, 0.14))))
                .text_size(px(12.0))
                .text_color(hsla(c.foreground))
                .child(modal_icon(ICON_BACK, 12.0, c.foreground))
                .child(self.render_agent_icon(agent, 14.0, 0.0))
                .child(agent.name.clone())
                .on_click(cx.listener(|this, _, window, cx| {
                    this.leave_scope(window, cx);
                }))
        })
    }

    /// `CommandInput`: the 36px input group with the search glyph at the end
    /// while empty and a clear button once something is typed.
    pub(super) fn render_search(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = self.colors;
        let has_query = !self.query.is_empty();
        h_flex()
            .flex_shrink_0()
            .h(px(NEW_THREAD_PICKER_SEARCH_HEIGHT))
            .mx(px(6.0))
            .mt(px(6.0))
            .pl(px(12.0))
            .pr(px(12.0))
            .gap(px(10.0))
            .items_center()
            .rounded(px(6.0))
            .border_1()
            .border_color(hsla(c.input_border))
            .bg(hsla(c.input_background))
            .children(self.render_scope_chip(cx))
            .child(
                div().flex_1().min_w_0().child(
                    Input::new(&self.input)
                        .with_size(ComponentSize::XSmall)
                        .appearance(false)
                        .bordered(false)
                        .focus_bordered(false)
                        .w_full()
                        .px(px(0.0))
                        .py(px(0.0))
                        .text_size(px(ROW_TEXT_SIZE))
                        .text_color(hsla(c.item)),
                ),
            )
            .child(if has_query {
                div()
                    .id("ghostex-gpui-new-thread-picker-clear")
                    .flex()
                    .flex_shrink_0()
                    .size(px(24.0))
                    .items_center()
                    .justify_center()
                    .text_color(hsla(c.muted))
                    .hover(|this| this.text_color(hsla(c.foreground)))
                    .child(modal_icon(ICON_CLEAR, 16.0, c.muted))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.clear_query(window, cx);
                        this.selected = 0;
                        this.input.update(cx, |input, cx| input.focus(window, cx));
                        cx.notify();
                    }))
                    .into_any_element()
            } else {
                modal_icon(ICON_SEARCH, 16.0, rgba_of(c.muted, 0.5)).into_any_element()
            })
    }

    /// `ProjectAgentLauncherIcon` in brand colour: the provider logo in its
    /// 14px box, or the code glyph for agents without one.
    pub(super) fn render_agent_icon(
        &self,
        agent: &NewThreadPickerAgent,
        box_size: f32,
        grow: f32,
    ) -> AnyElement {
        let c = self.colors;
        div()
            .flex()
            .flex_shrink_0()
            .size(px(box_size))
            .items_center()
            .justify_center()
            .child(match agent.icon_path {
                Some(path) => {
                    modal_icon(path, agent.icon_svg_size + grow, c.brand(agent.icon_accent))
                }
                None => modal_icon(ICON_CODE, 14.0, c.glyph),
            })
            .into_any_element()
    }

    /// `.new-thread-palette-row`: 36px, 10px inset, 5px radius; the selected row
    /// carries the composer fill and its 1px ring.
    pub(super) fn row_shell(
        &self,
        id: impl Into<ElementId>,
        row: PickerRow,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let c = self.colors;
        h_flex()
            .id(id)
            .flex_shrink_0()
            .w_full()
            .min_h(px(NEW_THREAD_PICKER_ROW_HEIGHT))
            .items_center()
            .gap(px(8.0))
            .px(px(10.0))
            .rounded(px(5.0))
            .border_1()
            .border_color(transparent())
            .text_size(px(ROW_TEXT_SIZE))
            .line_height(px(ROW_LINE_HEIGHT))
            .text_color(hsla(c.label))
            .when(selected, |this| {
                this.bg(hsla(c.selected_background))
                    .border_color(hsla(c.selected_ring))
                    .text_color(hsla(c.selected_label))
            })
            .when(!selected, |this| {
                this.hover(|this| this.bg(hsla(rgba_of(c.foreground, 0.05))))
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.activate(row, window, cx);
            }))
    }

    pub(super) fn render_agent_row(
        &self,
        index: usize,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = self.colors;
        let agent = &self.agents[index];
        // A box has no account list and opens in the terminal, so neither marker applies there.
        let in_box = self.run_on.in_box();
        let provider = agent.provider().filter(|_| !in_box);
        let account_count = provider.and_then(|provider| self.provider_account_count(provider));
        self.row_shell(
            ElementId::Name(format!("new-thread-agent-{}", agent.agent_id).into()),
            PickerRow::Agent(index),
            selected,
            cx,
        )
        .child(self.render_agent_icon(agent, 14.0, 0.0))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .child(agent.name.clone()),
        )
        .when(provider.is_some(), |this| {
            // `.group-agent-menu-accounts` + `button.group-agent-menu-account-button`:
            // muted at 58%, 24px tall, its own 5px hover surface and outline.
            this.child(
                h_flex()
                    .id(ElementId::Name(
                        format!("new-thread-accounts-{}", agent.agent_id).into(),
                    ))
                    .flex_shrink_0()
                    .h(px(24.0))
                    .min_w(px(28.0))
                    .items_center()
                    .justify_center()
                    .gap(px(3.0))
                    .px(px(4.0))
                    .rounded(px(5.0))
                    .border_1()
                    .border_color(transparent())
                    .text_size(px(12.0))
                    .text_color(hsla(rgba_of(c.muted, 0.58)))
                    .hover(|this| {
                        this.bg(hsla(rgba_of(c.foreground, 0.10)))
                            .border_color(hsla(rgba_of(c.foreground, 0.18)))
                            .text_color(hsla(rgba_of(c.muted, 0.9)))
                    })
                    .child(modal_icon(ICON_ACCOUNTS, 14.0, rgba_of(c.muted, 0.58)))
                    .children(account_count.map(|count| count.to_string()))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.enter_scope(index, window, cx);
                    })),
            )
        })
        .when(!in_box && agent.supports_chat(), |this| {
            // `.group-agent-menu-chat-support`: muted at 58%, 6px before and 5px after.
            this.child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .ml(px(6.0))
                    .mr(px(5.0))
                    .child(modal_icon(ICON_CHAT, 14.0, rgba_of(c.muted, 0.58))),
            )
        })
        .into_any_element()
    }

    pub(super) fn render_plain_row(
        &self,
        id: &'static str,
        row: PickerRow,
        icon_path: &'static str,
        label: &'static str,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = self.colors;
        self.row_shell(id, row, selected, cx)
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .size(px(14.0))
                    .items_center()
                    .justify_center()
                    .child(modal_icon(icon_path, 14.0, c.glyph)),
            )
            .child(div().flex_1().min_w_0().child(label))
            .into_any_element()
    }

    /// `.new-thread-palette-account-row`: the 16px provider logo, the name with
    /// its Default marker, and the usage figures underneath.
    pub(super) fn render_account_row(
        &self,
        index: usize,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = self.colors;
        let (Some(agent), Some(account)) = (self.scope_agent(), self.account(index)) else {
            return div().into_any_element();
        };
        let display_name = account.name.clone();
        let usage = account.usage.clone();
        // A cloud session's default is the project's last cloud account, when it has one.
        let is_default = match self
            .cloud_default_account
            .as_deref()
            .filter(|_| self.cloud_scope())
        {
            Some(default) => account.id == default,
            None => account.is_default,
        };
        let ready = account.ready;
        self.row_shell(
            ElementId::Name(format!("new-thread-account-{index}").into()),
            PickerRow::Account(index),
            selected,
            cx,
        )
        .py(px(6.0))
        .when(!ready, |this| this.opacity(0.5))
        .child(self.render_agent_icon(agent, 16.0, 1.5))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(2.0))
                .child(
                    h_flex()
                        .min_w_0()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            div()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .child(display_name),
                        )
                        .when(is_default, |this| {
                            this.child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(11.0))
                                    .text_color(hsla(c.muted))
                                    .child(DEFAULT_SUFFIX),
                            )
                        }),
                )
                .children(usage.map(|usage| {
                    div()
                        .font_family(MODAL_MONO_FONT)
                        .text_size(px(10.5))
                        .line_height(px(13.0))
                        .text_color(hsla(c.muted))
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(usage)
                })),
        )
        .into_any_element()
    }

    /// `.gx-account-launcher-hint` inside the palette: 11px muted, 6px 10px.
    pub(super) fn render_hint_text(&self, text: impl Into<SharedString>) -> AnyElement {
        div()
            .px(px(10.0))
            .py(px(6.0))
            .text_size(px(11.0))
            .line_height(px(16.5))
            .text_color(hsla(self.colors.muted))
            .child(text.into())
            .into_any_element()
    }

    /// `CommandEmpty`: centered 14px text with 24px above and below.
    pub(super) fn render_empty(&self, text: &'static str) -> AnyElement {
        div()
            .w_full()
            .py(px(24.0))
            .text_center()
            .text_size(px(ROW_TEXT_SIZE))
            .line_height(px(ROW_LINE_HEIGHT))
            .text_color(hsla(self.colors.item))
            .child(text)
            .into_any_element()
    }

    pub(super) fn render_divider(&self) -> AnyElement {
        div()
            .flex_shrink_0()
            .h(px(1.0))
            .mx(px(4.0))
            .my(px(4.0))
            .bg(hsla(self.colors.divider))
            .into_any_element()
    }

    pub(super) fn render_list(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let rows = self.rows();
        let mut children: Vec<AnyElement> = Vec::new();
        match self.scope_agent() {
            None => {
                let agent_rows = rows
                    .iter()
                    .filter(|row| matches!(row, PickerRow::Agent(_)))
                    .count();
                // One search for every "Start <agent> in" row drawn below.
                let start_places = if first_start_row(&rows).is_some() {
                    self.start_row_places()
                } else {
                    Vec::new()
                };
                for (position, row) in rows.iter().enumerate() {
                    let selected = position == self.selected;
                    if agent_rows > 0 && position == agent_rows {
                        children.push(self.render_divider());
                    }
                    if first_start_row(&rows) == Some(position) {
                        children.push(self.render_start_heading());
                    }
                    children.push(match *row {
                        PickerRow::Agent(index) => self.render_agent_row(index, selected, cx),
                        PickerRow::StartIn(index) => self.render_start_row(
                            index,
                            start_places.iter().find(|start| start.choice == index),
                            selected,
                            cx,
                        ),
                        PickerRow::Coordinator => self.render_plain_row(
                            "new-thread-coordinator",
                            PickerRow::Coordinator,
                            ICON_COORDINATOR,
                            ROW_COORDINATOR,
                            selected,
                            cx,
                        ),
                        PickerRow::Browser => self.render_plain_row(
                            "new-thread-browser",
                            PickerRow::Browser,
                            ICON_BROWSER,
                            ROW_BROWSER,
                            selected,
                            cx,
                        ),
                        PickerRow::Terminal => self.render_plain_row(
                            "new-thread-terminal",
                            PickerRow::Terminal,
                            ICON_TERMINAL,
                            ROW_TERMINAL,
                            selected,
                            cx,
                        ),
                        _ => continue,
                    });
                }
                if rows.is_empty() {
                    children.push(if self.agents_loaded {
                        self.render_empty(EMPTY_AGENTS)
                    } else {
                        self.render_hint_text(LOADING_AGENTS)
                    });
                }
            }
            Some(agent) => {
                let agent = agent.clone();
                // The pick is final for a cloud session: Claude cannot move it to another account.
                if self.cloud_scope() && self.accounts.is_some() {
                    children.push(self.render_hint_text(CLOUD_ACCOUNT_NOTE));
                }
                if self.accounts.is_none() && self.accounts_error.is_none() {
                    children.push(self.render_hint_text(READING_ACCOUNTS));
                }
                if let Some(error) = &self.accounts_error
                    && self.accounts.is_none()
                {
                    children.push(self.render_hint_text(error.clone()));
                }
                for (position, row) in rows.iter().enumerate() {
                    let selected = position == self.selected;
                    match *row {
                        PickerRow::Account(index) => {
                            children.push(self.render_account_row(index, selected, cx));
                        }
                        PickerRow::CliLogin => {
                            children.push(
                                self.row_shell(
                                    "new-thread-cli-login",
                                    PickerRow::CliLogin,
                                    selected,
                                    cx,
                                )
                                .child(self.render_agent_icon(&agent, 14.0, 0.0))
                                .child(div().flex_1().min_w_0().child(ROW_CLI_LOGIN))
                                .into_any_element(),
                            );
                            children.push(self.render_hint_text(CLI_LOGIN_HINT));
                            children.push(self.render_divider());
                            children.push(self.render_hint_text(ADD_ACCOUNT_HINT));
                        }
                        PickerRow::AddAccount => {
                            children.push(
                                self.row_shell(
                                    "new-thread-add-account",
                                    PickerRow::AddAccount,
                                    selected,
                                    cx,
                                )
                                .child(div().flex_1().min_w_0().child(ROW_ADD_ACCOUNT))
                                .into_any_element(),
                            );
                        }
                        PickerRow::Retry => {
                            children.push(
                                self.row_shell(
                                    "new-thread-retry-accounts",
                                    PickerRow::Retry,
                                    selected,
                                    cx,
                                )
                                .child(div().flex_1().min_w_0().child(ROW_TRY_AGAIN))
                                .into_any_element(),
                            );
                        }
                        _ => {}
                    }
                }
                if self.accounts.is_some()
                    && rows.is_empty()
                    && agent
                        .provider()
                        .is_some_and(|provider| !self.provider_accounts(provider).is_empty())
                {
                    children.push(self.render_empty(EMPTY_ACCOUNTS));
                }
            }
        }
        children
    }
}

impl Render for GpuiNewThreadPickerWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = self.colors;
        let choosing = self.choosing_project;
        let list_children = if choosing {
            self.render_project_list(cx)
        } else {
            self.render_list(cx)
        };
        let place = self.render_place(cx);
        v_flex()
            .id("ghostex-gpui-new-thread-picker")
            .size_full()
            .overflow_hidden()
            .rounded(px(10.0))
            .border_1()
            .border_color(hsla(c.frame_border))
            // Under window glass the picker's window blurs what is behind it, so its fill thins.
            // Under glass it takes the app's frosted dialog fill, passed in by the app because this
            // file also builds into the demo binary (`frosted_panel_fill` in helpers/window_glass.rs).
            .bg(self.frosted_fill.unwrap_or_else(|| hsla(c.surface)))
            .font_family(PICKER_FONT)
            .text_size(px(ROW_TEXT_SIZE))
            .line_height(px(ROW_LINE_HEIGHT))
            .text_color(hsla(c.item))
            .capture_action(cx.listener(Self::on_move_up))
            .capture_action(cx.listener(Self::on_move_down))
            .capture_action(cx.listener(Self::on_enter))
            .capture_action(cx.listener(Self::on_escape))
            .capture_action(cx.listener(Self::on_tab))
            .capture_action(cx.listener(Self::on_move_right))
            .capture_action(cx.listener(Self::on_move_left))
            .capture_action(cx.listener(Self::on_backspace))
            .capture_action(cx.listener(Self::on_move_home))
            .capture_action(cx.listener(Self::on_move_end))
            .capture_key_down(cx.listener(Self::on_run_on_key_down))
            .children(self.render_coordinator_title())
            .children(place)
            .child(self.render_search(cx))
            .when(
                !choosing && self.place.is_none() && self.run_on.shown(),
                |root| root.child(self.render_run_on(cx)),
            )
            .map(|root| {
                if choosing {
                    root.child(self.render_project_hints().into_any_element())
                } else {
                    root.child(self.render_hints().into_any_element())
                }
            })
            .child(
                div()
                    .relative()
                    .w_full()
                    .min_h_0()
                    .flex_1()
                    .child(
                        v_flex()
                            .id("ghostex-gpui-new-thread-picker-list")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll)
                            .px(px(6.0))
                            .pt(px(2.0))
                            .pb(px(6.0))
                            .children(list_children),
                    )
                    // CDXC:AgentLauncher 2026-10-29 DECISION:
                    // User: "dont show the scroll unless i'm scrolling (then scroll fades away when i stop scrolling)". The agent list's bar uses gpui-component's Scrolling mode instead of the app theme's Hover mode (which showed it whenever the pointer was over the list): hidden at rest, shown while the list scrolls (wheel, trackpad, or the keyboard moving the highlight past an edge), kept while the pointer is on the visible bar so it can be dragged, and faded out after the idle hold.
                    .child(
                        Scrollbar::vertical(&self.scroll)
                            .mode(ScrollbarMode::Scrolling)
                            .thickness(px(SCROLLBAR_WIDTH)),
                    ),
            )
            .children(self.render_coordinator_warning())
    }
}

impl GpuiNewThreadPickerWindow {
    /// The coordinator step's title: "Pick the coordinator's agent", left-aligned like the
    /// dialogs' titles.
    fn render_coordinator_title(&self) -> Option<AnyElement> {
        self.coordinator.as_ref()?;
        Some(
            div()
                .flex_shrink_0()
                .h(px(COORDINATOR_TITLE_HEIGHT))
                .px(px(14.0))
                .pt(px(10.0))
                .text_size(px(13.0))
                .line_height(px(18.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(hsla(self.colors.foreground))
                .child(TITLE_COORDINATOR_STEP)
                .into_any_element(),
        )
    }

    /// The coordinator step's warning line for the highlighted agent; the line keeps its room
    /// while there is none.
    fn render_coordinator_warning(&self) -> Option<AnyElement> {
        self.coordinator.as_ref()?;
        let warning = self.coordinator_warning().map(str::to_string);
        let color = if self.colors.light {
            rgb(0xb45309)
        } else {
            rgb(0xf5b84a)
        };
        Some(
            h_flex()
                .id("new-thread-coordinator-warning")
                .flex_shrink_0()
                .h(px(COORDINATOR_WARNING_HEIGHT))
                .px(px(14.0))
                .gap(px(8.0))
                .items_center()
                .border_t_1()
                .border_color(hsla(self.colors.divider))
                .when_some(warning, |line, warning| {
                    line.child(
                        gpui::svg()
                            .path(ICON_WARNING)
                            .flex_shrink_0()
                            .size(px(14.0))
                            .text_color(hsla(color)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(hsla(color))
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(warning),
                    )
                })
                .into_any_element(),
        )
    }
}
