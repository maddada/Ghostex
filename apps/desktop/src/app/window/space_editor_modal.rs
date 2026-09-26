//! Native GPUI New Space / Edit Space dialog, the desktop twin of the React
//! `SpaceEditorModal` in packages/core-ui/space-editor-modal.tsx with its
//! shared `CommandIconPicker`.
//!
//! CDXC:Spaces 2026-09-15 DECISION:
//! User: the React app modals are being rebuilt in GPUI one at a time and each native dialog must match its React twin 1 to 1: the same layout, copy, colors, states and behaviour in both appearances. The Space editor keeps the React contract: it owns no Space document and reports only name, icon and color (or a Delete), the icon picker offers the same 59-icon allowlist ranked by cmdk's scorer, and the color strip is the group palette drawn as swatches.
//! SEE-ALSO: packages/core-ui/space-editor-modal.tsx, packages/core-ui/command-icon-picker.tsx and the `.space-editor-*` / `.command-icon-picker-*` rules in packages/core-ui/styles/modals.css and modals-light.css (the React twin), packages/shared/sidebar-command-icons.ts and packages/core-ui/sidebar-command-icon.tsx (the icon allowlist mirrored in `SPACE_EDITOR_ICONS`), packages/core-ui/project-collections.ts (the color palette), apps/desktop/src/app/window/native_modal_kit.rs (shared chrome and controls), apps/desktop/src/app/space_editor_modal_lifecycle.rs (open and sidebar bridge), apps/desktop/src/bin/native_modal_demo/space_editor.rs (standalone preview).
use super::native_modal_kit::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    Anchor, AnyElement, App, AppContext as _, BoxShadow, ClickEvent, Context, FocusHandle,
    Focusable as _, FontWeight, InteractiveElement as _, IntoElement, KeyDownEvent, MouseDownEvent,
    ParentElement as _, Render, Rgba, ScrollHandle, StatefulInteractiveElement as _, Styled as _,
    Window, anchored, deferred, div, point, px, rgb,
};
use gpui_component::input::{Enter, Escape, Input, InputEvent, InputState, MoveDown, MoveUp};
use gpui_component::{Sizable as _, Size as ComponentSize, h_flex, v_flex};
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

/// `APP_MODAL_HOST_SIDEBAR_SPACE_EDITOR_WINDOW_WIDTH`.
pub(crate) const SPACE_EDITOR_MODAL_WIDTH: f32 = 480.0;
/// First-frame height only (`APP_MODAL_HOST_SIDEBAR_SPACE_EDITOR_WINDOW_HEIGHT`); the window is resized to the measured layout.
pub(crate) const SPACE_EDITOR_MODAL_INITIAL_HEIGHT: f32 = 380.0;

const ICON_CHEVRON_DOWN: &str = "modals/space-editor/chevron-down.svg";
const ICON_SEARCH: &str = "modals/space-editor/search.svg";
const ICON_CLEAR: &str = "modals/space-editor/x.svg";
const ICON_CHECK: &str = "modals/space-editor/check.svg";

const TITLE_EDIT: &str = "Edit Space";
const TITLE_CREATE: &str = "New Space";
const DESCRIPTION: &str = "A Space is a saved sidebar filter. Add groups and ungrouped projects to it from their own right-click menus.";
const NAME_LABEL: &str = "Name";
const ICON_LABEL: &str = "Icon";
const COLOR_LABEL: &str = "Color";
const SEARCH_ICONS: &str = "Search icons";
const NO_MATCHING_ICONS: &str = "No matching icons";
const CANCEL: &str = "Cancel";
const DELETE: &str = "Delete";
const SAVE: &str = "Save";
const CREATE: &str = "Create";

/// `DEFAULT_SIDEBAR_SPACE_ICON`.
const DEFAULT_ICON_ID: &str = "stack";

/// The popover geometry of the React picker: Base UI's 4px side offset and 5px
/// collision padding, the 8px-padded search row
/// with its 32px input group and hairline, the 4px list padding, and the
/// `.searchable-dropdown` list cap of `min(288px, 50vh)`.
const PICKER_SIDE_OFFSET: f32 = 4.0;
const PICKER_WINDOW_MARGIN: f32 = 5.0;
const PICKER_SEARCH_BLOCK_HEIGHT: f32 = 8.0 + 32.0 + 8.0 + 1.0;
const PICKER_LIST_PADDING: f32 = 4.0;
const PICKER_ROW_HEIGHT: f32 = 32.0;
const PICKER_LIST_MAX_HEIGHT: f32 = 288.0;
const PICKER_EMPTY_HEIGHT: f32 = 10.0 + 17.4 + 10.0;

/// One entry of `SIDEBAR_COMMAND_ICON_IDS`, with the label from
/// `getSidebarCommandIconLabel` and the Tabler glyph `ICON_COMPONENT_BY_ID` renders.
pub(crate) struct SpaceEditorIcon {
    pub(crate) id: &'static str,
    pub(crate) label: &'static str,
    asset: &'static str,
}

macro_rules! icon {
    ($id:literal, $label:literal, $file:literal) => {
        SpaceEditorIcon {
            id: $id,
            label: $label,
            asset: concat!("modals/space-editor/", $file, ".svg"),
        }
    };
}

pub(crate) const SPACE_EDITOR_ICONS: [SpaceEditorIcon; 59] = [
    icon!("playerPlay", "Play", "player-play-filled"),
    icon!("api", "API", "api"),
    icon!("archive", "Archive", "archive-filled"),
    icon!("bell", "Notification", "bell-filled"),
    icon!("bolt", "Bolt", "bolt-filled"),
    icon!("book", "Book", "book-filled"),
    icon!("brain", "Brain", "brain"),
    icon!("braces", "Braces", "braces"),
    icon!("brandDocker", "Docker", "brand-docker"),
    icon!("brandGithub", "GitHub", "brand-github-filled"),
    icon!("brandPython", "Python", "brand-python"),
    icon!("brandReact", "React", "brand-react"),
    icon!("brandVscode", "VS Code", "brand-vscode"),
    icon!("bug", "Bug", "bug-filled"),
    icon!("chartBar", "Metrics", "chart-bar"),
    icon!("cloud", "Cloud", "cloud-filled"),
    icon!("checklist", "Checklist", "checklist"),
    icon!("clock", "Timer", "clock-filled"),
    icon!("code", "Code", "code"),
    icon!("command", "Command", "command"),
    icon!("cpu", "Compute", "cpu"),
    icon!("database", "Database", "database-filled"),
    icon!("deviceDesktop", "Desktop", "device-desktop-filled"),
    icon!("deviceLaptop", "Laptop", "device-laptop"),
    icon!("download", "Download", "download-filled"),
    icon!("fileCode", "Code File", "file-code-filled"),
    icon!("fileDiff", "Diff", "file-diff-filled"),
    icon!("fileSearch", "Search File", "file-search"),
    icon!("fileText", "Docs", "file-text-filled"),
    icon!("flask", "Experiment", "flask-filled"),
    icon!("folder", "Folder", "folder-filled"),
    icon!("folderOpen", "Open Folder", "folder-open-filled"),
    icon!("gitBranch", "Branch", "git-branch"),
    icon!("gitCommit", "Commit", "git-commit"),
    icon!("gitMerge", "Merge", "git-merge"),
    icon!("gitPullRequest", "Pull Request", "git-pull-request"),
    icon!("key", "Key", "key-filled"),
    icon!("layoutDashboard", "Dashboard", "layout-dashboard-filled"),
    icon!("link", "Link", "link"),
    icon!("lock", "Lock", "lock-filled"),
    icon!("messageCircle", "Chat", "message-circle-filled"),
    icon!("package", "Package", "package"),
    icon!("pencilCode", "Edit", "pencil-code"),
    icon!("refresh", "Refresh", "refresh"),
    icon!("robot", "Agent", "robot"),
    icon!("route", "Route", "route"),
    icon!("rocket", "Launch", "rocket"),
    icon!("search", "Search", "search-filled"),
    icon!("server", "Server", "server"),
    icon!("settings", "Settings", "settings-filled"),
    icon!("shieldSearch", "Security", "shield-search"),
    icon!("sparkles", "Sparkles", "sparkles-filled"),
    icon!("stack", "Stack", "stack-filled"),
    icon!("terminal", "Terminal", "terminal-2"),
    icon!("testPipe", "Test", "test-pipe"),
    icon!("tool", "Tools", "tool"),
    icon!("upload", "Upload", "upload"),
    icon!("wand", "Generate", "wand"),
    icon!("world", "Browser", "world"),
];

/// `SIDEBAR_SPACE_COLORS` with `getSidebarSpaceColorLabel` from packages/core-ui/space-colors.ts.
pub(crate) const SPACE_EDITOR_COLORS: [(&str, &str); 12] = [
    (SPACE_GRAY_LIGHT_THEME, "Gray"),
    ("#7c6df2", "Violet"),
    ("#3aa675", "Green"),
    ("#d6873f", "Orange"),
    ("#d75b72", "Pink"),
    ("#3f8fc7", "Blue"),
    ("#b36ad4", "Purple"),
    ("#8c9b45", "Lime"),
    ("#c95353", "Red"),
    ("#c4a23d", "Gold"),
    ("#2f9b95", "Teal"),
    ("#596fd1", "Indigo"),
];

/// CDXC:Spaces 2026-09-21 DECISION: User: merge the two gray Space colors into one, and use one of them for dark mode and one for light mode.
/// A Space saved with either hex is the single Gray, drawn as #4f5663 on light themes and #808080 on dark themes.
/// SEE-ALSO: packages/core-ui/space-colors.ts.
const SPACE_GRAY_LIGHT_THEME: &str = "#4f5663";
const SPACE_GRAY_DARK_THEME: &str = "#808080";

fn is_space_gray(color: &str) -> bool {
    let color = color.trim();
    color.eq_ignore_ascii_case(SPACE_GRAY_LIGHT_THEME)
        || color.eq_ignore_ascii_case(SPACE_GRAY_DARK_THEME)
}

/// The hex a Space draws with under the given appearance.
pub(crate) fn space_display_color(color: &str, light: bool) -> &str {
    if !is_space_gray(color) {
        color
    } else if light {
        SPACE_GRAY_LIGHT_THEME
    } else {
        SPACE_GRAY_DARK_THEME
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SpaceEditorMode {
    Create,
    Edit,
}

pub(crate) struct SpaceEditorModalConfig {
    pub(crate) mode: SpaceEditorMode,
    pub(crate) initial_name: Option<String>,
    pub(crate) initial_icon: Option<String>,
    pub(crate) initial_color: Option<String>,
    pub(crate) palette: ModalPalette,
}

/// What the dialog asks its host to do. The dialog removes its own window before sending any of these.
pub(crate) enum SpaceEditorModalCommand {
    Submit {
        name: String,
        icon: String,
        color: String,
    },
    Delete,
    Cancel,
}

pub(crate) type SpaceEditorModalHost = Rc<dyn Fn(SpaceEditorModalCommand, &mut App)>;

pub(crate) struct GpuiSpaceEditorModalWindow {
    host: SpaceEditorModalHost,
    palette: ModalPalette,
    mode: SpaceEditorMode,
    name: gpui::Entity<InputState>,
    icon: usize,
    color: String,
    picker_open: bool,
    picker_search: gpui::Entity<InputState>,
    /// cmdk's `data-selected` row, as an index into the filtered results.
    picker_highlight: Option<usize>,
    picker_scroll: ScrollHandle,
    picker_trigger_bounds: Rc<Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
    fit: ModalFit,
    focus_handle: FocusHandle,
    _subscriptions: Vec<gpui::Subscription>,
}

impl GpuiSpaceEditorModalWindow {
    pub(crate) fn new(
        config: SpaceEditorModalConfig,
        host: SpaceEditorModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let initial_name = config.initial_name.unwrap_or_default();
        let name = cx.new(|cx| InputState::new(window, cx).default_value(initial_name.clone()));
        let picker_search = cx.new(|cx| InputState::new(window, cx).placeholder(SEARCH_ICONS));
        let subscriptions = vec![
            cx.subscribe_in(
                &name,
                window,
                |this: &mut Self, _input, event: &InputEvent, window, cx| match event {
                    InputEvent::Change => cx.notify(),
                    InputEvent::PressEnter { .. } => this.submit(window, cx),
                    _ => {}
                },
            ),
            cx.subscribe_in(
                &picker_search,
                window,
                |this: &mut Self, _input, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Change) {
                        // cmdk selects the first visible item whenever the query changes.
                        this.picker_highlight = (!this.picker_results(cx).is_empty()).then_some(0);
                        this.picker_scroll.scroll_to_item(0);
                        cx.notify();
                    }
                },
            ),
        ];
        // The same initial-focus contract as Rename Session: the name is focused and fully selected.
        name.update(cx, |input, cx| {
            input.focus(window, cx);
            let len = input.value().len();
            input.set_selected_range(0..len, cx);
        });
        Self {
            host,
            palette: config.palette,
            mode: config.mode,
            name,
            icon: resolve_icon_index(config.initial_icon.as_deref()),
            color: config
                .initial_color
                .unwrap_or_else(|| SPACE_EDITOR_COLORS[0].0.to_string()),
            picker_open: false,
            picker_search,
            picker_highlight: None,
            picker_scroll: ScrollHandle::new(),
            picker_trigger_bounds: Rc::new(Cell::new(None)),
            fit: ModalFit::new(),
            focus_handle,
            _subscriptions: subscriptions,
        }
    }

    fn trimmed_name(&self, cx: &App) -> String {
        self.name.read(cx).value().trim().to_string()
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.trimmed_name(cx);
        if name.is_empty() {
            return;
        }
        let command = SpaceEditorModalCommand::Submit {
            name,
            icon: SPACE_EDITOR_ICONS[self.icon].id.to_string(),
            color: self.color.clone(),
        };
        self.close_window_and_send(command, window, cx);
    }

    fn delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode != SpaceEditorMode::Edit {
            return;
        }
        self.close_window_and_send(SpaceEditorModalCommand::Delete, window, cx);
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_window_and_send(SpaceEditorModalCommand::Cancel, window, cx);
    }

    fn close_window_and_send(
        &mut self,
        command: SpaceEditorModalCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.remove_window();
        (self.host)(command, cx);
    }

    fn picker_query(&self, cx: &App) -> String {
        self.picker_search.read(cx).value().to_string()
    }

    /// The visible rows in cmdk order: every icon while the query is empty,
    /// otherwise the icons the scorer matches, best score first (stable).
    fn picker_results(&self, cx: &App) -> Vec<usize> {
        let query = self.picker_query(cx);
        if query.is_empty() {
            return (0..SPACE_EDITOR_ICONS.len()).collect();
        }
        let mut scored: Vec<(usize, f64)> = SPACE_EDITOR_ICONS
            .iter()
            .enumerate()
            .map(|(index, icon)| (index, command_score(icon.label, &query)))
            .filter(|(_, score)| *score > 0.0)
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.into_iter().map(|(index, _)| index).collect()
    }

    fn open_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.picker_open = true;
        self.picker_search.update(cx, |input, cx| {
            input.set_value("", window, cx);
        });
        self.picker_highlight = Some(0);
        self.picker_scroll.scroll_to_item(0);
        // The React popover suppresses its open auto-focus; focus stays out of the search box.
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn close_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.picker_open = false;
        self.picker_highlight = None;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn toggle_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.picker_open {
            self.close_picker(window, cx);
        } else {
            self.open_picker(window, cx);
        }
    }

    fn choose_icon(&mut self, icon: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.icon = icon;
        self.close_picker(window, cx);
    }

    fn choose_color(&mut self, color: &str, cx: &mut Context<Self>) {
        self.color = color.to_string();
        cx.notify();
    }

    fn move_picker_highlight(&mut self, delta: isize, cx: &mut Context<Self>) {
        let results = self.picker_results(cx);
        if results.is_empty() {
            return;
        }
        let current = self.picker_highlight.unwrap_or(0) as isize;
        let next = (current + delta).clamp(0, results.len() as isize - 1) as usize;
        self.picker_highlight = Some(next);
        self.picker_scroll.scroll_to_item(next);
        cx.notify();
    }

    fn picker_search_focused(&self, window: &Window, cx: &App) -> bool {
        self.picker_open
            && self
                .picker_search
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
    }

    fn on_input_escape(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        if self.picker_open {
            // `CommandInput` clears a non-empty query on Escape; an empty one lets the popover close.
            if self.picker_search_focused(window, cx) && !self.picker_query(cx).is_empty() {
                self.picker_search.update(cx, |input, cx| {
                    input.set_value("", window, cx);
                });
                self.picker_highlight = Some(0);
                self.picker_scroll.scroll_to_item(0);
                cx.notify();
            } else {
                self.close_picker(window, cx);
            }
            return;
        }
        self.cancel(window, cx);
    }

    fn on_input_enter(&mut self, _: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        if !self.picker_search_focused(window, cx) {
            cx.propagate();
            return;
        }
        let results = self.picker_results(cx);
        if let Some(icon) = self
            .picker_highlight
            .and_then(|highlight| results.get(highlight).copied())
        {
            self.choose_icon(icon, window, cx);
        }
    }

    fn on_input_move_up(&mut self, _: &MoveUp, window: &mut Window, cx: &mut Context<Self>) {
        if !self.picker_search_focused(window, cx) {
            cx.propagate();
            return;
        }
        self.move_picker_highlight(-1, cx);
    }

    fn on_input_move_down(&mut self, _: &MoveDown, window: &mut Window, cx: &mut Context<Self>) {
        if !self.picker_search_focused(window, cx) {
            cx.propagate();
            return;
        }
        self.move_picker_highlight(1, cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "escape" => {
                if self.picker_open {
                    self.close_picker(window, cx);
                } else {
                    self.cancel(window, cx);
                }
            }
            "enter" => {
                if event.is_held || self.picker_open {
                    return;
                }
                self.submit(window, cx);
            }
            _ => return,
        }
        cx.stop_propagation();
    }

    fn render_field_label(&self, text: &'static str, line_height: f32) -> AnyElement {
        let p = self.palette;
        div()
            .text_size(px(12.0))
            .line_height(px(line_height))
            .font_weight(FontWeight::MEDIUM)
            .text_color(hsla(p.muted))
            .child(text)
            .into_any_element()
    }

    fn render_glyph(&self, icon: usize, color: Rgba) -> AnyElement {
        div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .size(px(16.0))
            .child(modal_icon(SPACE_EDITOR_ICONS[icon].asset, 16.0, color))
            .into_any_element()
    }

    /// `.command-icon-picker-trigger` inside `.space-editor-form`: the raised
    /// 32px control showing the current glyph, its label and a chevron.
    fn render_icon_trigger(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        // `color-mix(in srgb, var(--app-muted) 84%, var(--app-foreground) 16%)`; `--app-muted`
        // is #717171 in the light theme, the palette's muted in the dark ones.
        let app_muted = if p.light { rgb(0x717171) } else { p.muted };
        let chevron = css_mix(app_muted, 0.84, p.foreground);
        h_flex()
            .id("space-editor-icon-trigger")
            .w_full()
            .h(px(MODAL_CONTROL_HEIGHT))
            .px(px(12.0))
            .gap(px(10.0))
            .items_center()
            .justify_between()
            .rounded(px(MODAL_RADIUS_CONTROL))
            .border_1()
            .border_color(hsla(p.hairline))
            .bg(hsla(p.raised))
            .text_size(px(14.0))
            .line_height(px(20.0))
            .text_color(hsla(p.foreground))
            .cursor_pointer()
            .hover(move |this| this.bg(hsla(p.raised_hover)))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.toggle_picker(window, cx);
            }))
            .child(
                h_flex()
                    .min_w_0()
                    .items_center()
                    .gap(px(8.0))
                    .child(self.render_glyph(self.icon, p.foreground))
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(SPACE_EDITOR_ICONS[self.icon].label),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .child(modal_icon(ICON_CHEVRON_DOWN, 16.0, chevron)),
            )
            .into_any_element()
    }

    /// The `CommandInput` row of the picker: an 8px-padded 32px input group
    /// with the search glyph on the right, replaced by a clear button once a
    /// query is typed.
    fn render_picker_search(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let has_query = !self.picker_query(cx).is_empty();
        // `--input` is white 15% (dark) or black 16% (light); the group fills with `bg-input/30`.
        let input_border = if p.light {
            modal_rgba(0x000000, 0.16)
        } else {
            modal_rgba(0xffffff, 0.15)
        };
        let input_fill = rgba_of(input_border, input_border.a * 0.30);
        // `--ghostex-tooltip-border` under the search row.
        let row_border = if p.light {
            modal_rgba(0x000000, 0.14)
        } else {
            modal_rgba(0xffffff, 0.12)
        };
        let focused = self
            .picker_search
            .read(cx)
            .focus_handle(cx)
            .is_focused(window);
        // Light mode keeps the search border subtle when focused (CDXC:DesignSystem 2026-09-14);
        // dark mode takes the `--ring` border of the focused input group.
        let focused_border = if p.light {
            modal_rgba(0x000000, 0.16)
        } else {
            rgb(0x737373)
        };
        div()
            .w_full()
            .p(px(8.0))
            .border_b_1()
            .border_color(hsla(row_border))
            .child(
                h_flex()
                    .w_full()
                    .h(px(32.0))
                    .items_center()
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(hsla(if focused {
                        focused_border
                    } else {
                        input_border
                    }))
                    .bg(hsla(input_fill))
                    .child(
                        div().flex_1().min_w_0().pl(px(10.0)).child(
                            Input::new(&self.picker_search)
                                .with_size(ComponentSize::Small)
                                .appearance(false)
                                .bordered(false)
                                .focus_bordered(false)
                                .w_full()
                                .px(px(0.0))
                                .py(px(0.0))
                                .text_size(px(13.0))
                                .text_color(hsla(p.foreground)),
                        ),
                    )
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .items_center()
                            // `pr-3`, pulled back by `has-[>button]:-mr-1` once the clear button shows.
                            .pr(px(if has_query { 8.0 } else { 12.0 }))
                            .child(if has_query {
                                div()
                                    .id("space-editor-icon-search-clear")
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .size(px(24.0))
                                    .cursor_pointer()
                                    .text_color(hsla(p.muted))
                                    .hover(move |this| this.text_color(hsla(p.foreground)))
                                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                        this.picker_search.update(cx, |input, cx| {
                                            input.set_value("", window, cx);
                                            input.focus(window, cx);
                                        });
                                        this.picker_highlight = Some(0);
                                        this.picker_scroll.scroll_to_item(0);
                                        cx.notify();
                                    }))
                                    .child(modal_icon(ICON_CLEAR, 16.0, p.muted))
                                    .into_any_element()
                            } else {
                                div()
                                    .opacity(0.5)
                                    .child(modal_icon(ICON_SEARCH, 16.0, p.muted))
                                    .into_any_element()
                            }),
                    ),
            )
            .into_any_element()
    }

    /// The icon popover: a `.searchable-dropdown` anchored to the trigger,
    /// flipped above it when the space below is too short, with the scrolling
    /// list capped like the React `[data-slot='command-list']`.
    fn render_icon_picker(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.picker_open {
            return None;
        }
        let trigger = self.picker_trigger_bounds.get()?;
        let p = self.palette;
        let results = self.picker_results(cx);
        let highlight = self.picker_highlight;
        let viewport = window.viewport_size();
        let list_natural = if results.is_empty() {
            PICKER_EMPTY_HEIGHT + PICKER_LIST_PADDING * 2.0
        } else {
            results.len() as f32 * PICKER_ROW_HEIGHT + PICKER_LIST_PADDING * 2.0
        };
        let natural = PICKER_SEARCH_BLOCK_HEIGHT + list_natural + 2.0;
        let below = f32::from(viewport.height)
            - f32::from(trigger.origin.y + trigger.size.height)
            - PICKER_SIDE_OFFSET
            - PICKER_WINDOW_MARGIN;
        let above = f32::from(trigger.origin.y) - PICKER_SIDE_OFFSET - PICKER_WINDOW_MARGIN;
        let (available, flip_above) = if natural <= below || below >= above {
            (below, false)
        } else {
            (above, true)
        };
        let list_cap = PICKER_LIST_MAX_HEIGHT.min(f32::from(viewport.height) * 0.5);
        let list_height = list_natural
            .min(list_cap)
            .min((available - PICKER_SEARCH_BLOCK_HEIGHT - 2.0).max(PICKER_ROW_HEIGHT));
        // `color-mix(in srgb, var(--app-card-active) 96%, var(--app-background) 4%)`; the
        // popover border resolves to `--ghostex-tooltip-border` in both appearances.
        let (card_active, app_background, menu_border, app_muted) = if p.light {
            (
                rgb(0xefefef),
                rgb(0xf4f4f5),
                modal_rgba(0x000000, 0.14),
                rgb(0x717171),
            )
        } else {
            (
                rgb(0x2a2a2a),
                rgb(0x0e0e0e),
                modal_rgba(0xffffff, 0.12),
                p.muted,
            )
        };
        let menu_background = css_mix(card_active, 0.96, app_background);
        // `--app-context-menu-hover-background` for the checked row.
        let checked_background = if p.light {
            rgb(0xefefef)
        } else {
            rgb(0x202020)
        };
        let highlighted_background = rgba_of(p.foreground, 0.08);
        let empty_color = css_mix(app_muted, 0.88, p.foreground);
        let current = self.icon;
        let rows: Vec<AnyElement> = results
            .iter()
            .enumerate()
            .map(|(position, icon)| {
                let icon = *icon;
                let checked = icon == current;
                let highlighted = highlight == Some(position);
                h_flex()
                    .id(("space-editor-icon-option", icon))
                    .w_full()
                    .min_h(px(PICKER_ROW_HEIGHT))
                    .px(px(10.0))
                    .py(px(6.0))
                    .gap(px(8.0))
                    .items_center()
                    .rounded(px(6.0))
                    .text_size(px(13.0))
                    .line_height(px(20.0))
                    .text_color(hsla(p.foreground))
                    .cursor_default()
                    .when(checked, |this| this.bg(hsla(checked_background)))
                    .when(highlighted, |this| this.bg(hsla(highlighted_background)))
                    .hover(move |this| this.bg(hsla(highlighted_background)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.choose_icon(icon, window, cx);
                    }))
                    .child(self.render_glyph(icon, p.foreground))
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(SPACE_EDITOR_ICONS[icon].label),
                    )
                    .when(checked, |this| {
                        this.child(div().ml_auto().flex_shrink_0().child(modal_icon(
                            ICON_CHECK,
                            16.0,
                            p.foreground,
                        )))
                    })
                    .into_any_element()
            })
            .collect();
        let list = if rows.is_empty() {
            div()
                .w_full()
                .p(px(10.0))
                .text_size(px(12.0))
                .line_height(px(17.4))
                .text_center()
                .text_color(hsla(empty_color))
                .child(NO_MATCHING_ICONS)
                .into_any_element()
        } else {
            v_flex()
                .id("space-editor-icon-list")
                .w_full()
                .max_h(px(list_height))
                .p(px(PICKER_LIST_PADDING))
                .overflow_y_scroll()
                .track_scroll(&self.picker_scroll)
                .children(rows)
                .into_any_element()
        };
        let position = if flip_above {
            point(trigger.origin.x, trigger.origin.y - px(PICKER_SIDE_OFFSET))
        } else {
            point(
                trigger.origin.x,
                trigger.origin.y + trigger.size.height + px(PICKER_SIDE_OFFSET),
            )
        };
        Some(
            deferred(
                anchored()
                    .position(position)
                    .anchor(if flip_above {
                        Anchor::BottomLeft
                    } else {
                        Anchor::TopLeft
                    })
                    .snap_to_window_with_margin(px(PICKER_WINDOW_MARGIN))
                    .child(
                        v_flex()
                            .id("space-editor-icon-picker")
                            .occlude()
                            .w(trigger.size.width)
                            .overflow_hidden()
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(hsla(menu_border))
                            .bg(hsla(menu_background))
                            .shadow(vec![BoxShadow {
                                color: hsla(modal_rgba(0x000000, 0.35)),
                                offset: point(px(0.0), px(12.0)),
                                blur_radius: px(28.0),
                                spread_radius: px(0.0),
                                inset: false,
                            }])
                            .on_mouse_down_out(cx.listener(
                                move |this, event: &MouseDownEvent, window, cx| {
                                    if trigger.contains(&event.position) {
                                        return;
                                    }
                                    this.close_picker(window, cx);
                                },
                            ))
                            .child(self.render_picker_search(window, cx))
                            .child(list),
                    ),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }

    /// `.space-editor-color-strip`: thirteen 22px round swatches, 8px apart;
    /// the selected one takes the foreground border and the 2px surface + 3px
    /// foreground double ring.
    fn render_color_strip(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let swatch_border = if p.light {
            modal_rgba(0x000000, 0.24)
        } else {
            modal_rgba(0xffffff, 0.16)
        };
        let selected_border = if p.light {
            rgb(0x262626)
        } else {
            rgb(0xffffff)
        };
        // The light override recolors only the border; the 3px ring stays `#fff` in both appearances.
        let selected_ring = rgb(0xffffff);
        h_flex()
            .w_full()
            .gap(px(8.0))
            .children(
                SPACE_EDITOR_COLORS
                    .iter()
                    .enumerate()
                    .map(|(index, (color, _label))| {
                        let selected = self.color.eq_ignore_ascii_case(color)
                            || (is_space_gray(color) && is_space_gray(&self.color));
                        let fill = parse_hex_color(space_display_color(color, p.light))
                            .unwrap_or(p.raised);
                        div()
                            .id(("space-editor-color-swatch", index))
                            .flex_shrink_0()
                            .size(px(22.0))
                            .rounded_full()
                            .border_1()
                            .border_color(hsla(if selected {
                                selected_border
                            } else {
                                swatch_border
                            }))
                            .bg(hsla(fill))
                            .cursor_pointer()
                            .when(selected, |this| {
                                this.shadow(vec![
                                    BoxShadow {
                                        color: hsla(selected_ring),
                                        offset: point(px(0.0), px(0.0)),
                                        blur_radius: px(0.0),
                                        spread_radius: px(3.0),
                                        inset: false,
                                    },
                                    BoxShadow {
                                        color: hsla(p.solid_surface),
                                        offset: point(px(0.0), px(0.0)),
                                        blur_radius: px(0.0),
                                        spread_radius: px(2.0),
                                        inset: false,
                                    },
                                ])
                            })
                            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                                this.choose_color(color, cx);
                            }))
                    }),
            )
            .into_any_element()
    }

    /// The three fields as one body column at the form's 20px rhythm, so the
    /// window fit measures header and body the way `modal_shell` expects.
    fn render_body(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        v_flex()
            .w_full()
            .gap(px(MODAL_SECTION_GAP))
            .children([
                v_flex()
                    .w_full()
                    .gap(px(12.0))
                    .child(self.render_field_label(NAME_LABEL, 16.5))
                    .child(modal_text_input(&p, &self.name, false, window, cx))
                    .into_any_element(),
                v_flex()
                    .w_full()
                    .gap(px(6.0))
                    .child(self.render_field_label(ICON_LABEL, 17.14))
                    .child(
                        h_flex()
                            .w_full()
                            .on_children_prepainted(capture_child_bounds(
                                self.picker_trigger_bounds.clone(),
                                0,
                            ))
                            .child(self.render_icon_trigger(cx)),
                    )
                    .into_any_element(),
                v_flex()
                    .w_full()
                    .gap(px(12.0))
                    .child(self.render_field_label(COLOR_LABEL, 17.14))
                    .child(self.render_color_strip(cx))
                    .into_any_element(),
            ])
            .into_any_element()
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let mut buttons = vec![modal_action_button(
            &p,
            "space-editor-cancel",
            CANCEL,
            None,
            ModalButtonTone::Neutral,
            false,
            |this, window, cx| this.cancel(window, cx),
            cx,
        )];
        if self.mode == SpaceEditorMode::Edit {
            buttons.push(modal_action_button(
                &p,
                "space-editor-delete",
                DELETE,
                None,
                ModalButtonTone::Danger,
                false,
                |this, window, cx| this.delete(window, cx),
                cx,
            ));
        }
        buttons.push(modal_action_button(
            &p,
            "space-editor-submit",
            if self.mode == SpaceEditorMode::Edit {
                SAVE
            } else {
                CREATE
            },
            None,
            ModalButtonTone::Neutral,
            self.trimmed_name(cx).is_empty(),
            |this, window, cx| this.submit(window, cx),
            cx,
        ));
        modal_footer(buttons)
    }
}

impl Render for GpuiSpaceEditorModalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let title = if self.mode == SpaceEditorMode::Edit {
            TITLE_EDIT
        } else {
            TITLE_CREATE
        };
        let content = vec![
            modal_header(&p, title, Some(DESCRIPTION)),
            self.render_body(window, cx),
        ];
        let footer = self.render_footer(cx);
        let picker = self.render_icon_picker(window, cx);
        modal_shell(
            &p,
            "ghostex-gpui-space-editor-modal",
            &self.focus_handle,
            &self.fit,
            Self::on_key_down,
            content,
            footer,
            picker,
            cx,
        )
        .capture_action(cx.listener(Self::on_input_escape))
        .capture_action(cx.listener(Self::on_input_enter))
        .capture_action(cx.listener(Self::on_input_move_up))
        .capture_action(cx.listener(Self::on_input_move_down))
    }
}

/// `resolveSpaceEditorIcon`: an unknown or missing icon id falls back to the default Space icon.
fn resolve_icon_index(icon: Option<&str>) -> usize {
    icon.and_then(|icon| SPACE_EDITOR_ICONS.iter().position(|entry| entry.id == icon))
        .or_else(|| {
            SPACE_EDITOR_ICONS
                .iter()
                .position(|entry| entry.id == DEFAULT_ICON_ID)
        })
        .unwrap_or(0)
}

fn parse_hex_color(value: &str) -> Option<Rgba> {
    let hex = value.trim().strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    u32::from_str_radix(hex, 16).ok().map(rgb)
}

// cmdk's `command-score` (node_modules/cmdk/dist/command-score.js), ported so
// the native picker ranks icons exactly like the React one.
const SCORE_CONTINUE_MATCH: f64 = 1.0;
const SCORE_SPACE_WORD_JUMP: f64 = 0.9;
const SCORE_NON_SPACE_WORD_JUMP: f64 = 0.8;
const SCORE_CHARACTER_JUMP: f64 = 0.17;
const SCORE_TRANSPOSITION: f64 = 0.1;
const PENALTY_SKIPPED: f64 = 0.999;
const PENALTY_CASE_MISMATCH: f64 = 0.9999;
const PENALTY_NOT_COMPLETE: f64 = 0.99;

fn is_gap(ch: char) -> bool {
    matches!(
        ch,
        '\\' | '/' | '_' | '+' | '.' | '#' | '"' | '@' | '[' | '(' | '{' | '&'
    )
}

fn is_space(ch: char) -> bool {
    ch.is_whitespace() || ch == '-'
}

fn format_input(value: &str) -> Vec<char> {
    value
        .to_lowercase()
        .chars()
        .map(|ch| if is_space(ch) { ' ' } else { ch })
        .collect()
}

struct CommandScorer<'a> {
    string: &'a [char],
    abbreviation: &'a [char],
    lower_string: &'a [char],
    lower_abbreviation: &'a [char],
    memo: HashMap<(usize, usize), f64>,
}

impl CommandScorer<'_> {
    fn score(&mut self, string_index: usize, abbreviation_index: usize) -> f64 {
        if abbreviation_index == self.abbreviation.len() {
            return if string_index == self.string.len() {
                SCORE_CONTINUE_MATCH
            } else {
                PENALTY_NOT_COMPLETE
            };
        }
        if let Some(score) = self.memo.get(&(string_index, abbreviation_index)) {
            return *score;
        }
        let abbreviation_char = self.lower_abbreviation[abbreviation_index];
        let mut high_score = 0.0_f64;
        let mut index = index_of(self.lower_string, abbreviation_char, string_index);
        while let Some(found) = index {
            let mut score = self.score(found + 1, abbreviation_index + 1);
            if score > high_score {
                if found == string_index {
                    score *= SCORE_CONTINUE_MATCH;
                } else if found > 0 && is_gap(self.string[found - 1]) {
                    score *= SCORE_NON_SPACE_WORD_JUMP;
                    let word_breaks = self.string[string_index..found - 1]
                        .iter()
                        .filter(|ch| is_gap(**ch))
                        .count();
                    if word_breaks > 0 && string_index > 0 {
                        score *= PENALTY_SKIPPED.powi(word_breaks as i32);
                    }
                } else if found > 0 && is_space(self.string[found - 1]) {
                    score *= SCORE_SPACE_WORD_JUMP;
                    let space_breaks = self.string[string_index..found - 1]
                        .iter()
                        .filter(|ch| is_space(**ch))
                        .count();
                    if space_breaks > 0 && string_index > 0 {
                        score *= PENALTY_SKIPPED.powi(space_breaks as i32);
                    }
                } else {
                    score *= SCORE_CHARACTER_JUMP;
                    if string_index > 0 {
                        score *= PENALTY_SKIPPED.powi((found - string_index) as i32);
                    }
                }
                if self.string[found] != self.abbreviation[abbreviation_index] {
                    score *= PENALTY_CASE_MISMATCH;
                }
            }
            let previous = found.checked_sub(1).map(|i| self.lower_string[i]);
            let next_abbreviation = self.lower_abbreviation.get(abbreviation_index + 1).copied();
            if (score < SCORE_TRANSPOSITION && previous.is_some() && previous == next_abbreviation)
                || (next_abbreviation == Some(abbreviation_char)
                    && previous != Some(abbreviation_char))
            {
                let transposed = self.score(found + 1, abbreviation_index + 2);
                if transposed * SCORE_TRANSPOSITION > score {
                    score = transposed * SCORE_TRANSPOSITION;
                }
            }
            if score > high_score {
                high_score = score;
            }
            index = index_of(self.lower_string, abbreviation_char, found + 1);
        }
        self.memo
            .insert((string_index, abbreviation_index), high_score);
        high_score
    }
}

fn index_of(haystack: &[char], needle: char, from: usize) -> Option<usize> {
    haystack
        .iter()
        .enumerate()
        .skip(from)
        .find(|(_, ch)| **ch == needle)
        .map(|(index, _)| index)
}

/// `commandScore(string, abbreviation)` without aliases.
pub(crate) fn command_score(string: &str, abbreviation: &str) -> f64 {
    let string_chars: Vec<char> = string.chars().collect();
    let abbreviation_chars: Vec<char> = abbreviation.chars().collect();
    let lower_string = format_input(string);
    let lower_abbreviation = format_input(abbreviation);
    if lower_string.len() != string_chars.len()
        || lower_abbreviation.len() != abbreviation_chars.len()
    {
        // A case mapping that changes the length has no cmdk equivalent worth mirroring; fall back to a plain substring match.
        return if lower_string
            .iter()
            .collect::<String>()
            .contains(&lower_abbreviation.iter().collect::<String>())
        {
            SCORE_CHARACTER_JUMP
        } else {
            0.0
        };
    }
    let mut scorer = CommandScorer {
        string: &string_chars,
        abbreviation: &abbreviation_chars,
        lower_string: &lower_string,
        lower_abbreviation: &lower_abbreviation,
        memo: HashMap::new(),
    };
    scorer.score(0, 0)
}
