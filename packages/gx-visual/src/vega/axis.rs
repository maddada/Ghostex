//! The plot frame of an x/y chart: where the plot sits, its scales, gridlines and axis labels.

use super::scale::{Band, Linear, TickFormat};
use crate::scene::{crisp, Anchor, Block};
use crate::text::{line_height, text_width, truncate};
use crate::theme::Theme;

const RIGHT_PAD: f32 = 8.0;
const LABEL_GAP: f32 = 6.0;

pub(crate) enum AxisKind {
    Linear {
        domain: (f64, f64),
        nice: bool,
        format: TickFormat,
    },
    Band {
        labels: Vec<String>,
        padding: f32,
    },
}

pub(crate) struct AxisSpec {
    pub kind: AxisKind,
    pub title: Option<String>,
    pub hidden: bool,
}

pub(crate) enum Scale {
    Linear(Linear),
    Band(Band),
}

/// The plot rectangle and the scales that map data into it.
pub(crate) struct Frame {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub x: Scale,
    pub y: Scale,
}

impl Frame {
    /// The zero line in the border color, on the quantitative axis, when zero is in its domain.
    pub(crate) fn baseline(&self, block: &mut Block, theme: &Theme) {
        if let Scale::Linear(y) = &self.y {
            if y.contains(0.0) {
                let at = crisp(y.pos(0.0));
                block.line([self.left, at], [self.right, at], theme.border, 1.0);
            }
        } else if let Scale::Linear(x) = &self.x {
            if x.contains(0.0) {
                let at = crisp(x.pos(0.0));
                block.line([at, self.top], [at, self.bottom], theme.border, 1.0);
            }
        }
    }
}

/// Lays out the frame inside (`width` × `height`) starting at `top`, and draws its gridlines,
/// tick labels and axis titles into `block`.
pub(crate) fn build(
    block: &mut Block,
    theme: &Theme,
    width: f32,
    top: f32,
    height: f32,
    x: AxisSpec,
    y: AxisSpec,
) -> Frame {
    let size = theme.text_size;
    let label_h = line_height(size);
    let y_title = y.title.clone().filter(|_| !y.hidden);
    let x_title = x.title.clone().filter(|_| !x.hidden);
    let plot_top =
        top + if y_title.is_some() {
            label_h + 6.0
        } else {
            0.0
        } + 7.0;
    let x_axis_h = if x.hidden { 4.0 } else { LABEL_GAP + label_h }
        + if x_title.is_some() {
            4.0 + label_h
        } else {
            0.0
        };
    let plot_bottom = (top + height - x_axis_h).max(plot_top + 30.0);
    let plot_h = plot_bottom - plot_top;

    let (y_scale, y_labels, gutter) = match y.kind {
        AxisKind::Linear {
            domain,
            nice,
            format,
        } => {
            let target = ((plot_h / 45.0).round() as usize).clamp(2, 6);
            let mut scale = Linear::new(domain, nice, target, format);
            scale.r0 = plot_bottom;
            scale.r1 = plot_top;
            let labels: Vec<String> = scale.ticks.iter().map(|t| scale.label(*t)).collect();
            let widest = labels
                .iter()
                .map(|l| tick_width(l, size))
                .fold(0.0, f32::max);
            (Scale::Linear(scale), labels, widest)
        }
        AxisKind::Band { labels, padding } => {
            let widest = widest(&labels, size).min((width * 0.35).max(40.0));
            let band = Band {
                n: labels.len(),
                r0: plot_top,
                r1: plot_bottom,
                padding,
            };
            (Scale::Band(band), labels, widest)
        }
    };
    let gutter = if y.hidden {
        0.0
    } else {
        gutter + LABEL_GAP + 2.0
    };

    let mut left = gutter;
    let mut right = width - RIGHT_PAD;
    let (x_scale, x_labels) = match x.kind {
        AxisKind::Linear {
            domain,
            nice,
            format,
        } => {
            let target = (((right - left) / (90.0 * size / 11.0)).round() as usize).clamp(2, 8);
            let mut scale = Linear::new(domain, nice, target, format);
            let labels: Vec<String> = scale.ticks.iter().map(|t| scale.label(*t)).collect();
            if !x.hidden {
                let first = labels.first().map(|l| tick_width(l, size)).unwrap_or(0.0);
                let last = labels.last().map(|l| tick_width(l, size)).unwrap_or(0.0);
                left = left.max(first / 2.0);
                right = width - RIGHT_PAD.max(last / 2.0);
            }
            if right - left < 20.0 {
                right = left + 20.0;
            }
            scale.r0 = left;
            scale.r1 = right;
            (Scale::Linear(scale), labels)
        }
        AxisKind::Band { labels, padding } => {
            if right - left < 20.0 {
                right = left + 20.0;
            }
            let band = Band {
                n: labels.len(),
                r0: left,
                r1: right,
                padding,
            };
            (Scale::Band(band), labels)
        }
    };

    let frame = Frame {
        left,
        top: plot_top,
        right,
        bottom: plot_bottom,
        x: x_scale,
        y: y_scale,
    };

    if let Some(title) = &y_title {
        block.text_line(
            0.0,
            top,
            truncate(title, width, size, 400),
            size,
            400,
            theme.muted,
            Anchor::Start,
        );
    }
    if !y.hidden {
        draw_y_axis(block, theme, &frame, &y_labels, gutter);
    }
    if !x.hidden {
        draw_x_axis(block, theme, &frame, &x_labels);
    }
    if let Some(title) = &x_title {
        block.text_line(
            (left + right) / 2.0,
            plot_bottom + LABEL_GAP + label_h + 4.0,
            truncate(title, width, size, 400),
            size,
            400,
            theme.muted,
            Anchor::Middle,
        );
    }
    frame
}

/// A numeric tick label's width. UI fonts draw digits at one width (about 0.6 em, "1" included),
/// so this never measures a tick narrower than that; a narrower estimate pushed "100%" past the
/// left edge of the chart.
fn tick_width(label: &str, size: f32) -> f32 {
    text_width(label, size, 400).max(label.chars().count() as f32 * 0.6 * size)
}

fn widest(labels: &[String], size: f32) -> f32 {
    labels
        .iter()
        .map(|l| text_width(l, size, 400))
        .fold(0.0, f32::max)
}

fn draw_y_axis(block: &mut Block, theme: &Theme, frame: &Frame, labels: &[String], gutter: f32) {
    let size = theme.text_size;
    let label_x = frame.left - LABEL_GAP;
    let max_label = (gutter - LABEL_GAP - 2.0).max(0.0);
    match &frame.y {
        Scale::Linear(scale) => {
            for (tick, label) in scale.ticks.iter().zip(labels) {
                let at = scale.pos(*tick);
                block.line(
                    [frame.left, crisp(at)],
                    [frame.right, crisp(at)],
                    theme.grid,
                    1.0,
                );
                block.text_centered(
                    label_x,
                    at,
                    label.clone(),
                    size,
                    400,
                    theme.muted,
                    Anchor::End,
                );
            }
        }
        Scale::Band(band) => {
            let every = thin_every(band.step().abs(), line_height(size));
            for (i, label) in labels.iter().enumerate() {
                if i % every != 0 {
                    continue;
                }
                let text = truncate(label, max_label, size, 400);
                block.text_centered(
                    label_x,
                    band.center(i),
                    text,
                    size,
                    400,
                    theme.muted,
                    Anchor::End,
                );
            }
        }
    }
}

fn draw_x_axis(block: &mut Block, theme: &Theme, frame: &Frame, labels: &[String]) {
    let size = theme.text_size;
    let label_top = frame.bottom + LABEL_GAP;
    match &frame.x {
        Scale::Linear(scale) => {
            let horizontal_bars = matches!(frame.y, Scale::Band(_));
            let positions: Vec<f32> = scale.ticks.iter().map(|t| scale.pos(*t)).collect();
            let widths: Vec<f32> = labels.iter().map(|l| tick_width(l, size)).collect();
            let every = (1..=positions.len().max(1))
                .find(|k| {
                    let shown: Vec<usize> = (0..positions.len()).step_by(*k).collect();
                    shown.windows(2).all(|pair| {
                        let (a, b) = (pair[0], pair[1]);
                        (positions[b] - positions[a]).abs() >= (widths[a] + widths[b]) / 2.0 + 8.0
                    })
                })
                .unwrap_or(1);
            for (i, (at, label)) in positions.iter().zip(labels).enumerate() {
                if horizontal_bars {
                    block.line(
                        [crisp(*at), frame.top],
                        [crisp(*at), frame.bottom],
                        theme.grid,
                        1.0,
                    );
                }
                if i % every == 0 {
                    block.text_line(
                        *at,
                        label_top,
                        label.clone(),
                        size,
                        400,
                        theme.muted,
                        Anchor::Middle,
                    );
                }
            }
        }
        Scale::Band(band) => {
            let step = band.step().abs();
            let limit = (step - 4.0).max(36.0);
            let texts: Vec<String> = labels
                .iter()
                .map(|l| truncate(l, limit, size, 400))
                .collect();
            let every = thin_every(step, widest(&texts, size) + 8.0);
            for (i, text) in texts.into_iter().enumerate() {
                if i % every != 0 {
                    continue;
                }
                block.text_line(
                    band.center(i),
                    label_top,
                    text,
                    size,
                    400,
                    theme.muted,
                    Anchor::Middle,
                );
            }
        }
    }
}

/// Show every k-th label so labels `needed` apart fit on a `step` grid.
fn thin_every(step: f32, needed: f32) -> usize {
    if step.is_nan() || step <= 0.0 {
        return usize::MAX;
    }
    let k = (needed / step).ceil();
    if k.is_finite() && k >= 1.0 {
        (k as usize).max(1)
    } else {
        1
    }
}
