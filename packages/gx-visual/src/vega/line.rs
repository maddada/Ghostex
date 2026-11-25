//! Line and area charts: one polyline (or filled band) per color series over an ordered x.

use std::collections::BTreeMap;

use super::axis::{self, AxisKind, AxisSpec, Frame, Scale};
use super::data::{self, Domain};
use super::scale::{is_year_axis, quant_domain, TickFormat};
use super::spec::{Aggregate, ChartSpec, MarkKind, StackMode};
use super::tooltip::{self, Shown};
use super::{legend_on_top, no_values, Series};
use crate::scene::{Block, Item, TooltipLine};
use crate::text::{format_number, format_percent, format_plain};
use crate::theme::Theme;

/// The ordered x positions: categories, or distinct numbers.
enum Slots {
    Cat(Domain),
    Num { values: Vec<f64>, years: bool },
}

impl Slots {
    fn len(&self) -> usize {
        match self {
            Slots::Cat(domain) => domain.len(),
            Slots::Num { values, .. } => values.len(),
        }
    }

    fn label(&self, i: usize) -> String {
        match self {
            Slots::Cat(domain) => domain.labels.get(i).cloned().unwrap_or_default(),
            Slots::Num { values, years } => values
                .get(i)
                .map(|v| {
                    if *years {
                        format_plain(*v)
                    } else {
                        format_number(*v)
                    }
                })
                .unwrap_or_default(),
        }
    }
}

/// One series' value at one slot: drawn from `v0` up to `v1`; `value` is the unstacked number.
struct Point {
    slot: usize,
    value: f64,
    v0: f64,
    v1: f64,
    rows: Vec<usize>,
}

pub(crate) fn render(spec: &ChartSpec, width: f32, theme: &Theme) -> Result<Block, String> {
    let area = spec.mark.kind == MarkKind::Area;
    let name = if area {
        "An area chart"
    } else {
        "A line chart"
    };
    let (Some(x), Some(y)) = (spec.x.as_ref(), spec.y.as_ref()) else {
        return Err(format!("{name} needs both x and y in encoding."));
    };
    if y.ty.is_category() {
        return Err(format!(
            "{name} needs a quantitative y (put the categories on x or in color)."
        ));
    }
    if x.aggregate.is_some() {
        return Err(format!("{name} can't aggregate x; aggregate y instead."));
    }
    let series = Series::of(spec, theme)?;
    let slots = if x.ty.is_category() {
        Slots::Cat(data::domain(spec, x))
    } else {
        let mut values: Vec<f64> = spec
            .rows
            .iter()
            .filter_map(|r| data::row_number(r, x))
            .collect();
        values.sort_by(f64::total_cmp);
        values.dedup();
        let years = is_year_axis(x, &values);
        Slots::Num { values, years }
    };
    if slots.len() == 0 {
        return Err(no_values(x));
    }
    let slot_of = |row: &data::Row| -> Option<usize> {
        match &slots {
            Slots::Cat(domain) => domain.of_row(row, x),
            Slots::Num { values, .. } => {
                let v = data::row_number(row, x)?;
                values.binary_search_by(|p| p.total_cmp(&v)).ok()
            }
        }
    };
    let mut groups: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
    for (i, row) in spec.rows.iter().enumerate() {
        if let (Some(slot), Some(s)) = (slot_of(row), series.of_row(spec, row)) {
            groups.entry((s, slot)).or_default().push(i);
        }
    }
    let default_op = if area {
        Aggregate::Sum
    } else {
        Aggregate::Mean
    };
    let mut lines: Vec<Vec<Point>> = (0..series.len()).map(|_| Vec::new()).collect();
    for ((s, slot), rows) in groups {
        if let (Some(value), Some(line)) = (
            data::measure(&spec.rows, &rows, y, default_op),
            lines.get_mut(s),
        ) {
            line.push(Point {
                slot,
                value,
                v0: 0.0,
                v1: value,
                rows,
            });
        }
    }
    if lines.iter().all(Vec::is_empty) {
        return Err(no_values(y));
    }
    let stack = if area {
        match y.stack {
            StackMode::Off => None,
            StackMode::Zero => Some(false),
            StackMode::Normalize => Some(true),
            StackMode::Default => spec.color_field().is_some().then_some(false),
        }
    } else {
        None
    };
    let normalize = stack == Some(true);
    if let Some(normalize) = stack {
        stack_series(&mut lines, slots.len(), normalize);
    }

    let extents: Vec<f64> = lines.iter().flatten().flat_map(|p| [p.v0, p.v1]).collect();
    let y_domain = if normalize {
        (0.0, 1.0)
    } else {
        quant_domain(&extents, y, area)
    };
    let mut block = Block::new();
    let top = legend_on_top(&mut block, &series, width, theme);
    let x_kind = match &slots {
        Slots::Cat(domain) => AxisKind::Band {
            labels: domain.labels.clone(),
            padding: 0.0,
        },
        Slots::Num { values, years } => AxisKind::Linear {
            domain: quant_domain(values, x, false),
            nice: x.domain.is_none(),
            format: if *years {
                TickFormat::Plain
            } else {
                TickFormat::Compact
            },
        },
    };
    let x_axis = AxisSpec {
        kind: x_kind,
        title: x.axis_title(),
        hidden: x.axis_hidden,
    };
    let y_axis = AxisSpec {
        kind: AxisKind::Linear {
            domain: y_domain,
            nice: y.domain.is_none() && !normalize,
            format: if normalize {
                TickFormat::Percent
            } else {
                TickFormat::Compact
            },
        },
        title: y.axis_title(),
        hidden: y.axis_hidden,
    };
    let height = spec.plot_height();
    let frame = axis::build(&mut block, theme, width, top, height, x_axis, y_axis);
    let Scale::Linear(y_scale) = &frame.y else {
        return Err(format!("{name} needs a quantitative y."));
    };
    let x_pos = |slot: usize| -> f32 {
        match (&frame.x, &slots) {
            (Scale::Band(band), _) => band.center(slot),
            (Scale::Linear(scale), Slots::Num { values, .. }) => {
                scale.pos_clamped(values.get(slot).copied().unwrap_or(0.0))
            }
            _ => frame.left,
        }
    };

    for (s, line) in lines.iter().enumerate() {
        let color = series.color(s);
        let mut sorted: Vec<&Point> = line.iter().collect();
        sorted.sort_by(|a, b| x_pos(a.slot).total_cmp(&x_pos(b.slot)));
        let top_points: Vec<[f32; 2]> = sorted
            .iter()
            .map(|p| [x_pos(p.slot), y_scale.pos_clamped(p.v1)])
            .collect();
        let reveal = crate::Motion::Reveal {
            from: top_points.first().map_or(0.0, |p| p[0]),
            to: top_points.last().map_or(0.0, |p| p[0]),
        };
        if area && top_points.len() > 1 {
            let mut polygon = top_points.clone();
            polygon.extend(
                sorted
                    .iter()
                    .rev()
                    .map(|p| [x_pos(p.slot), y_scale.pos_clamped(p.v0)]),
            );
            block.push(Item::Path {
                points: polygon,
                closed: true,
                fill: Some(color.with_alpha(color.a * 0.35)),
                stroke: None,
                stroke_width: 0.0,
                dashed: false,
            });
            block.animate(block.last_item(), reveal);
        }
        if top_points.len() > 1 {
            block.push(Item::Path {
                points: top_points.clone(),
                closed: false,
                fill: None,
                stroke: Some(color),
                stroke_width: if area { 1.5 } else { 2.0 },
                dashed: false,
            });
            block.animate(block.last_item(), reveal);
        }
        if spec.mark.point || top_points.len() <= 1 {
            for [px, py] in &top_points {
                block.circle(*px, *py, 3.0, color);
                block.animate(block.last_item(), crate::Motion::Pop);
            }
        }
    }
    frame.baseline(&mut block, theme);
    add_column_regions(
        &mut block, spec, &frame, &slots, &series, &lines, normalize, &x_pos,
    );
    block.height = top + height;
    Ok(block)
}

/// Stacks the series slot by slot in series order; a series missing at a slot counts as zero.
fn stack_series(lines: &mut [Vec<Point>], slot_count: usize, normalize: bool) {
    let mut totals = vec![0.0f64; slot_count];
    if normalize {
        for p in lines.iter().flatten() {
            if let Some(t) = totals.get_mut(p.slot) {
                *t += p.value.abs();
            }
        }
    }
    let mut acc = vec![0.0f64; slot_count];
    let active: Vec<usize> = {
        let mut seen = vec![false; slot_count];
        for p in lines.iter().flatten() {
            if let Some(s) = seen.get_mut(p.slot) {
                *s = true;
            }
        }
        (0..slot_count).filter(|i| seen[*i]).collect()
    };
    for line in lines.iter_mut() {
        let mut by_slot: BTreeMap<usize, Point> = line.drain(..).map(|p| (p.slot, p)).collect();
        for &slot in &active {
            let mut point = by_slot.remove(&slot).unwrap_or(Point {
                slot,
                value: 0.0,
                v0: 0.0,
                v1: 0.0,
                rows: Vec::new(),
            });
            let total = totals.get(slot).copied().unwrap_or(0.0);
            let v = if normalize && total > 0.0 {
                point.value / total
            } else {
                point.value
            };
            point.v0 = acc[slot];
            acc[slot] += v;
            point.v1 = acc[slot];
            line.push(point);
        }
    }
}

/// One hover column per x slot spanning the plot height, listing every series' value there.
#[allow(clippy::too_many_arguments)]
fn add_column_regions(
    block: &mut Block,
    spec: &ChartSpec,
    frame: &Frame,
    slots: &Slots,
    series: &Series,
    lines: &[Vec<Point>],
    normalize: bool,
    x_pos: &dyn Fn(usize) -> f32,
) {
    let (Some(x), Some(y)) = (spec.x.as_ref(), spec.y.as_ref()) else {
        return;
    };
    let mut columns: BTreeMap<usize, Vec<(usize, &Point)>> = BTreeMap::new();
    for (s, line) in lines.iter().enumerate() {
        for p in line {
            if !p.rows.is_empty() {
                columns.entry(p.slot).or_default().push((s, p));
            }
        }
    }
    let mut ordered: Vec<(f32, usize)> = columns.keys().map(|&slot| (x_pos(slot), slot)).collect();
    ordered.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (k, &(at, slot)) in ordered.iter().enumerate() {
        let left = match k.checked_sub(1).and_then(|j| ordered.get(j)) {
            Some(prev) => (prev.0 + at) / 2.0,
            None => frame.left,
        };
        let right = match ordered.get(k + 1) {
            Some(next) => (at + next.0) / 2.0,
            None => frame.right,
        };
        let entries = columns.get(&slot).map(Vec::as_slice).unwrap_or(&[]);
        let value_text = |p: &Point| {
            if normalize {
                format!(
                    "{} · {}",
                    format_percent(p.v1 - p.v0),
                    format_number(p.value)
                )
            } else {
                format_number(p.value)
            }
        };
        let tooltip_lines = match entries {
            [(s, p)] if series.domain.is_none() || !spec.tooltip.is_empty() => {
                let mut shown = vec![Shown::new(x, slots.label(slot))];
                if let (Some(def), Some(label)) = (spec.color_field(), series.label(*s)) {
                    shown.push(Shown::colored(def, label.to_string(), series.color(*s)));
                }
                shown.push(Shown::new(y, value_text(p)));
                tooltip::lines(spec, &p.rows, &shown)
            }
            _ => {
                let mut out = vec![TooltipLine {
                    label: x.label(),
                    value: slots.label(slot),
                    color: None,
                }];
                for (s, p) in entries {
                    out.push(TooltipLine {
                        label: series
                            .label(*s)
                            .map(str::to_string)
                            .unwrap_or_else(|| y.label()),
                        value: value_text(p),
                        color: Some(series.color(*s)),
                    });
                }
                out
            }
        };
        block.region(
            None,
            left,
            frame.top,
            (right - left).max(1.0),
            frame.bottom - frame.top,
            tooltip_lines,
        );
    }
}
