use super::*;
use gpui::Focusable as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, ClickEvent, Context, Div, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, ParentElement as _, Render, Rgba, SharedString, Stateful,
    StatefulInteractiveElement as _, Styled as _, Window, div, px, rgb,
};
use gpui_component::input::{Input, InputState};
use gpui_component::{Sizable as _, Size as ComponentSize, h_flex, v_flex};

/// `color-mix(in srgb, color <weight>, transparent)`: the color at `weight` of its alpha.
pub(crate) fn css_fade(color: Rgba, weight: f32) -> Rgba {
    css_mix(
        color,
        weight,
        Rgba {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        },
    )
}

/// The `.ghostex-settings-shadcn command-config-modal-shadcn` skin of the
/// legacy dialogs (Delete Worktree, Rename Worktree, Configure action): the
/// shadcn tokens as the modal host resolves them for that class. Dark values
/// come from `.ghostex-settings-shadcn` in packages/core-ui/styles/shadcn.css,
/// re-tokened by the later `.ghostex-settings-shadcn[data-sidebar-theme]`
/// block in packages/core-ui/styles.css (deleted 2026-11-06) (`--background`, `--border` and
/// `--input` become the settings surface and hairline) and the per-theme
/// `--foreground` / `--muted-foreground` in styles/theme.css; light values
/// come from the `.ghostex-settings-shadcn` block in styles/modals-light.css (deleted 2026-11-06).
#[derive(Clone, Copy)]
pub(crate) struct ModalLegacyPalette {
    /// `--app-modal-background`: the child window around the dialog.
    pub(crate) window: Rgba,
    /// `--settings-surface`, which is `--background` under this skin: the dialog itself.
    pub(crate) surface: Rgba,
    pub(crate) foreground: Rgba,
    pub(crate) muted: Rgba,
    pub(crate) primary: Rgba,
    pub(crate) primary_foreground: Rgba,
    pub(crate) border: Rgba,
    pub(crate) input: Rgba,
    /// `--settings-raised`: the text input fill (at 82%).
    pub(crate) raised: Rgba,
    /// `--settings-focus-border-color` (dark) or the light `#525252` focus rule.
    pub(crate) focus_border: Rgba,
    pub(crate) destructive: Rgba,
    /// The modal's window is frosted (`ModalPalette::frosted`).
    pub(crate) glass: bool,
    pub(crate) light: bool,
}

impl ModalLegacyPalette {
    pub(crate) fn resolve(p: &ModalPalette) -> Self {
        let lp = if p.light {
            Self {
                window: p.surface,
                surface: p.surface,
                foreground: p.foreground,
                muted: p.muted,
                primary: p.primary,
                primary_foreground: p.primary_foreground,
                border: p.hairline,
                input: modal_rgba(0x000000, 0.16),
                raised: rgb(0xf0f0f0),
                focus_border: rgb(0x525252),
                destructive: p.destructive,
                glass: false,
                light: true,
            }
        } else {
            // styles/theme.css publishes `--app-modal-background` as #191919 for
            // dark-1 and #0e0e0e for every other dark theme; the palette's
            // `background` is `--app-background`, which only coincides with it
            // for dark-1 and dark-2, so the colored dark themes keep the #0e0e0e window.
            let window = if p.background == rgb(0x191919) {
                p.background
            } else {
                p.surface
            };
            Self {
                window,
                surface: p.surface,
                foreground: p.foreground,
                muted: p.muted,
                primary: p.primary,
                primary_foreground: p.primary_foreground,
                border: p.hairline,
                input: p.hairline,
                raised: rgb(0x161616),
                // `color-mix(in srgb, var(--foreground) 58%, var(--border) 42%)`.
                focus_border: css_mix(p.foreground, 0.58, p.hairline),
                // shadcn's dark `--destructive: oklch(0.704 0.191 22.216)`, Tailwind red-400.
                destructive: rgb(0xff6467),
                glass: false,
                light: false,
            }
        };
        if !p.glass {
            return lp;
        }
        // Under window glass the window's frosted fill is the only layer: the dialog drawn on it
        // is clear, and its fixed raised tones become the kit's ink washes.
        Self {
            window: p.surface,
            surface: css_fade(p.surface, 0.0),
            raised: p.raised,
            glass: true,
            ..lp
        }
    }

    /// `color-mix(in srgb, var(--foreground) <weight>, var(--muted-foreground))`: the skin's text blends.
    pub(crate) fn text(&self, foreground_weight: f32) -> Rgba {
        css_mix(self.foreground, foreground_weight, self.muted)
    }

    /// `color-mix(in srgb, var(--border) 82%, transparent)`: the header and footer dividers.
    pub(crate) fn divider(&self) -> Rgba {
        css_fade(self.border, 0.82)
    }
}

/// The legacy skin's square checkbox (`.worktree-*-branch-checkbox`): 16px,
/// no radius, `--input` border over a 30% `--input` fill; `--primary` fill with
/// a `--primary-foreground` tick when checked; half opacity when disabled.
pub(crate) fn modal_square_checkbox(
    lp: &ModalLegacyPalette,
    checked: bool,
    disabled: bool,
) -> AnyElement {
    div()
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .size(px(16.0))
        .border_1()
        .border_color(hsla(if checked { lp.primary } else { lp.input }))
        .bg(hsla(if checked {
            lp.primary
        } else {
            css_fade(lp.input, 0.30)
        }))
        .when(disabled, |this| this.opacity(0.5))
        .when(checked, |this| {
            this.child(modal_icon(
                "modals/kit/legacy-check.svg",
                16.0,
                lp.primary_foreground,
            ))
        })
        .into_any_element()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModalLegacyButtonTone {
    /// shadcn `variant='outline'`, drawn with the neutral fill (CDXC:AppModal 2026-10-14 on
    /// [`neutral_button_fills`]).
    Outline,
    /// shadcn `variant='default'`. This skin has no white buttons, so it takes the same neutral
    /// fill as `Outline`.
    Primary,
    /// shadcn `variant='destructive'`: the destructive fill.
    Destructive,
}

/// A shadcn `Button` inside the legacy skin: 32px tall, 8px radius, 12px
/// side padding, 14px label at the skin's flat 400 weight (`.ghostex-settings-shadcn
/// .ghostex-settings-shadcn * { font-weight: 400 }`), `min-width: 112px` as the
/// worktree dialogs' footer buttons set it, content-sized and right-aligned by the footer.
pub(crate) fn modal_legacy_action_button<V: 'static>(
    lp: &ModalLegacyPalette,
    id: &'static str,
    label: impl Into<SharedString>,
    tone: ModalLegacyButtonTone,
    disabled: bool,
    on_click: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let lp = *lp;
    let (fills, text) = match tone {
        ModalLegacyButtonTone::Outline | ModalLegacyButtonTone::Primary => {
            (neutral_button_fills(lp.light), lp.foreground)
        }
        ModalLegacyButtonTone::Destructive => (
            destructive_button_fills(lp.destructive, lp.light),
            lp.destructive,
        ),
    };
    h_flex()
        .id(id)
        .flex_shrink_0()
        .min_w(px(112.0))
        .h(px(MODAL_FOOTER_BUTTON_HEIGHT))
        .px(px(12.0))
        .gap(px(6.0))
        .items_center()
        .justify_center()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(transparent())
        .bg(hsla(fills.rest))
        .text_size(px(14.0))
        .line_height(px(20.0))
        .text_color(hsla(text))
        .whitespace_nowrap()
        .when(disabled, |this| this.opacity(0.5).cursor_default())
        .when(!disabled, |this| {
            this.cursor_pointer()
                .hover(move |this| this.bg(hsla(fills.hover)))
                .active(move |this| this.bg(hsla(fills.pressed)))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    on_click(this, window, cx);
                }))
        })
        .child(label.into())
        .into_any_element()
}

/// The shadcn `Input` inside the legacy skin (`.ghostex-settings-shadcn
/// [data-slot='input']`): 32px tall, 8px radius, the settings raised tone at
/// 82% with a hairline border that turns into the focus border while focused.
pub(crate) fn modal_legacy_text_input(
    lp: &ModalLegacyPalette,
    state: &gpui::Entity<InputState>,
    disabled: bool,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let focused = state.read(cx).focus_handle(cx).is_focused(window);
    div()
        .w_full()
        .min_w_0()
        .h(px(MODAL_CONTROL_HEIGHT))
        .px(px(12.0))
        .flex()
        .items_center()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(if focused { lp.focus_border } else { lp.border }))
        .bg(hsla(css_fade(lp.raised, 0.82)))
        .when(disabled, |this| this.opacity(0.5))
        .child(
            div().flex_1().min_w_0().child(
                Input::new(state)
                    .with_size(ComponentSize::Small)
                    .appearance(false)
                    .bordered(false)
                    .focus_bordered(false)
                    .disabled(disabled)
                    .w_full()
                    .px(px(0.0))
                    .py(px(0.0))
                    .text_size(px(14.0))
                    .text_color(hsla(lp.foreground)),
            ),
        )
        .into_any_element()
}

/// The legacy dialog frame (`.command-config-modal-shadcn` in a native child
/// window): the dialog is narrower than the window and sits centered at its
/// top on the `--app-modal-background` window color, a 12px-radius surface
/// with a window-colored 1px edge, a 20px-padded header over a hairline
/// divider, the caller's body, and a right-aligned footer (10px gaps, padding
/// 14px 20px 20px) under another divider. The window fits itself to the
/// dialog's measured height, nothing added.
pub(crate) fn modal_legacy_shell<V: Render>(
    lp: &ModalLegacyPalette,
    id: &'static str,
    dialog_width: f32,
    focus_handle: &FocusHandle,
    fit: &ModalFit,
    on_key_down: impl Fn(&mut V, &KeyDownEvent, &mut Window, &mut Context<V>) + 'static,
    header: Div,
    body: AnyElement,
    footer_buttons: Vec<AnyElement>,
    cx: &mut Context<V>,
) -> Stateful<Div> {
    let divider = hsla(lp.divider());
    div()
        .id(id)
        .size_full()
        .overflow_hidden()
        .bg(hsla(lp.window))
        .font_family(MODAL_UI_FONT)
        .text_size(px(14.0))
        .line_height(px(20.0))
        .text_color(hsla(lp.foreground))
        .track_focus(focus_handle)
        .on_key_down(cx.listener(on_key_down))
        .child(
            v_flex()
                .size_full()
                .items_center()
                .on_children_prepainted(fit.listener(0.0))
                .child(
                    v_flex()
                        .w(px(dialog_width))
                        .flex_shrink_0()
                        .rounded(px(MODAL_RADIUS_SECTION))
                        .border_1()
                        .border_color(if lp.glass {
                            transparent()
                        } else {
                            hsla(lp.window)
                        })
                        .bg(hsla(lp.surface))
                        .overflow_hidden()
                        .child(
                            header
                                .w_full()
                                .p(px(20.0))
                                .border_b_1()
                                .border_color(divider),
                        )
                        .child(body)
                        .child(
                            h_flex()
                                .w_full()
                                .flex_wrap()
                                .justify_end()
                                .gap(px(10.0))
                                .pt(px(14.0))
                                .px(px(20.0))
                                .pb(px(20.0))
                                .border_t_1()
                                .border_color(divider)
                                .children(footer_buttons),
                        ),
                ),
        )
}

// ---------------------------------------------------------------------------
// Added with the Add Worktree modal: the raised segmented control, the
// searchable select popup, per-modal field skins, and an inset shell.
// ---------------------------------------------------------------------------
