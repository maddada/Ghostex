//! `SettingsSection`, `SettingRow`, `SettingsListItem` and the row's label actions (the modified
//! asterisk, the advanced arrow, the info tooltip) from fields.tsx, with the grouped-list geometry
//! of `.settings-list-*` in packages/core-ui/styles.css.
use super::super::super::native_modal_kit::*;
use super::super::palette::SettingsPalette;
use super::super::store::SettingsValues;
use super::{SettingsPage, icon};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, Div, ElementId, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, Stateful, StatefulInteractiveElement as _, Styled as _,
    Window, div, px,
};
use gpui_component::tooltip::{ManagedTooltipExt as _, ManagedTooltipPlacement, Tooltip};
use gpui_component::{h_flex, v_flex};
use std::rc::Rc;
use std::time::Duration;

/// `.settings-list-row { min-height: 3.25rem; padding: 0.625rem 1.25rem; gap: 1.5rem }`.
pub(crate) const ROW_MIN_HEIGHT: f32 = 52.0;
pub(crate) const ROW_PADDING_X: f32 = 20.0;
pub(crate) const ROW_PADDING_Y: f32 = 10.0;
pub(crate) const ROW_GAP: f32 = 24.0;
/// `--settings-control-height`.
pub(crate) const CONTROL_HEIGHT: f32 = 32.0;
/// `--settings-select-width`.
pub(crate) const SELECT_WIDTH: f32 = 192.0;
/// `.settings-control-lane` and `.settings-slider-number`: `width: 17rem`.
pub(crate) const CONTROL_LANE_WIDTH: f32 = 272.0;
/// `TooltipProvider delayDuration={300}`.
pub(crate) const TOOLTIP_DELAY: Duration = Duration::from_millis(300);
/// `MODIFIED_SETTING_TOOLTIP`.
pub(crate) const MODIFIED_SETTING_TOOLTIP: &str = "Modified Setting.\n \nClick to Reset to Default";

/// A click handler on a page.
pub(crate) type PageAction<V> = Rc<dyn Fn(&mut V, &mut Window, &mut Context<V>)>;

/// What a `SettingRow` shows around its control.
#[derive(Clone, Default)]
pub(crate) struct RowSpec {
    pub(crate) label: SharedString,
    pub(crate) description: Option<SharedString>,
    pub(crate) subtitle: Option<SharedString>,
    pub(crate) experimental: bool,
    pub(crate) disabled_reason: Option<SharedString>,
    /// CDXC:Settings 2026-09-12 DECISION:
    /// User: settings that cascade from a setting above them start their name with four spaces and the ↳ glyph.
    /// A setting that only applies while the one above is on: `    ↳ ` before the label.
    pub(crate) dependent: bool,
    /// The control takes its own full-width line under the label.
    pub(crate) wide: bool,
    pub(crate) badge: Option<SharedString>,
    /// Muted text after the label (`labelAddon`: the Theme colour row's colour name).
    pub(crate) readout: Option<SharedString>,
    /// The setting differs from its default: the asterisk shows and resets it.
    pub(crate) modified: bool,
    /// Side padding when the row sits directly in a list whose neighbours use a narrower inset
    /// than `ROW_PADDING_X` (the Accounts list: its rows and expanded panel use 16px).
    pub(crate) inset_x: Option<f32>,
    /// An icon tile in front of the label (the Theme page's rows; CDXC:Theming 2026-10-16 in
    /// tabs/theme/mod.rs). No other page sets it.
    pub(crate) icon: Option<&'static str>,
    /// The modified asterisk sits right after the label instead of hanging in the gutter to its
    /// left, where an icon tile would crowd it (the Theme page's rows and their sub-rows).
    pub(crate) marker_after_label: bool,
}

impl RowSpec {
    pub(crate) fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            ..Self::default()
        }
    }

    pub(crate) fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub(crate) fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub(crate) fn dependent(mut self) -> Self {
        self.dependent = true;
        self
    }

    pub(crate) fn readout(mut self, readout: impl Into<SharedString>) -> Self {
        self.readout = Some(readout.into());
        self
    }

    pub(crate) fn wide(mut self) -> Self {
        self.wide = true;
        self
    }

    pub(crate) fn icon(mut self, icon: &'static str) -> Self {
        self.icon = Some(icon);
        self.marker_after_label = true;
        self
    }

    pub(crate) fn marker_after_label(mut self) -> Self {
        self.marker_after_label = true;
        self
    }

    pub(crate) fn inset_x(mut self, inset: f32) -> Self {
        self.inset_x = Some(inset);
        self
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn experimental(mut self) -> Self {
        self.experimental = true;
        self
    }

    pub(crate) fn disabled_reason(mut self, reason: Option<SharedString>) -> Self {
        self.disabled_reason = reason;
        self
    }

    pub(crate) fn modified(mut self, modified: bool) -> Self {
        self.modified = modified;
        self
    }

    /// `getSettingModificationProps(key)`: the modified asterisk and the "Not recommended" label
    /// of a setting. (The advanced arrow marker is gone: CDXC:Settings 2026-10-29 in search.rs.)
    pub(crate) fn keyed(mut self, values: &SettingsValues, key: &str) -> Self {
        let catalog = super::super::catalog::settings_catalog();
        if catalog.is_not_recommended(key) {
            self.badge = Some("Not recommended".into());
        }
        let modified = values.is_modified(key);
        self.modified(modified)
    }

    /// `[description, subtitle].filter(Boolean).join('\n\n')`.
    fn tooltip_text(&self) -> Option<SharedString> {
        let parts: Vec<&str> = [self.description.as_ref(), self.subtitle.as_ref()]
            .into_iter()
            .flatten()
            .map(|text| text.as_ref())
            .filter(|text| !text.is_empty())
            .collect();
        (!parts.is_empty()).then(|| SharedString::from(parts.join("\n\n")))
    }
}

/// The reset handler of a keyed row: back to `DEFAULT_ghostex_SETTINGS[key]`.
pub(crate) fn reset_key<V: SettingsPage>(key: &'static str) -> PageAction<V> {
    Rc::new(move |page: &mut V, _window, cx| {
        let store = page.settings_store().clone();
        store.update(cx, |store, cx| store.reset_setting(key, cx));
    })
}

pub(crate) fn settings_icon(path: &'static str, size: f32, color: gpui::Rgba) -> gpui::Svg {
    modal_icon(path, size, color)
}

/// Settings' tooltip: the app's plain tooltip bubble (`titlebar_tooltip`, CDXC:Tooltips 2026-10-29
/// DECISION in helpers/titlebar/tooltips_and_popup_geometry.rs), replacing the near-black React
/// `TooltipContent` port. `center` centres its text, `max_width` caps its width, and the
/// placement's offset (`sideOffset`) is the bubble's margin toward its trigger.
fn settings_tooltip_bubble(
    text: SharedString,
    placement: Option<(ManagedTooltipPlacement, f32)>,
    max_width: Option<f32>,
    center: bool,
    window: &mut Window,
    cx: &mut gpui::App,
) -> gpui::AnyView {
    Tooltip::new(text)
        .when_some(max_width, |this, width| this.max_w(px(width)))
        .when(center, |this| this.text_center())
        .when_some(placement, |this, (placement, offset)| match placement {
            ManagedTooltipPlacement::Right | ManagedTooltipPlacement::Left => this.mx(px(offset)),
            _ => this.my(px(offset)),
        })
        .build(window, cx)
}

/// A Settings tooltip (the React `Tooltip` / `AppTooltip` surface) for `.tooltip(..)`.
pub(crate) fn tooltip_text(
    text: impl Into<SharedString>,
) -> impl Fn(&mut Window, &mut gpui::App) -> gpui::AnyView + 'static {
    let text: SharedString = text.into();
    move |window, cx| settings_tooltip_bubble(text.clone(), None, None, false, window, cx)
}

/// The widest a wrapped description tooltip gets, and the room it leaves at the window's edge.
const WRAPPED_TOOLTIP_MAX_WIDTH: f32 = 320.0;
const WRAPPED_TOOLTIP_WINDOW_MARGIN: f32 = 24.0;
/// How far below the pointer a wrapped tooltip starts, past the pointer's arrow (the same drop as
/// `list_row_tooltip`).
const WRAPPED_TOOLTIP_DROP: f32 = 20.0;

/// The Settings tooltip for text cut off with an ellipsis (a card's description): the full text
/// wrapped at a readable width, never wider than the window, dropped below the pointer.
///
/// CDXC:Tooltips 2026-10-10 DECISION:
/// User, of a truncated extension card description: "i can't read the rest of the text here, make hovering over it show tooltip that wraps at sensible spot (not too wide!) with full text". A description cut off by an ellipsis shows its full text on hover, wrapped at about 320px.
pub(crate) fn wrapped_tooltip_text(
    text: impl Into<SharedString>,
) -> impl Fn(&mut Window, &mut gpui::App) -> gpui::AnyView + 'static {
    let text: SharedString = text.into();
    move |window, cx| {
        let room = f32::from(window.viewport_size().width) - WRAPPED_TOOLTIP_WINDOW_MARGIN;
        settings_tooltip_bubble(
            text.clone(),
            Some((ManagedTooltipPlacement::Below, WRAPPED_TOOLTIP_DROP)),
            Some(WRAPPED_TOOLTIP_MAX_WIDTH.min(room.max(0.0))),
            false,
            window,
            cx,
        )
    }
}

/// A Settings tooltip placed like its React `TooltipContent` (`side`, `sideOffset`), after the
/// provider's 300ms delay.
fn placed_tooltip(
    element: Stateful<Div>,
    text: SharedString,
    placement: ManagedTooltipPlacement,
    offset: f32,
    max_width: Option<f32>,
    center: bool,
) -> Stateful<Div> {
    element.managed_discrete_tooltip_with_placement(placement, TOOLTIP_DELAY, move |window, cx| {
        settings_tooltip_bubble(
            text.clone(),
            Some((placement, offset)),
            max_width,
            center,
            window,
            cx,
        )
    })
}

/// `ModifiedSettingResetButton`: a 14px asterisk hung 3px left of the label.
fn modified_reset_button(
    p: &SettingsPalette,
    id: ElementId,
    after_label: bool,
    on_reset: BoundPress,
) -> AnyElement {
    let color = p.modified_marker();
    let hover = p.foreground;
    // `right: calc(100% + 0.1875rem)`: 14px wide, 3px left of the label line.
    let button = div()
        .id(id)
        .role(gpui::Role::Button)
        .aria_label("Reset to default")
        .size(px(14.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .cursor_pointer()
        .text_color(hsla(color))
        .hover(move |this| this.text_color(hsla(hover)))
        .on_bound_press(on_reset)
        .child(settings_icon(icon::ASTERISK, 10.0, color));
    // `<TooltipContent className='whitespace-pre-line text-center' sideOffset={6}>` (below).
    let button = placed_tooltip(
        button,
        MODIFIED_SETTING_TOOLTIP.into(),
        ManagedTooltipPlacement::Below,
        6.0,
        None,
        true,
    );
    if after_label {
        return div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .child(button)
            .into_any_element();
    }
    div()
        .absolute()
        .left(px(-17.0))
        .top_0()
        .bottom_0()
        .flex()
        .items_center()
        .child(button)
        .into_any_element()
}

/// `.settings-row-info-button`: 18px, revealed with the row.
fn label_action_button(
    p: &SettingsPalette,
    id: ElementId,
    group: SharedString,
    icon_path: &'static str,
    color: gpui::Rgba,
    tooltip: SharedString,
) -> AnyElement {
    let _ = p;
    let button = div()
        .id(id)
        .flex_shrink_0()
        .size(px(18.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .opacity(0.0)
        .group_hover(group, |this| this.opacity(1.0))
        .child(settings_icon(icon_path, 15.0, color));
    // `SettingDescriptionTooltip`: `side='right' sideOffset={8}`, at most 350px wide.
    placed_tooltip(
        button,
        tooltip,
        ManagedTooltipPlacement::Right,
        8.0,
        Some(350.0),
        false,
    )
    .into_any_element()
}

/// `SettingDescriptionTooltip` for a row built outside `setting_row` (a management list item):
/// the info icon that appears while the row's `group` is hovered, with `tooltip` behind it.
pub(crate) fn description_info_button(
    p: &SettingsPalette,
    id: impl Into<ElementId>,
    group: SharedString,
    tooltip: impl Into<SharedString>,
) -> AnyElement {
    label_action_button(
        p,
        id.into(),
        group,
        icon::INFO_CIRCLE,
        p.muted,
        tooltip.into(),
    )
}

/// CDXC:Settings 2026-09-09 DECISION:
/// User: rows show no subtitle text. The description (and any subtitle) lives only in the tooltip behind the hover-revealed info icon next to the label.
/// `SettingRow`: the label line on the left (asterisk, `↳` prefix, label, badge and info icon) and the control on the right, or under the label for a wide row.
///
/// CDXC:Release 2026-10-22 WHY: the body lives in the non-generic `setting_row_bound`; generic over the page it was copied into the release binary once per page and id type (14 copies, about 0.15 MB).
pub(crate) fn setting_row<V: SettingsPage>(
    p: &SettingsPalette,
    id: impl Into<SharedString>,
    spec: RowSpec,
    on_reset: Option<PageAction<V>>,
    control: AnyElement,
    cx: &mut Context<V>,
) -> AnyElement {
    let on_reset = on_reset
        .filter(|_| spec.modified)
        .map(|on_reset| bind_press_rc(cx, on_reset));
    setting_row_bound(p, id.into(), spec, on_reset, control)
}

fn setting_row_bound(
    p: &SettingsPalette,
    id: SharedString,
    spec: RowSpec,
    on_reset: Option<BoundPress>,
    control: AnyElement,
) -> AnyElement {
    let group: SharedString = format!("settings-row-{id}").into();
    let tooltip = spec.tooltip_text();
    let label = if spec.dependent {
        format!("    \u{21b3} {}", spec.label)
    } else {
        spec.label.to_string()
    };
    let mut label_line = h_flex()
        .relative()
        .items_center()
        .gap(px(4.0))
        .min_w_0()
        .max_w_full()
        .child(
            div()
                .min_w_0()
                .text_size(px(14.0))
                .line_height(px(18.9))
                .text_color(hsla(p.foreground))
                .whitespace_normal()
                .child(label),
        );
    if spec.modified
        && let Some(on_reset) = on_reset
    {
        label_line = label_line.child(modified_reset_button(
            p,
            ElementId::Name(format!("{id}-reset").into()),
            spec.marker_after_label,
            on_reset,
        ));
    }
    if let Some(readout) = spec.readout.clone() {
        label_line = label_line.child(
            div()
                .flex_shrink_0()
                .ml(px(8.0))
                .text_size(px(13.0))
                .line_height(px(18.57))
                .text_color(hsla(p.muted))
                .child(readout),
        );
    }
    if let Some(badge) = spec.badge.clone() {
        label_line = label_line.child(
            div()
                .flex_shrink_0()
                .px(px(6.0))
                .py(px(2.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(hsla(p.hairline))
                .text_size(px(11.0))
                .line_height(px(14.0))
                .text_color(hsla(p.muted))
                .child(badge),
        );
    }
    if spec.experimental {
        label_line = label_line.child(
            placed_tooltip(
                div()
                    .id(ElementId::Name(format!("{id}-experimental").into()))
                    .flex_shrink_0()
                    .ml(px(2.0))
                    .size(px(16.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(settings_icon(
                        "modals/space-editor/flask-filled.svg",
                        14.0,
                        p.muted,
                    )),
                "Experimental Feature".into(),
                ManagedTooltipPlacement::Below,
                6.0,
                None,
                false,
            )
            .into_any_element(),
        );
    }
    if let Some(tooltip) = tooltip {
        label_line = label_line.child(label_action_button(
            p,
            ElementId::Name(format!("{id}-info").into()),
            group.clone(),
            icon::INFO_CIRCLE,
            p.muted,
            tooltip,
        ));
    }
    let text = v_flex().flex_1().min_w_0().gap(px(4.0)).child(label_line);
    let text = match spec.icon {
        Some(icon) => h_flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .gap(px(ICON_TILE_GAP))
            .child(icon_tile(p, icon))
            .child(text)
            .into_any_element(),
        None => text.into_any_element(),
    };
    let control_inset = if spec.icon.is_some() {
        ICON_TILE_SIZE + ICON_TILE_GAP
    } else {
        0.0
    };
    let row = div()
        .id(ElementId::Name(format!("{id}-row").into()))
        .group(group)
        .w_full()
        .min_h(px(ROW_MIN_HEIGHT))
        .px(px(spec.inset_x.unwrap_or(ROW_PADDING_X)))
        .py(px(ROW_PADDING_Y))
        .flex();
    if spec.wide {
        row.flex_col()
            .items_stretch()
            .gap(px(10.0))
            .child(text)
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .pl(px(control_inset))
                    .flex()
                    .child(control),
            )
            .into_any_element()
    } else {
        row.items_center()
            .justify_between()
            .gap(px(ROW_GAP))
            .child(text)
            .child(
                div()
                    .flex_shrink_0()
                    .max_w(gpui::relative(0.6))
                    .min_w_0()
                    .flex()
                    .items_center()
                    .justify_end()
                    .child(control),
            )
            .into_any_element()
    }
}

/// The icon tile in front of a row's label: 34px, a 9px radius, a hairline and a faint wash.
pub(crate) const ICON_TILE_SIZE: f32 = 34.0;
/// The space between the icon tile and the label.
pub(crate) const ICON_TILE_GAP: f32 = 14.0;

pub(crate) fn icon_tile(p: &SettingsPalette, icon: &'static str) -> AnyElement {
    div()
        .flex_shrink_0()
        .size(px(ICON_TILE_SIZE))
        .rounded(px(9.0))
        .border_1()
        .border_color(hsla(p.hairline))
        .bg(hsla(p.foreground_alpha(0.035)))
        .flex()
        .items_center()
        .justify_center()
        .child(settings_icon(icon, 17.0, p.muted))
        .into_any_element()
}

/// A card child that is not a row (a notice, a button strip, an editor form): the row inset.
pub(crate) fn card_inset(content: impl IntoElement) -> AnyElement {
    div()
        .w_full()
        .px(px(ROW_PADDING_X))
        .py(px(14.0))
        .child(content)
        .into_any_element()
}

/// CDXC:Settings 2026-09-09 DECISION:
/// User: every Settings page and section shares one style, the grouped-list prototype: a plain group heading above a raised card, one setting per row with its label on the left and its control on the right, rows separated by hairlines, no floating title pill and no visible subtitle text.
/// `SettingsSection`: a plain heading (16px, optional 13px description and right-side actions)
/// above a raised card whose children divide with hairlines. `None` when every row is hidden,
/// the `:has(> .settings-list-card:empty)` rule.
pub(crate) fn settings_section(
    p: &SettingsPalette,
    title: impl Into<SharedString>,
    description: Option<SharedString>,
    actions: Option<AnyElement>,
    rows: Vec<AnyElement>,
) -> Option<Div> {
    if rows.is_empty() {
        return None;
    }
    let hairline = hsla(p.hairline);
    let rows = rows.into_iter().enumerate().map(|(index, row)| {
        div()
            .w_full()
            .when(index > 0, |this| this.border_t_1().border_color(hairline))
            .child(row)
    });
    Some(
        v_flex()
            .w_full()
            .gap(px(12.0))
            .child(
                h_flex()
                    .w_full()
                    .items_end()
                    .justify_between()
                    .gap(px(16.0))
                    .px(px(2.0))
                    .child(
                        v_flex()
                            .min_w_0()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_size(px(16.0))
                                    .line_height(px(20.8))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(hsla(p.foreground))
                                    .child(title.into()),
                            )
                            .children(description.map(|description| {
                                div()
                                    .text_size(px(13.0))
                                    .line_height(px(18.85))
                                    .text_color(hsla(p.muted))
                                    .child(description)
                            })),
                    )
                    .children(actions.map(|actions| {
                        h_flex()
                            .flex_shrink_0()
                            .items_center()
                            .gap(px(8.0))
                            .child(actions)
                    })),
            )
            .child(
                v_flex()
                    .w_full()
                    .rounded(px(MODAL_RADIUS_SECTION))
                    .border_1()
                    .border_color(hairline)
                    .bg(hsla(p.raised))
                    .overflow_hidden()
                    .children(rows),
            ),
    )
}

/// The status dot of a `SettingsListItem`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ListItemStatus {
    Success,
    Warning,
    Neutral,
}

/// `SettingsListItem`: a management row (agents, actions, open targets) on the setting rows'
/// geometry: optional status dot and 32px icon, title and one-line detail, trailing controls.
pub(crate) fn settings_list_item(
    p: &SettingsPalette,
    status: Option<ListItemStatus>,
    icon: Option<AnyElement>,
    title: impl IntoElement,
    detail: Option<AnyElement>,
    controls: Option<AnyElement>,
) -> AnyElement {
    h_flex()
        .w_full()
        .min_h(px(ROW_MIN_HEIGHT))
        .px(px(ROW_PADDING_X))
        .py(px(ROW_PADDING_Y))
        .gap(px(ROW_GAP))
        .items_center()
        .justify_between()
        .children(status.map(|status| {
            let color = match status {
                ListItemStatus::Success => modal_rgba(0x34d399, 0.8),
                ListItemStatus::Warning => modal_rgba(0xfbbf24, 0.85),
                ListItemStatus::Neutral => modal_rgba(0xffffff, 0.2),
            };
            div()
                .flex_shrink_0()
                .size(px(6.0))
                .rounded_full()
                .bg(hsla(color))
        }))
        .children(icon.map(|icon| {
            div()
                .flex_shrink_0()
                .size(px(32.0))
                .flex()
                .items_center()
                .justify_center()
                .text_color(hsla(p.muted))
                .child(icon)
        }))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(14.0))
                        .line_height(px(18.9))
                        .text_color(hsla(p.foreground))
                        .child(title),
                )
                .children(detail.map(|detail| {
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(13.0))
                        .line_height(px(18.2))
                        .text_color(hsla(p.muted))
                        .child(detail)
                })),
        )
        .children(controls.map(|controls| {
            div()
                .flex_shrink_0()
                .max_w(gpui::relative(0.6))
                .flex()
                .items_center()
                .justify_end()
                .child(controls)
        }))
        .into_any_element()
}

/// `StaticNoteField`'s value box.
pub(crate) fn static_note(
    p: &SettingsPalette,
    value: impl Into<SharedString>,
    boxed: bool,
) -> AnyElement {
    if !boxed {
        return div()
            .text_size(px(14.0))
            .text_color(hsla(p.muted))
            .child(value.into())
            .into_any_element();
    }
    div()
        .px(px(12.0))
        .py(px(8.0))
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(p.hairline))
        .bg(hsla(css_fade(p.raised_hover, 0.3)))
        .text_size(px(14.0))
        .text_color(hsla(p.muted))
        .child(value.into())
        .into_any_element()
}

/// A focusable element id helper for rows built from a key.
pub(crate) fn row_id(key: &str) -> SharedString {
    SharedString::from(key.to_string())
}

/// Wraps a stateful element in the settings tooltip, the `AppTooltip` of the React fields.
pub(crate) fn with_tooltip(element: Stateful<Div>, text: impl Into<SharedString>) -> Stateful<Div> {
    element.tooltip(tooltip_text(text))
}
