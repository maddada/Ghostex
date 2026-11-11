//! The view panel with no view in it: a card per view the project could open, grouped built-ins,
//! then your views and extensions.

use gpui::AnyElement;
use gpui::FontWeight;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::ParentElement as _;
use gpui::Styled as _;
use gpui::Window;
use gpui::div;
use gpui::img;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui_component::h_flex;
use gpui_component::v_flex;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

/// Which of the picker's two groups a view belongs to. Ruling 13: the picker lists everything, so
/// the groups are only how it is read, never what it leaves out.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ViewPickerGroup {
    BuiltIn,
    Extension,
}

impl ViewPickerGroup {
    fn of(mode: TitlebarMode) -> Self {
        match mode {
            mode if mode.is_addon_view() => Self::Extension,
            _ => Self::BuiltIn,
        }
    }

    /// The label above the group, or nothing for the built-ins, which open the list.
    fn label(self) -> Option<&'static str> {
        match self {
            Self::BuiltIn => None,
            Self::Extension => Some("YOUR VIEWS AND EXTENSIONS"),
        }
    }
}

/// What a card says under a view's name: the line it shows, and the whole text its hover shows
/// when that line is only part of it. Built-ins say what the view is for in the product's own
/// words; a website view shows the first sentence of its catalog description; a custom view says
/// which address it opens. An installed extension's line comes from its manifest instead (see
/// `render_view_picker_card`).
///
/// CDXC:Workarea 2026-09-24 DECISION:
/// User: no card shows more than one line of description; hover shows the full text. Built-ins keep sentences short enough to fit, Linear, Jira, and GitHub keep their shortened sentences, and custom views read "Opens ticktick.com/tasks/..." without the `http(s)://` or `www.` prefix. Supersedes the 2026-09-23 rule that lines never end in an ellipsis.
fn view_picker_description(mode: TitlebarMode) -> (gpui::SharedString, Option<gpui::SharedString>) {
    if let Some(provider) = mode.website_provider() {
        let line = match provider.id.as_str() {
            "linear" => "Your team's issues and projects.",
            "jira" => "The team board beside your work.",
            "github" => "Opens from the project's origin.",
            _ => provider
                .description
                .split_inclusive(". ")
                .next()
                .unwrap_or_default()
                .trim(),
        };
        return (line.into(), Some(provider.description.clone().into()));
    }
    let line = match mode {
        mode if mode.is_storybook() => "Annotate your project’s components.",
        TitlebarMode::Source => "Edit and search project files.",
        TitlebarMode::Browser => "Open a local app or any website.",
        TitlebarMode::Kanban => "See what's ready, who's on what and what's blocked.",
        TitlebarMode::Automate => "Run repeatable project routines.",
        TitlebarMode::Terminal => "Shell commands beside your agents..",
        TitlebarMode::Manage => "Notes, plans and reference files.",
        TitlebarMode::Changes => "Review the diff, comment for the agent and commit.",
        TitlebarMode::BotFeed => "Every Hermes cron run, one channel per job.",
        TitlebarMode::Work => "Your tickets, issues and PRs.",
        TitlebarMode::SideChat => "Ask about a session without interrupting it.",
        TitlebarMode::Extension(id) => {
            let Some(view) = gpui_custom_view(id).filter(|view| !view.url.is_empty()) else {
                return ("".into(), None);
            };
            let url = view.url.as_str();
            let url = url
                .strip_prefix("https://")
                .or_else(|| url.strip_prefix("http://"))
                .unwrap_or(url);
            let url = url
                .strip_prefix("www.")
                .unwrap_or(url)
                .trim_end_matches('/');
            let text = gpui::SharedString::from(format!("Opens {url}"));
            return (text.clone(), Some(text));
        }
        TitlebarMode::Agents => "",
    };
    (line.into(), None)
}

impl GhostexGpuiApp {
    /// CDXC:Workarea 2026-09-20 DECISION:
    /// User (screen 02, ruling 13): with the panel open and nothing selected, it shows a picker of
    /// every view this project can open — built-ins, then your views and extensions — with single-letter shortcuts while it has focus and a link to the Settings
    /// scope editor at the bottom. Only views the project's own scope hides are missing from it.
    pub(crate) fn render_view_picker(&mut self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let modes = self.view_picker_entries();
        // CDXC:Workarea 2026-09-22 WHY:
        // The list is sorted by the user's view order, and a built-in that order has never seen
        // (a Terminal view added after the order was saved) sorts to the end, behind the
        // extensions. The groups are read by kind, not by position, so a built-in always lands in
        // the built-in grid and the label only ever opens the extensions.
        let mut groups: Vec<(ViewPickerGroup, Vec<TitlebarModeSwitcherItem>)> = Vec::new();
        for group in [ViewPickerGroup::BuiltIn, ViewPickerGroup::Extension] {
            let items = modes
                .iter()
                .copied()
                .filter(|item| ViewPickerGroup::of(item.mode) == group)
                .collect::<Vec<_>>();
            if !items.is_empty() {
                groups.push((group, items));
            }
        }
        let mut body = v_flex()
            .w_full()
            .flex_shrink_0()
            .max_w(px(VIEW_PICKER_CONTENT_WIDTH))
            .items_center()
            .child(
                div()
                    .text_size(px(15.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(titlebar_active_text_color())
                    .child("Open a view"),
            )
            .child(
                div()
                    .mt(px(4.0))
                    .mb(px(14.0))
                    .text_size(px(12.5))
                    .text_color(titlebar_inactive_text_color())
                    .child(
                        "Views belong to this project, so they stay put when you switch sessions.",
                    ),
            );
        for (group, items) in groups {
            if let Some(label) = group.label() {
                body = body.child(
                    div()
                        .w_full()
                        .mt(px(12.0))
                        .mb(px(6.0))
                        .text_size(px(11.0))
                        .text_color(titlebar_disabled_text_color())
                        .child(label),
                );
            }
            for pair in items.chunks(2) {
                let mut row = h_flex()
                    .w_full()
                    .mb(px(VIEW_PICKER_CARD_GAP))
                    .items_stretch()
                    .gap(px(VIEW_PICKER_CARD_GAP));
                for item in pair {
                    row = row.child(self.render_view_picker_card(*item, cx));
                }
                if pair.len() == 1 {
                    // The odd card keeps its column instead of stretching across both. The spacer
                    // carries the card's own padding and border, or the card would be that much
                    // wider than its column and wrap its text differently from how it was measured.
                    row = row.child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .px(px(14.0))
                            .border_1()
                            .border_color(gpui::transparent_black()),
                    );
                }
                body = body.child(row);
            }
        }
        // The cards scroll in their own region so the manage link stays pinned to the panel's
        // bottom edge instead of trailing the last card.
        let cards = v_flex()
            .id("ghostex-gpui-view-picker-cards")
            .w_full()
            .flex_1()
            .min_h_0()
            .items_center()
            .justify_center()
            .overflow_y_scroll()
            .track_scroll(&self.view_picker_scroll)
            .child(body);
        v_flex()
            .id("ghostex-gpui-view-picker")
            .size_full()
            .min_w_0()
            .min_h_0()
            .items_center()
            .p(px(24.0))
            .bg(glass_clear(project_editor_shell_background_color()))
            .font_family(crate::ui_fonts::UI_FONT)
            .text_color(titlebar_text_color())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseDownEvent, _window, cx| {
                    this.focus_view_picker(cx);
                }),
            )
            .child(cards)
            .child(self.render_view_picker_manage_link(cx))
            .into_any_element()
    }

    /// Every view the picker offers: the scoped list, minus `Agents`, which is the panel being
    /// closed rather than a view, and minus Claude, which is the sidebar's Claude button.
    ///
    /// CDXC:Extensions 2026-11-04 DECISION:
    /// User, of the picker's "YOUR VIEWS AND EXTENSIONS" section showing only "Claude / Opens claude.ai": "remove this, it's not needed". Claude is turned on and off from its button in the sidebar's bottom bar (native_sidebar/navigation.rs), so the picker no longer offers it, and its section heading goes with it when nothing else is in that group.
    pub(crate) fn view_picker_entries(&self) -> Vec<TitlebarModeSwitcherItem> {
        // Grok, like Claude, is its own sidebar button (claude_web/place.rs).
        self.titlebar_mode_switcher_items()
            .into_iter()
            .filter(|item| item.mode != TitlebarMode::Agents && !item.mode.is_web_place())
            .collect()
    }

    fn render_view_picker_card(
        &self,
        item: TitlebarModeSwitcherItem,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let mode = item.mode;
        let available = item.is_available;
        // CDXC:Extensions 2026-09-23 DECISION:
        // User: each extension card in the view picker shows the extension's own icon and manifest description, kept to one line; hovering the card shows the whole description.
        let extension = match mode {
            TitlebarMode::Extension(id) if mode.is_addon_view() => self
                .extensions_snapshot
                .installed
                .get(id.as_str())
                .filter(|extension| extension.enabled),
            _ => None,
        };
        let extension_description = extension
            .map(|extension| gpui::SharedString::from(extension.description.clone()))
            .filter(|description| !description.is_empty());
        let (description, full_description) = match extension_description {
            Some(description) => (description.clone(), Some(description)),
            None => view_picker_description(mode),
        };
        let icon = match extension {
            Some(extension) => img(extension.icon_image.clone())
                .size(px(15.0))
                .flex_shrink_0()
                .into_any_element(),
            None => {
                titlebar_svg_icon(mode.tab_icon(), 15.0, titlebar_icon_color()).into_any_element()
            }
        };
        let shortcut = view_picker_shortcut(mode);
        let dashed = mode.is_addon_view();
        let (fill, border, hover) = view_picker_card_colors();
        div()
            .id(format!(
                "ghostex-gpui-view-picker-card-{}",
                mode.element_slug()
            ))
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .gap(px(3.0))
            .px(px(14.0))
            .py(px(12.0))
            .rounded(px(12.0))
            .border_1()
            .when(dashed, |this| this.border_dashed())
            .border_color(border)
            .bg(fill)
            .cursor_default()
            .when(!available, |this| this.opacity(0.5))
            .when_some(full_description, |this, description| {
                this.tooltip(move |window, cx| titlebar_tooltip(description.clone(), window, cx))
            })
            .when(available, |this| {
                this.hover(move |this| this.bg(hover)).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.open_view_tab(mode, window, cx);
                    }),
                )
            })
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .gap(px(8.0))
                    .child(icon)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(titlebar_active_text_color())
                            .child(mode.tab_label()),
                    ),
            )
            .when(!description.is_empty(), |this| {
                this.child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(titlebar_inactive_text_color())
                        .child(description),
                )
            })
            .when_some(item.disabled_reason, |this, reason| {
                this.child(
                    div()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(titlebar_disabled_text_color())
                        .child(reason),
                )
            })
            .when_some(shortcut, |this, shortcut| {
                this.child(
                    div()
                        .absolute()
                        .top(px(12.0))
                        .right(px(12.0))
                        .text_size(px(11.0))
                        .font_family(ACCOUNT_INDICATOR_FONT_FAMILY)
                        .text_color(titlebar_disabled_text_color())
                        .child(shortcut.to_string()),
                )
            })
    }

    fn render_view_picker_manage_link(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
            .id("ghostex-gpui-view-picker-manage")
            .flex_shrink_0()
            .mt(px(14.0))
            .text_size(px(12.0))
            .text_color(titlebar_inactive_text_color())
            .cursor_default()
            .hover(|this| this.text_color(titlebar_active_text_color()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.open_gpui_settings_extensions_page(Some(window), cx);
                }),
            )
            .child("Manage your extensions")
    }

    /// CDXC:Hotkeys 2026-09-20 DECISION:
    /// User (screen 02): single-letter shortcuts open a view while the picker has focus. They are
    /// only live there, so they cannot collide with anything typed in a terminal, a chat or a page.
    pub(crate) fn open_view_from_view_picker_keystroke(
        &mut self,
        keystroke: &gpui::Keystroke,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self.view_picker_open()
            || self.shell_focus != ShellFocusTarget::ProjectEditorSurface(TitlebarMode::Agents)
        {
            return false;
        }
        let modifiers = keystroke.modifiers;
        if modifiers.control || modifiers.alt || modifiers.platform || modifiers.function {
            return false;
        }
        let Some(letter) = keystroke
            .key
            .chars()
            .next()
            .filter(|_| keystroke.key.chars().count() == 1)
            .map(|letter| letter.to_ascii_uppercase())
        else {
            return false;
        };
        let Some(mode) = self
            .view_picker_entries()
            .into_iter()
            .find(|item| item.is_available && view_picker_shortcut(item.mode) == Some(letter))
            .map(|item| item.mode)
        else {
            return false;
        };
        self.open_view_tab(mode, window, cx)
    }
}

/// A picker card's fill, border and hover fill.
///
/// CDXC:Theming 2026-09-23 DECISION:
/// User: "make this also glassy matching the main area glass bg look and same for the cards". Under window glass the picker paints no page fill of its own, so the work area's frosted glass shows through, and its cards are the same flat ink wash as the frosted view cards (sleeping and setup cards) with a faint ink border and a stronger wash on hover. The opaque window keeps its solid menu-toned cards.
fn view_picker_card_colors() -> (gpui::Hsla, gpui::Hsla, gpui::Hsla) {
    if !window_glass_active() {
        return (
            titlebar_popup_menu_background(),
            workspace_pane_border_color().into(),
            titlebar_active_segment_color().into(),
        );
    }
    let ink = gpui::Hsla::from(chrome_ink());
    let light = chrome_uses_light_appearance();
    (
        ink.opacity(if light { 0.04 } else { 0.06 }),
        ink.opacity(0.08),
        ink.opacity(if light { 0.07 } else { 0.10 }),
    )
}

/// The letter a built-in view answers to in the picker. Extension, custom and Ghostex views have
/// none, because their names are not the app's to reserve a key for.
fn view_picker_shortcut(mode: TitlebarMode) -> Option<char> {
    match mode {
        mode if mode.is_storybook() => Some('S'),
        TitlebarMode::Source => Some('C'),
        TitlebarMode::Browser => Some('B'),
        TitlebarMode::Kanban => Some('K'),
        TitlebarMode::Automate => Some('U'),
        TitlebarMode::Manage => Some('D'),
        TitlebarMode::Changes => Some('G'),
        TitlebarMode::Terminal => Some('T'),
        TitlebarMode::Work => Some('W'),
        TitlebarMode::Agents
        | TitlebarMode::BotFeed
        | TitlebarMode::SideChat
        | TitlebarMode::Extension(_) => None,
    }
}
