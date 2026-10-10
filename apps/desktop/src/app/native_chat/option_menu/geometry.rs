use super::super::appearance::ChatAppearance;
use gpui::{App, TextRun, WindowTextSystem, px};
use serde_json::Value;

/// The box sizes of a menu panel's plain rows, in unscaled pixels.
#[derive(Clone, Copy)]
pub(super) struct MenuMetrics {
    pub(super) padding: f32,
    pub(super) gap: f32,
    pub(super) radius: f32,
    pub(super) text: f32,
    pub(super) line: f32,
    pub(super) row_x: f32,
    pub(super) row_y: f32,
    pub(super) row_radius: f32,
    pub(super) row_gap: f32,
    pub(super) icon: f32,
    pub(super) heading_min: f32,
    pub(super) separator: f32,
}

impl MenuMetrics {
    pub(super) const REGULAR: Self = Self {
        padding: 6.0,
        gap: 2.0,
        radius: 8.0,
        text: 13.0,
        line: 18.2,
        row_x: 10.0,
        row_y: 8.0,
        row_radius: 6.0,
        row_gap: 8.0,
        icon: 14.0,
        heading_min: 28.0,
        separator: 13.0,
    };

    /**
    CDXC:SessionChat 2026-09-21 DECISION: "The context menu that we show in the gpui chat view" must look like the context menu in the GPUI sidebar (`native_sidebar/menus.rs`). Menus opened at the pointer use that menu's box: 13px text, 34px rows, 16px icons, 8px corners, a width fitted to the longest row from the same 178px floor, and no window shadow or system frame (`app/window/popup_frame.rs`).
    */
    pub(super) const CONTEXT: Self = Self {
        icon: 16.0,
        ..Self::REGULAR
    };

    /// The panel's own padding and 1px border on both sides.
    pub(super) fn chrome(&self) -> f32 {
        self.padding * 2.0 + 2.0
    }
}

/// The narrowest width from the sidebar menu's 178px floor, up to `max`, that shows every row of a pointer menu unclipped.
pub(super) fn fit_width(rows: &[Value], max: f32, appearance: &ChatAppearance, cx: &App) -> f32 {
    natural_width(rows, appearance, cx).clamp(178.0_f32.min(max), max)
}

/// The width a panel needs for its rows' hotkeys (`detail`): a fixed panel width was sized for the
/// macOS glyphs, and Windows spells a chord out (`Ctrl+Alt+Shift+F`), which ran into the labels.
/// Never narrower than `width`.
pub(super) fn width_for_details(
    rows: &[Value],
    width: f32,
    appearance: &ChatAppearance,
    cx: &App,
) -> f32 {
    let detailed: Vec<Value> = rows
        .iter()
        .filter(|row| row["detail"].is_string())
        .cloned()
        .collect();
    if detailed.is_empty() {
        return width;
    }
    width.max(natural_width(&detailed, appearance, cx))
}

/// The width that shows every row of `rows` unclipped, with no floor or ceiling.
fn natural_width(rows: &[Value], appearance: &ChatAppearance, cx: &App) -> f32 {
    let m = MenuMetrics::CONTEXT;
    let text_system = WindowTextSystem::new(cx.text_system().clone());
    let font = gpui::font(appearance.font.clone());
    let text_width = |text: &str| {
        let run = TextRun {
            len: text.len(),
            font: font.clone(),
            ..Default::default()
        };
        f32::from(
            text_system
                .shape_line(text.to_owned().into(), px(m.text), &[run], None)
                .width,
        )
    };
    let widest = rows
        .iter()
        .filter(|row| row["separator"] != true)
        .map(|row| {
            let label = text_width(row["label"].as_str().unwrap_or_default());
            let description = row["description"].as_str().map(text_width).unwrap_or(0.0);
            let icon = if row["icon"].is_string() || row["iconPath"].is_string() {
                m.icon + m.row_gap
            } else if row["dot"].is_string() {
                6.0 + m.row_gap
            } else {
                0.0
            };
            let detail = row["detail"]
                .as_str()
                .map(|text| text_width(text) + m.row_gap)
                .unwrap_or(0.0);
            let accessory = if row.get("checked").is_some() || row["children"].is_array() {
                m.icon + m.row_gap
            } else {
                0.0
            };
            m.row_x * 2.0 + icon + label.max(description) + detail + accessory
        })
        .fold(0.0, f32::max);
    // Room for a trailing glyph's rounding, so the label never ellipsizes at its own width.
    widest.ceil() + 2.0 + m.chrome()
}

pub(super) fn measure_rows(
    rows: &[Value],
    width: f32,
    metrics: MenuMetrics,
    appearance: &ChatAppearance,
    cx: &App,
) -> anyhow::Result<Vec<f32>> {
    let m = metrics;
    let scale = appearance.scale;
    let text_system = WindowTextSystem::new(cx.text_system().clone());
    let font = gpui::font(appearance.font.clone());
    let text_width = |text: &str| {
        let run = TextRun {
            len: text.len(),
            font: font.clone(),
            ..Default::default()
        };
        f32::from(
            text_system
                .shape_line(text.to_owned().into(), px(12.0 * scale), &[run], None)
                .width,
        ) / scale
    };
    rows.iter()
        .map(|row| {
            if row["context"].is_object() {
                return super::context::height(&row["context"], width, appearance, cx);
            }
            if row["accounts"].is_object() {
                return super::accounts::height(
                    &row["accounts"],
                    width,
                    row["customize"] == true,
                    appearance,
                    cx,
                );
            }
            if row["modelMenu"].is_object() {
                return Ok(super::model_menu::menu_height(&row["modelMenu"]));
            }
            if row["modelMenuFlyout"].is_object() {
                return Ok(super::model_menu::flyout_height(&row["modelMenuFlyout"]));
            }
            if row["separator"] == true {
                return Ok(m.separator);
            }
            let heading = row["heading"] == true;
            let accessory =
                if !heading && (row.get("checked").is_some() || row["children"].is_array()) {
                    m.icon + m.row_gap
                } else {
                    0.0
                };
            let icon = if row["icon"].is_string() || row["iconPath"].is_string() {
                m.icon + m.row_gap
            } else if row["dot"].is_string() {
                6.0 + m.row_gap
            } else {
                0.0
            };
            let detail = row["detail"]
                .as_str()
                .map(|text| text_width(text) + m.row_gap)
                .unwrap_or(0.0);
            let content_width =
                (width - m.chrome() - m.row_x * 2.0 - accessory - icon - detail).max(1.0) * scale;
            let description_height = if row["description"].is_string() && !heading {
                // A row's subtitle is one truncated 16px line under a 2px gap (`render.rs`). Its
                // shaped text is one wrapped line whose height counts every wrap, so a subtitle
                // longer than the row (the Skills list's) measured several lines too tall.
                16.0 + 2.0
            } else if let Some(description) = row["description"].as_str() {
                let run = TextRun {
                    len: description.len(),
                    font: font.clone(),
                    ..Default::default()
                };
                let lines = text_system.shape_text(
                    description.to_owned().into(),
                    px(if heading { 11.0 } else { 12.0 } * scale),
                    &[run],
                    Some(px(content_width)),
                    None,
                )?;
                lines
                    .iter()
                    // A row's subtitle is one truncated line (`render.rs`); only a heading wraps.
                    .take(if heading { usize::MAX } else { 1 })
                    .map(|line| f32::from(line.size(px(16.0 * scale)).height) / scale)
                    .sum::<f32>()
                    + 2.0
            } else {
                0.0
            };
            Ok(if heading {
                (m.line + 6.0 + description_height).max(m.heading_min)
            } else {
                m.line + m.row_y * 2.0 + description_height
            })
        })
        .collect()
}
