//! The Add Project dialog's colours and small chrome pieces. The dialog is a command palette,
//! not a `.gx-app-modal` form, so its tokens are the shadcn ones the React `CommandDialog`
//! resolves (`--border`, `--input`, `--muted`) plus the `.add-project-modal` overrides in
//! packages/core-ui/styles/modals.css (deleted 2026-11-06) and modals-light.css (deleted 2026-11-06), measured from the Storybook story.
use super::super::native_modal_kit::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Div, FontWeight, IntoElement, Keystroke, ParentElement as _, Rgba, SharedString,
    Styled, div, point, px, rgb,
};
use gpui_component::h_flex;
use gpui_component::kbd::Kbd;

#[derive(Clone, Copy)]
pub(super) struct AddProjectSkin {
    pub(super) p: ModalPalette,
    /// `--border`: hairlines and the review cards.
    pub(super) border: Rgba,
    /// `--input`: the branch field and the checkboxes.
    pub(super) input: Rgba,
    /// `--muted`: the highlighted row, key caps and hovered ghost buttons.
    pub(super) muted_fill: Rgba,
    /// `.add-project-modal [data-slot='input-group']`.
    pub(super) bar_background: Rgba,
    pub(super) bar_border: Rgba,
    pub(super) bar_focus_border: Rgba,
    /// `--destructive` as the dialog resolves it (red-400 dark, red-700 light).
    pub(super) destructive: Rgba,
    /// `--ring`.
    pub(super) ring: Rgba,
}

impl AddProjectSkin {
    pub(super) fn resolve(p: ModalPalette) -> Self {
        let (border, input, muted_fill, destructive) = if p.light {
            (
                modal_rgba(0x000000, 0.14),
                modal_rgba(0x000000, 0.16),
                rgb(0xf1f1f1),
                rgb(0xb91c1c),
            )
        } else {
            (
                modal_rgba(0xffffff, 0.11),
                modal_rgba(0xffffff, 0.15),
                rgb(0x2a2a2a),
                rgb(0xff6467),
            )
        };
        let (bar_background, bar_border, bar_focus_border) = if p.light {
            (
                rgb(0xf5f5f5),
                modal_rgba(0x000000, 0.16),
                modal_rgba(0x000000, 0.16),
            )
        } else {
            (
                rgb(0x161616),
                modal_rgba(0xffffff, 0.08),
                modal_rgba(0xffffff, 0.28),
            )
        };
        let skin = Self {
            p,
            border,
            input,
            muted_fill,
            bar_background,
            bar_border,
            bar_focus_border,
            destructive,
            ring: rgb(0x737373),
        };
        /*
        CDXC:Theming 2026-09-27 DECISION: User: "yes pls make all those that could be glass glass".
        Under window glass the dialog is a frosted surface, so the path bar and the highlighted
        row take the kit's ink washes instead of solid slabs: the bar sits one step above the
        surface (the React glass sheet's +3% `--app-dropdown-background`), its submit button one
        more (`raised`), and the highlight is the accent wash.
        */
        if p.glass {
            return Self {
                muted_fill: p.accent,
                bar_background: p.panel,
                bar_border: p.hairline,
                ..skin
            };
        }
        skin
    }

    pub(super) fn fg(&self) -> Rgba {
        self.p.foreground
    }

    pub(super) fn muted(&self) -> Rgba {
        self.p.muted
    }

    /// `text-muted-foreground/<alpha>`.
    pub(super) fn muted_at(&self, alpha: f32) -> Rgba {
        css_fade(self.p.muted, alpha)
    }

    /// `border-border/<alpha>`.
    pub(super) fn border_at(&self, alpha: f32) -> Rgba {
        css_fade(self.border, alpha)
    }

    /// `bg-muted/<alpha>`.
    pub(super) fn muted_fill_at(&self, alpha: f32) -> Rgba {
        css_fade(self.muted_fill, alpha)
    }

    /// The one fill every button in this dialog takes, in both appearances, with its label in
    /// `button_text`: a solid near-black that reads as a button on the light dialog and on the
    /// dark path bar alike, lighter on hover and again while pressed.
    ///
    /// CDXC:AddProject 2026-10-14 DECISION:
    /// User: "I want the buttons in the Add Project dialog to not be white; they should be a darker background color", then: the buttons' background should be the same colour in light and dark mode, darker still than the dark-mode fill, with a bit heavier text, and an icon on Create & Add. Every Add Project button (the path bar's actions, the clone review's footer, Setup Required / Install) is this solid #262626 with #f5f5f5 text, one font weight heavier than before. This supersedes the 2026-10-02 white bar buttons; the bar's actions still share one look.
    pub(super) fn button_fills(&self) -> ButtonFills {
        ButtonFills {
            rest: rgb(0x262626),
            hover: rgb(0x303030),
            pressed: rgb(0x3a3a3a),
        }
    }

    /// The label colour on `button_fills`.
    pub(super) fn button_text(&self) -> Rgba {
        rgb(0xf5f5f5)
    }

    /// A fill for a control that can carry a ring, on the clone-options panel. gpui paints a box
    /// shadow under the element, so a translucent fill would let the ring colour through; on the
    /// opaque dialog the fill is composited onto the panel colour instead. Under window glass the
    /// fill stays translucent and the ring draws no halo (`ring_halo`).
    pub(super) fn on_options_panel(&self, fill: Rgba) -> Rgba {
        if self.p.glass {
            return fill;
        }
        over(fill, over(self.muted_fill_at(0.1), self.p.solid_surface))
    }

    /// The 3px ring halo of `color` at 20%, or none under window glass (see `on_options_panel`).
    pub(super) fn ring_halo(&self, color: Rgba) -> Vec<gpui::BoxShadow> {
        if self.p.glass {
            return Vec::new();
        }
        vec![gpui::BoxShadow {
            color: hsla(css_fade(color, 0.2)),
            offset: point(px(0.0), px(0.0)),
            blur_radius: px(0.0),
            spread_radius: px(3.0),
            inset: false,
        }]
    }

    /// The shadcn focus ring: `border-ring` plus a 3px `ring-ring/20` halo.
    pub(super) fn focus_ring<E: Styled>(&self, element: E, focused: bool) -> E {
        if !focused {
            return element;
        }
        element
            .border_color(hsla(self.ring))
            .shadow(self.ring_halo(self.ring))
    }
}

/// `top` composited over the opaque `bottom`.
pub(super) fn over(top: Rgba, bottom: Rgba) -> Rgba {
    css_mix(Rgba { a: 1.0, ..top }, top.a, Rgba { a: 1.0, ..bottom })
}

/// The dialog's `text-sm` line: 14px on a 20px line.
pub(super) fn text_sm(element: Div) -> Div {
    element.text_size(px(14.0)).line_height(px(20.0))
}

/// `AddProjectFooterHint`: key caps then the label, 6px apart; each space-separated key is its own cap.
pub(super) fn footer_hint(
    skin: &AddProjectSkin,
    keys: &str,
    label: impl Into<SharedString>,
) -> AnyElement {
    let caps = keys.split(' ').filter(|key| !key.is_empty()).map(|key| {
        Kbd::new(Keystroke {
            modifiers: Default::default(),
            key: key.to_string(),
            key_char: None,
        })
        .flex()
        .items_center()
        .justify_center()
        .h(px(20.0))
        .min_w(px(20.0))
        .px(px(6.0))
        .py(px(0.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(hsla(skin.border_at(0.7)))
        .bg(hsla(skin.muted_fill_at(0.6)))
        .text_color(hsla(skin.muted()))
        .text_size(px(14.0))
        .line_height(px(14.0))
        .font_family(crate::ui_fonts::UI_FONT)
    });
    h_flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(6.0))
        .whitespace_nowrap()
        .child(h_flex().items_center().gap(px(4.0)).children(caps))
        .child(label.into())
        .into_any_element()
}

/// The persistent error region: `border-destructive/40 bg-destructive/10`, 8px radius, a 14px
/// alert icon and the message.
pub(super) fn error_region(skin: &AddProjectSkin, message: &str) -> Div {
    h_flex()
        .items_start()
        .gap(px(8.0))
        .px(px(12.0))
        .py(px(8.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(hsla(css_fade(skin.destructive, 0.4)))
        .bg(hsla(css_fade(skin.destructive, 0.1)))
        .text_size(px(14.0))
        .line_height(px(20.0))
        .text_color(hsla(skin.destructive))
        .child(div().flex_shrink_0().mt(px(3.0)).child(modal_icon(
            super::window::ICON_ALERT,
            14.0,
            skin.destructive,
        )))
        .child(div().flex_1().min_w_0().child(message.to_string()))
}

/// `MiddleEllipsisText`: the tail (the last path segment when it is at most 24 characters, else
/// the last 24) stays whole and the head truncates.
pub(super) fn middle_ellipsis(value: &str) -> Div {
    const MAX_PRESERVED_END: usize = 24;
    let characters: Vec<char> = value.chars().collect();
    let separator = characters
        .iter()
        .rposition(|character| matches!(character, '/' | '\\'));
    let split = match separator {
        Some(index) if characters.len() - (index + 1) <= MAX_PRESERVED_END => index + 1,
        _ => characters.len().saturating_sub(MAX_PRESERVED_END).max(1),
    }
    .min(characters.len());
    let head: String = characters[..split].iter().collect();
    let tail: String = characters[split..].iter().collect();
    h_flex()
        .min_w_0()
        .whitespace_nowrap()
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .child(head),
        )
        .child(div().flex_shrink_0().child(tail))
}

/// The dialog's square shadcn checkbox (`rounded-none`): 16px, `--input` border, the primary
/// fill with a 14px check when checked.
pub(super) fn square_checkbox(skin: &AddProjectSkin, checked: bool, focused: bool) -> AnyElement {
    let p = skin.p;
    let base = div()
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .size(px(16.0))
        .border_1()
        .border_color(hsla(if checked { p.primary } else { skin.input }))
        .when(!checked && focused, |this| {
            this.bg(hsla(skin.on_options_panel(css_fade(skin.input, 0.0))))
        })
        .when(checked, |this| {
            this.bg(hsla(p.primary)).child(modal_icon(
                super::window::ICON_CHECK,
                14.0,
                p.primary_foreground,
            ))
        });
    skin.focus_ring(base, focused).into_any_element()
}

/// Section label copy: 14px/500 muted.
pub(super) fn label_medium(skin: &AddProjectSkin, text: impl Into<SharedString>) -> Div {
    text_sm(div())
        .font_weight(FontWeight::MEDIUM)
        .text_color(hsla(skin.muted()))
        .child(text.into())
}
