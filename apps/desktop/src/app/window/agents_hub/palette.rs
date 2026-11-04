//! The Agents Hub design tokens from packages/core-ui/styles/agents-hub.css, with the light
//! overrides in styles/modals-light.css and the window-glass steps in styles/modals-glass.css,
//! resolved on top of the shared [`ModalPalette`].
//!
//! The Hub sits one step darker than the `.gx-app-modal` language: a `#101010` pane holds
//! `#171717` cards (hover `#1b1b1b`), where the kit's panels are `#161616` / `#1d1d1d`. Those are
//! the reasons this is its own palette over the kit's.
use super::super::native_modal_kit::{ModalPalette, css_mix, modal_rgba, rgba_of};
use gpui::{Rgba, rgb};

#[derive(Clone, Copy)]
pub(crate) struct HubPalette {
    pub(crate) modal: ModalPalette,
    /// `--agents-hub-page`: the window and the dialog surface.
    pub(crate) page: Rgba,
    /// `--agents-hub-panel`: the two-pane frame.
    pub(crate) panel: Rgba,
    /// `--agents-hub-raised`: search fields, cards.
    pub(crate) raised: Rgba,
    /// `--agents-hub-raised-hover`.
    pub(crate) raised_hover: Rgba,
    /// `--agents-hub-line`: every hairline.
    pub(crate) line: Rgba,
    pub(crate) foreground: Rgba,
    pub(crate) muted: Rgba,
    /// The ink the list washes mix in: white in dark, `#262626` in light.
    pub(crate) wash: Rgba,
    /// `--background`: the outline button's fill.
    pub(crate) background: Rgba,
    /// `--muted`: the outline and ghost buttons' hover fill.
    pub(crate) muted_fill: Rgba,
    /// `--settings-focus-border-color`.
    pub(crate) focus_border: Rgba,
    /// `--settings-hairline`: button and segmented-control borders.
    pub(crate) hairline: Rgba,
    pub(crate) ok: Rgba,
    pub(crate) warn: Rgba,
    pub(crate) err: Rgba,
    /// The red status dot (`#dc2626` / `#f87171`), a step brighter than the red text.
    pub(crate) err_dot: Rgba,
    pub(crate) info: Rgba,
    /// The plan sheet's neutral verdict edge.
    pub(crate) verdict_edge: Rgba,
    /// `.agents-hub-tab-hotkey`, at rest and on the selected tab.
    pub(crate) tab_hotkey: Rgba,
    pub(crate) tab_hotkey_active: Rgba,
    pub(crate) editor: EditorColors,
    pub(crate) light: bool,
}

/// Monaco's built-in `vs-dark` and `vs` themes as the React editor drew them. Monaco outlines
/// the cursor's line (`editor.lineHighlightBorder`) and brightens its number; the Kit editor can
/// only fill that line, so the line is left plain and its number takes the text colour.
#[derive(Clone, Copy)]
pub(crate) struct EditorColors {
    pub(crate) background: Rgba,
    pub(crate) foreground: Rgba,
    pub(crate) line_number: Rgba,
    pub(crate) selection: Rgba,
    pub(crate) caret: Rgba,
}

impl HubPalette {
    pub(crate) fn resolve(modal: ModalPalette) -> Self {
        let light = modal.light;
        let glass = modal.glass;
        let (panel, raised, raised_hover, line) = if glass {
            // modals-glass.css: the Hub steps become the frosted palette's ink washes.
            (
                modal.panel,
                modal.raised,
                modal.raised_hover,
                modal.hairline,
            )
        } else if light {
            (
                rgb(0xf5f5f5),
                rgb(0xf0f0f0),
                rgb(0xe5e5e5),
                modal_rgba(0x000000, 0.14),
            )
        } else {
            (
                rgb(0x101010),
                rgb(0x171717),
                rgb(0x1b1b1b),
                modal_rgba(0xffffff, 0.08),
            )
        };
        let foreground = modal.foreground;
        let muted = modal.muted;
        let hairline = if glass {
            modal.hairline
        } else if light {
            modal_rgba(0x000000, 0.14)
        } else {
            modal_rgba(0xffffff, 0.08)
        };
        let editor = if light {
            EditorColors {
                background: if glass { panel } else { rgb(0xfffffe) },
                foreground: rgb(0x000000),
                line_number: rgb(0x237893),
                selection: rgb(0xadd6ff),
                caret: rgb(0x000000),
            }
        } else {
            EditorColors {
                // modals-glass.css paints Monaco's own background with the Hub pane under glass.
                background: if glass { panel } else { rgb(0x1e1e1e) },
                foreground: rgb(0xd4d4d4),
                line_number: rgb(0x858585),
                selection: rgb(0x264f78),
                caret: rgb(0xaeafad),
            }
        };
        Self {
            modal,
            page: modal.surface,
            panel,
            raised,
            raised_hover,
            line,
            foreground,
            muted,
            wash: if light { rgb(0x262626) } else { rgb(0xffffff) },
            background: modal.solid_surface,
            muted_fill: if light {
                rgb(0xf1f1f1)
            } else if glass {
                modal.raised_hover
            } else {
                rgb(0x262626)
            },
            focus_border: if light {
                hairline
            } else {
                css_mix(foreground, 0.58, hairline)
            },
            hairline,
            ok: if light { rgb(0x047857) } else { rgb(0x6ee7a0) },
            warn: if light { rgb(0xb45309) } else { rgb(0xfcd68a) },
            err: if light { rgb(0xb91c1c) } else { rgb(0xfda4a4) },
            err_dot: if light { rgb(0xdc2626) } else { rgb(0xf87171) },
            info: if light { rgb(0x0369a1) } else { rgb(0xbae6fd) },
            verdict_edge: if light { rgb(0x0284c7) } else { rgb(0x86d3f8) },
            tab_hotkey: rgba_of(foreground, 0.42),
            tab_hotkey_active: if light { rgb(0x404040) } else { rgb(0xc5c5c5) },
            editor,
            light,
        }
    }

    /// `color-mix(in srgb, <wash> <weight>, transparent)`: the list rows' hover and selection.
    pub(crate) fn wash(&self, weight: f32) -> Rgba {
        rgba_of(self.wash, weight)
    }

    /// `color-mix(in srgb, var(--foreground) <weight>, transparent)`.
    pub(crate) fn ink(&self, weight: f32) -> Rgba {
        rgba_of(self.foreground, weight)
    }

    /// `color-mix(in srgb, var(--agents-hub-panel) 60%, var(--agents-hub-raised))`: the opened
    /// fix row and the part-card bodies.
    pub(crate) fn detail_fill(&self) -> Rgba {
        css_mix(self.panel, 0.6, self.raised)
    }

    /// The tinted tile, pill and verdict fills: `rgba(16,185,129,.1)` and friends.
    pub(crate) fn ok_fill(&self) -> Rgba {
        modal_rgba(0x10b981, 0.10)
    }
    pub(crate) fn ok_border(&self) -> Rgba {
        modal_rgba(0x10b981, 0.40)
    }
    pub(crate) fn warn_fill(&self) -> Rgba {
        modal_rgba(0xf59e0b, 0.10)
    }
    pub(crate) fn warn_border(&self) -> Rgba {
        modal_rgba(0xf59e0b, 0.40)
    }
    pub(crate) fn err_fill(&self) -> Rgba {
        modal_rgba(0xef4444, 0.12)
    }
    pub(crate) fn err_border(&self) -> Rgba {
        modal_rgba(0xef4444, 0.40)
    }
    pub(crate) fn info_fill(&self) -> Rgba {
        modal_rgba(0x0ea5e9, 0.10)
    }
    pub(crate) fn info_border(&self) -> Rgba {
        modal_rgba(0x0ea5e9, 0.40)
    }
}
