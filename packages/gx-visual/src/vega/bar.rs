//! Bar charts: vertical or horizontal, stacked, normalized or grouped with xOffset.

use std::collections::BTreeMap;

use super::axis::{self, AxisKind, AxisSpec, Scale};
use super::data;
use super::scale::{quant_domain, Band, Linear, TickFormat};
use super::spec::{Aggregate, ChartSpec, StackMode};
use super::tooltip::{self, Shown};
use super::{legend_on_top, no_values, Series};
use crate::scene::Block;
use crate::text::{format_number, format_percent};
use crate::theme::Theme;

struct Bar {
    cat: usize,
    series: usize,
    offset: usize,
    value: f64,
    v0: f64,
    v1: f64,
    rows: Vec<usize>,
}

pub(crate) fn render(spec: &ChartSpec, width: f32, theme: &Theme) -> Result<Block, String> {
    let (Some(x), Some(y)) = (spec.x.as_ref(), spec.y.as_ref()) else {
        return Err("A bar chart needs both x and y in encoding.".to_string());
    };
    let horizontal = match (x.ty.is_category(), y.ty.is_category()) {
        (true, false) => false,
        (false, true) => true,
        (false, false) => {
            return Err(
                "bar needs one category axis: make x or y nominal, ordinal or temporal."
                    .to_string(),
            )
        }
        (true, true) => {
            return Err(
                "bar needs one quantitative axis: make x or y quantitative, or aggregate it."
                    .to_string(),
            )
        }
    };
    let (cat, val) = if horizontal { (y, x) } else { (x, y) };
    if horizontal && spec.x_offset.is_some() {
        return Err("xOffset needs vertical bars (the categories on x).".to_string());
    }
    let cats = data::domain(spec, cat);
    if cats.is_empty() {
        return Err(no_values(cat));
    }
    let series = Series::of(spec, theme)?;
    let offset_def = spec.x_offset.as_ref();
    let offsets = offset_def.map(|def| data::domain(spec, def));
    let stack = match val.stack {
        StackMode::Off => None,
        StackMode::Zero => Some(false),
        StackMode::Normalize => Some(true),
        StackMode::Default => (spec.color_field().is_some() && offsets.is_none()).then_some(false),
    };

    let mut groups: BTreeMap<(usize, usize, usize), Vec<usize>> = BTreeMap::new();
    for (i, row) in spec.rows.iter().enumerate() {
        let Some(c) = cats.of_row(row, cat) else {
            continue;
        };
        let Some(s) = series.of_row(spec, row) else {
            continue;
        };
        let o = match (&offsets, offset_def) {
            (Some(domain), Some(def)) => match domain.of_row(row, def) {
                Some(o) => o,
                None => continue,
            },
            _ => 0,
        };
        groups.entry((c, s, o)).or_default().push(i);
    }
    let mut bars: Vec<Bar> = groups
        .into_iter()
        .filter_map(|((cat, series, offset), rows)| {
            let value = data::measure(&spec.rows, &rows, val, Aggregate::Sum)?;
            Some(Bar {
                cat,
                series,
                offset,
                value,
                v0: 0.0,
                v1: value,
                rows,
            })
        })
        .collect();
    if bars.is_empty() {
        return Err(no_values(val));
    }
    let normalize = stack == Some(true);
    if let Some(normalize) = stack {
        let mut totals = vec![0.0f64; cats.len()];
        if normalize {
            for bar in &bars {
                totals[bar.cat] += bar.value.abs();
            }
        }
        let mut up = vec![0.0f64; cats.len()];
        let mut down = vec![0.0f64; cats.len()];
        for bar in &mut bars {
            let total = totals[bar.cat];
            let v = if normalize && total > 0.0 {
                bar.value / total
            } else {
                bar.value
            };
            let acc = if v >= 0.0 {
                &mut up[bar.cat]
            } else {
                &mut down[bar.cat]
            };
            bar.v0 = *acc;
            *acc += v;
            bar.v1 = *acc;
        }
    }

    let extents: Vec<f64> = bars.iter().flat_map(|b| [b.v0, b.v1]).collect();
    let domain = if normalize {
        (
            extents.iter().copied().fold(0.0, f64::min),
            extents.iter().copied().fold(1.0, f64::max),
        )
    } else {
        quant_domain(&extents, val, true)
    };
    let mut block = Block::new();
    let top = legend_on_top(&mut block, &series, width, theme);
    let cat_axis = AxisSpec {
        kind: AxisKind::Band {
            labels: cats.labels.clone(),
            padding: 0.2,
        },
        title: cat.axis_title(),
        hidden: cat.axis_hidden,
    };
    let val_axis = AxisSpec {
        kind: AxisKind::Linear {
            domain,
            nice: val.domain.is_none() && !normalize,
            format: if normalize {
                TickFormat::Percent
            } else {
                TickFormat::Compact
            },
        },
        title: val.axis_title(),
        hidden: val.axis_hidden,
    };
    let height = spec.plot_height();
    let frame = if horizontal {
        axis::build(&mut block, theme, width, top, height, val_axis, cat_axis)
    } else {
        axis::build(&mut block, theme, width, top, height, cat_axis, val_axis)
    };
    let (band, linear): (&Band, &Linear) = match (&frame.x, &frame.y) {
        (Scale::Band(b), Scale::Linear(l)) | (Scale::Linear(l), Scale::Band(b)) => (b, l),
        _ => return Err("bar needs one category axis.".to_string()),
    };
    let offset_count = offsets.as_ref().map(|d| d.len()).unwrap_or(1).max(1);
    for bar in &bars {
        let mut start = band.start(bar.cat);
        let mut thickness = band.width();
        if offset_count > 1 {
            let sub = thickness / offset_count as f32;
            start += sub * bar.offset as f32;
            thickness = (sub * 0.92).max(1.0);
        }
        let p0 = linear.pos_clamped(bar.v0);
        let p1 = linear.pos_clamped(bar.v1);
        let (lo, len) = (p0.min(p1), (p1 - p0).abs());
        let radius = if stack.is_some() {
            0.0
        } else {
            (thickness / 4.0).min(2.0)
        };
        let color = series.color(bar.series);
        let (rx, ry, rw, rh) = if horizontal {
            (lo, start, len, thickness)
        } else {
            (start, lo, thickness, len)
        };
        block.fill_rect(rx, ry, rw, rh, radius, color);
        let shape = block.last_item();
        block.animate(
            shape,
            crate::Motion::Grow {
                base: p0,
                horizontal,
            },
        );

        let mut shown = vec![Shown::new(cat, cats.labels[bar.cat].clone())];
        if let (Some(def), Some(label)) = (spec.color_field(), series.label(bar.series)) {
            shown.push(Shown::colored(def, label.to_string(), color));
        }
        if let (Some(def), Some(domain)) = (offset_def, &offsets) {
            let same_as_color = spec.color_field().is_some_and(|c| c.same_as(def));
            if let (false, Some(label)) = (same_as_color, domain.labels.get(bar.offset)) {
                shown.push(Shown::new(def, label.clone()));
            }
        }
        let value = if normalize {
            format!(
                "{} · {}",
                format_percent(bar.v1 - bar.v0),
                format_number(bar.value)
            )
        } else {
            format_number(bar.value)
        };
        shown.push(Shown::new(val, value));
        let lines = tooltip::lines(spec, &bar.rows, &shown);
        let (hx, hy, hw, hh) = if horizontal {
            (
                lo - (4.0 - len).max(0.0) / 2.0,
                start,
                len.max(4.0),
                thickness,
            )
        } else {
            (
                start,
                lo - (4.0 - len).max(0.0) / 2.0,
                thickness,
                len.max(4.0),
            )
        };
        block.region(shape, hx, hy, hw, hh, lines);
    }
    frame.baseline(&mut block, theme);
    block.height = top + height;
    Ok(block)
}
