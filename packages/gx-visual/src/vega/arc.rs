//! Pie and donut charts: `theta` per `color` category, clockwise from 12 o'clock.

use std::collections::BTreeMap;
use std::f32::consts::PI;

use super::data;
use super::legend::{self, Entry};
use super::spec::{Aggregate, ChartSpec};
use super::tooltip::{self, Shown};
use super::{no_values, Series};
use crate::scene::{Block, Item, TooltipLine};
use crate::text::{format_number, format_percent};
use crate::theme::Theme;

/// Below this width the legend goes under the pie instead of beside it.
const SIDE_LEGEND_MIN_WIDTH: f32 = 420.0;
const LEGEND_GAP: f32 = 24.0;

struct Slice {
    label: Option<String>,
    series: usize,
    value: f64,
    rows: Vec<usize>,
}

pub(crate) fn render(spec: &ChartSpec, width: f32, theme: &Theme) -> Result<Block, String> {
    let Some(theta) = spec.theta.as_ref() else {
        return Err(
            "An arc (pie) chart needs encoding.theta with a quantitative field.".to_string(),
        );
    };
    if theta.ty.is_category() {
        return Err("encoding.theta must be quantitative.".to_string());
    }
    let series = Series::of(spec, theme)?;
    let mut slices: Vec<Slice> = Vec::new();
    if series.domain.is_some() {
        let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (i, row) in spec.rows.iter().enumerate() {
            if let Some(s) = series.of_row(spec, row) {
                groups.entry(s).or_default().push(i);
            }
        }
        for (s, rows) in groups {
            if let Some(value) = data::measure(&spec.rows, &rows, theta, Aggregate::Sum) {
                slices.push(Slice {
                    label: series.label(s).map(str::to_string),
                    series: s,
                    value,
                    rows,
                });
            }
        }
    } else if theta.aggregate.is_some() {
        let all: Vec<usize> = (0..spec.rows.len()).collect();
        if let Some(value) = data::measure(&spec.rows, &all, theta, Aggregate::Sum) {
            slices.push(Slice {
                label: None,
                series: 0,
                value,
                rows: all,
            });
        }
    } else {
        for (i, row) in spec.rows.iter().enumerate().take(super::MAX_SERIES) {
            if let Some(value) = data::row_number(row, theta) {
                slices.push(Slice {
                    label: None,
                    series: i,
                    value,
                    rows: vec![i],
                });
            }
        }
    }
    if slices.is_empty() {
        return Err(no_values(theta));
    }
    slices.retain(|s| s.value > 0.0);
    let total: f64 = slices.iter().map(|s| s.value).sum();
    if slices.is_empty() || !total.is_finite() || total <= 0.0 {
        return Err("This pie chart has no positive values to draw.".to_string());
    }
    let color_of = |slice: &Slice| -> crate::theme::Color {
        if series.domain.is_some() {
            series.color(slice.series)
        } else if slices.len() > 1 {
            theme.series_color(slice.series)
        } else {
            series.color(0)
        }
    };

    let height = spec.plot_height();
    let entries: Vec<Entry> = slices
        .iter()
        .filter_map(|s| {
            s.label.clone().map(|label| Entry {
                label,
                color: color_of(s),
            })
        })
        .collect();
    let show_legend = entries.len() > 1;
    let side = show_legend && width >= SIDE_LEGEND_MIN_WIDTH;
    let mut block = Block::new();
    let (diameter, cx, cy, block_height) = if side {
        let legend_width = legend::column_width(&entries, width * 0.4, theme);
        let diameter = height.min(width - legend_width - LEGEND_GAP).max(40.0);
        let group = diameter + LEGEND_GAP + legend_width;
        let left = ((width - group) / 2.0).max(0.0);
        legend::column(
            &mut block,
            &entries,
            left + diameter + LEGEND_GAP,
            height / 2.0,
            legend_width,
            height,
            theme,
        );
        (diameter, left + diameter / 2.0, height / 2.0, height)
    } else {
        let diameter = height.min(width).max(40.0);
        let mut total_height = height;
        if show_legend {
            let mut legend_block = Block::new();
            let legend_height = legend::row(&mut legend_block, &entries, 0.0, 0.0, width, theme);
            block.append(legend_block, 0.0, height + 12.0);
            total_height += 12.0 + legend_height;
        }
        (diameter, width / 2.0, height / 2.0, total_height)
    };

    let outer = diameter / 2.0;
    let inner = spec.mark.inner_radius.min(outer - 4.0).max(0.0);
    let separators = slices.len() > 1;
    let mut start = 0.0f32;
    for slice in &slices {
        let share = (slice.value / total) as f32;
        let sweep = share * 2.0 * PI;
        let end = start + sweep;
        let color = color_of(slice);
        let points = wedge(cx, cy, outer, inner, start, end, slices.len() == 1);
        let (min_x, min_y, max_x, max_y) = bounds(&points);
        block.push(Item::Path {
            points,
            closed: true,
            fill: Some(color),
            stroke: separators.then_some(theme.background),
            stroke_width: if separators { 1.5 } else { 0.0 },
            dashed: false,
        });
        let shape = block.last_item();
        block.animate(
            shape,
            crate::Motion::Sweep {
                cx,
                cy,
                outer,
                inner,
                start,
                end,
                full: slices.len() == 1,
            },
        );
        let mut shown = Vec::new();
        if let (Some(def), Some(label)) = (spec.color_field(), &slice.label) {
            shown.push(Shown::colored(def, label.clone(), color));
        }
        shown.push(Shown::new(theta, format_number(slice.value)));
        let mut lines = tooltip::lines(spec, &slice.rows, &shown);
        if spec.tooltip.is_empty() {
            lines.push(TooltipLine {
                label: "Share".to_string(),
                value: format_percent(f64::from(share)),
                color: None,
            });
        }
        block.region(shape, min_x, min_y, max_x - min_x, max_y - min_y, lines);
        start = end;
    }
    block.height = block_height;
    Ok(block)
}

/// A slice as a polygon: the outer arc, then the inner arc backwards (or the center for a pie).
pub(crate) fn wedge(
    cx: f32,
    cy: f32,
    outer: f32,
    inner: f32,
    start: f32,
    end: f32,
    full: bool,
) -> Vec<[f32; 2]> {
    let degrees = (end - start).to_degrees();
    let steps = ((degrees / 3.0).ceil() as usize).max(8);
    let at = |radius: f32, angle: f32| [cx + radius * angle.sin(), cy - radius * angle.cos()];
    let mut points: Vec<[f32; 2]> = (0..=steps)
        .map(|i| at(outer, start + (end - start) * i as f32 / steps as f32))
        .collect();
    if full {
        points.pop();
    }
    if inner > 0.0 {
        let back = (0..=steps)
            .rev()
            .map(|i| at(inner, start + (end - start) * i as f32 / steps as f32));
        if full {
            points.push(points[0]);
        }
        points.extend(back);
    } else if !full {
        points.push([cx, cy]);
    }
    points
}

fn bounds(points: &[[f32; 2]]) -> (f32, f32, f32, f32) {
    points.iter().fold(
        (
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ),
        |(x0, y0, x1, y1), [x, y]| (x0.min(*x), y0.min(*y), x1.max(*x), y1.max(*y)),
    )
}
