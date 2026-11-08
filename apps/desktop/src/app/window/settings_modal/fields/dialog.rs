//! A dialog opened from inside Settings (`DialogContent className='ghostex-settings-shadcn ...'
//! nested`): the 12px-rounded panel on the theme's dialog surface (`SettingsPalette::dialog_surface`) with a
//! hairline edge and a 1px foreground/10 ring, 16px padding and 12px between its parts, a 16/16
//! medium title, a 14/20 muted description, the page's body and a right-aligned footer. A
//! `nested` dialog also lays its own black/65 backdrop over the whole Settings window (rail, search
//! and page); clicking the backdrop or pressing Escape dismisses it, as Base UI's `onOpenChange`.
use super::super::super::native_modal_kit::*;
use super::super::palette::SettingsPalette;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnchoredPositionMode, AnyElement, Context, FocusHandle, FontWeight, InteractiveElement as _,
    IntoElement, KeyDownEvent, MouseDownEvent, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, anchored, deferred, div, point, px,
};
use gpui_component::{h_flex, v_flex};
use std::rc::Rc;

/// `max-w-lg`: the stock shadcn dialog width.
pub(crate) const DIALOG_WIDTH: f32 = 512.0;
/// `bg-black/30` resolved by the Settings host CSS to `rgba(0, 0, 0, 0.65)`.
const BACKDROP_ALPHA: f32 = 0.65;

/// What the dialog shows around its body.
pub(crate) struct SettingsDialogSpec {
    pub(crate) id: SharedString,
    pub(crate) width: f32,
    pub(crate) title: SharedString,
    /// The `DialogDescription` (any element, so copy with emphasis can be built by the page).
    pub(crate) description: Option<AnyElement>,
    /// A control on the title's line (the QR preview's close button).
    pub(crate) header_trailing: Option<AnyElement>,
    /// `nested`: the dialog's own backdrop over the whole window.
    pub(crate) backdrop: bool,
    /// The dialog's padding and gap (`p-5 gap-4` on the QR preview; 16/12 by default).
    pub(crate) padding: f32,
    pub(crate) gap: f32,
}

impl SettingsDialogSpec {
    pub(crate) fn new(id: impl Into<SharedString>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            width: DIALOG_WIDTH,
            title: title.into(),
            description: None,
            header_trailing: None,
            backdrop: true,
            padding: 16.0,
            gap: 12.0,
        }
    }

    pub(crate) fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub(crate) fn description(mut self, description: impl IntoElement) -> Self {
        self.description = Some(description.into_any_element());
        self
    }

    pub(crate) fn header_trailing(mut self, trailing: impl IntoElement) -> Self {
        self.header_trailing = Some(trailing.into_any_element());
        self
    }

    pub(crate) fn backdrop(mut self, backdrop: bool) -> Self {
        self.backdrop = backdrop;
        self
    }

    pub(crate) fn spacing(mut self, padding: f32, gap: f32) -> Self {
        self.padding = padding;
        self.gap = gap;
        self
    }
}

/// The dialog's surface colour (`SettingsPalette::dialog_surface`).
pub(crate) fn dialog_background(p: &SettingsPalette) -> gpui::Rgba {
    p.dialog_surface()
}

/// `DialogFooter`: the buttons right-aligned with 8px between them.
pub(crate) fn dialog_footer(children: Vec<AnyElement>) -> AnyElement {
    h_flex()
        .w_full()
        .justify_end()
        .items_center()
        .gap(px(8.0))
        .children(children)
        .into_any_element()
}

/// The dialog, drawn above the whole Settings window. `focus` is tracked on the panel so Escape
/// reaches it; `on_dismiss` runs for Escape and for a click outside the panel.
pub(crate) fn settings_dialog<V: 'static>(
    p: &SettingsPalette,
    spec: SettingsDialogSpec,
    body: Vec<AnyElement>,
    footer: Option<AnyElement>,
    focus: Option<&FocusHandle>,
    on_dismiss: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    window: &mut Window,
    cx: &mut Context<V>,
) -> AnyElement {
    let on_dismiss = Rc::new(on_dismiss);
    let key_dismiss = on_dismiss.clone();
    let mouse_dismiss = on_dismiss.clone();
    let title_line = h_flex()
        .w_full()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .child(
            div()
                .min_w_0()
                .text_size(px(16.0))
                .line_height(px(16.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(hsla(p.foreground))
                .child(spec.title.clone()),
        )
        .children(spec.header_trailing);
    let header = v_flex()
        .w_full()
        .gap(px(6.0))
        .child(title_line)
        .children(spec.description.map(|description| {
            div()
                .w_full()
                .text_size(px(14.0))
                .line_height(px(20.0))
                .text_color(hsla(p.muted))
                .child(description)
        }));
    let panel = v_flex()
        .id(spec.id.clone())
        .when_some(focus, |this, focus| this.track_focus(focus))
        .occlude()
        .w(px(spec.width))
        .max_w(window.viewport_size().width - px(32.0))
        .max_h(window.viewport_size().height - px(32.0))
        .overflow_y_scroll()
        .p(px(spec.padding))
        .gap(px(spec.gap))
        .rounded(px(12.0))
        .border_1()
        .border_color(hsla(modal_rgba(0xffffff, 0.1012)))
        .bg(hsla(dialog_background(p)))
        .shadow(vec![gpui::BoxShadow {
            color: hsla(p.foreground_alpha(0.1)),
            offset: point(px(0.0), px(0.0)),
            blur_radius: px(0.0),
            spread_radius: px(1.0),
            inset: false,
        }])
        .font_family(MODAL_UI_FONT)
        .text_size(px(14.0))
        .line_height(px(20.0))
        .text_color(hsla(p.foreground))
        .on_key_down(cx.listener(move |page, event: &KeyDownEvent, window, cx| {
            if event.keystroke.key == "escape" {
                cx.stop_propagation();
                key_dismiss(page, window, cx);
            }
        }))
        .on_mouse_down_out(cx.listener(move |page, _: &MouseDownEvent, window, cx| {
            mouse_dismiss(page, window, cx);
        }))
        .child(header)
        .children(body)
        .children(footer);
    let viewport = window.viewport_size();
    deferred(
        anchored()
            .position_mode(AnchoredPositionMode::Window)
            .position(point(px(0.0), px(0.0)))
            .child(
                div()
                    .id(SharedString::from(format!("{}-backdrop", spec.id)))
                    .w(viewport.width)
                    .h(viewport.height)
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(spec.backdrop, |this| {
                        this.bg(hsla(modal_rgba(0x000000, BACKDROP_ALPHA)))
                    })
                    .occlude()
                    .child(panel),
            ),
    )
    .with_priority(2)
    .into_any_element()
}
