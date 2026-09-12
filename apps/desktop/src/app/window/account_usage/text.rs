use gpui::{
    App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId,
    Pixels, ShapedLine, SharedString, Style, TextAlign, TextRun, Window, px, size,
};
use std::sync::Arc;

/// CDXC:AgentProviders 2026-09-12 WHY:
/// The usage design uses CSS letter spacing on headings and totals; ordinary GPUI text has no tracking control.
/// Adjust the shaped glyph advances so kerning and native text rendering are preserved.
pub(super) fn tracked(text: impl Into<SharedString>, spacing: f32) -> TrackedText {
    TrackedText {
        text: text.into(),
        spacing,
    }
}

pub(super) struct TrackedText {
    text: SharedString,
    spacing: f32,
}

impl IntoElement for TrackedText {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for TrackedText {
    type RequestLayoutState = (ShapedLine, Pixels);
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let text_style = window.text_style();
        let font_size = text_style.font_size.to_pixels(window.rem_size());
        let line_height = text_style
            .line_height
            .to_pixels(text_style.font_size, window.rem_size());
        let mut line = window.text_system().shape_line(
            self.text.clone(),
            font_size,
            &[TextRun {
                len: self.text.len(),
                font: text_style.font(),
                color: text_style.color,
                ..Default::default()
            }],
            None,
        );
        let mut layout = gpui::LineLayout {
            font_size: line.font_size,
            width: line.width,
            ascent: line.ascent,
            descent: line.descent,
            runs: line.runs.clone(),
            len: line.len(),
        };
        for run in &mut layout.runs {
            for glyph in &mut run.glyphs {
                glyph.position.x +=
                    px(self.text[..glyph.index].chars().count() as f32 * self.spacing);
            }
        }
        layout.width += px(self.text.chars().count() as f32 * self.spacing);
        *line = Arc::new(layout);
        let style = Style {
            size: size(line.width.into(), line_height.into()),
            ..Default::default()
        };
        (window.request_layout(style, [], cx), (line, line_height))
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Window,
        _: &mut App,
    ) {
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Err(error) = state
            .0
            .paint(bounds.origin, state.1, TextAlign::Left, None, window, cx)
        {
            eprintln!("could not paint usage text: {error}");
        }
    }
}
