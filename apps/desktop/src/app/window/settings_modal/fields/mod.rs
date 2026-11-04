//! The Settings field library: the native twins of packages/core-ui/settings-modal/fields.tsx (deleted 2026-10-01)
//! (and the row pieces of app-tooltip.tsx and disabled-setting-control-tooltip.tsx (deleted 2026-10-01)), drawn on
//! GPUI-Kit (`Input`, `Slider` state, `Tooltip`) and the shared app modal kit (`ModalSelect`).
//!
//! Every page entity implements [`SettingsPage`]; the field functions take the page, build their
//! element with listeners on it, and save through its store. Widget state that must outlive a
//! render (text buffers, slider positions, open dropdowns) lives in the page's [`FieldStates`].
pub(crate) mod advanced;
pub(crate) mod checkbox;
pub(crate) mod chips;
pub(crate) mod color;
pub(crate) mod controls;
pub(crate) mod diagnostic_logging;
pub(crate) mod dialog;
pub(crate) mod dialog_sheet;
pub(crate) mod font_picker;
pub(crate) mod hotkey_recorder;
pub(crate) mod hover_actions;
pub(crate) mod icon_picker;
pub(crate) mod ignored_ports;
pub(crate) mod media;
pub(crate) mod qr;
pub(crate) mod reorder;
pub(crate) mod row;
pub(crate) mod searchable_dropdown;
pub(crate) mod searchable_list;
pub(crate) mod searchable_select;
pub(crate) mod segmented;
pub(crate) mod select;
pub(crate) mod sized_button;
pub(crate) mod slider;
pub(crate) mod stock_segmented;
pub(crate) mod tag_list;
pub(crate) mod text;
pub(crate) mod textarea;

pub(crate) use advanced::*;
pub(crate) use checkbox::*;
pub(crate) use chips::*;
pub(crate) use color::*;
pub(crate) use controls::*;
pub(crate) use diagnostic_logging::*;
pub(crate) use dialog::*;
pub(crate) use dialog_sheet::*;
pub(crate) use font_picker::*;
pub(crate) use hotkey_recorder::*;
pub(crate) use hover_actions::*;
pub(crate) use icon_picker::*;
pub(crate) use ignored_ports::*;
pub(crate) use media::*;
pub(crate) use qr::*;
pub(crate) use reorder::*;
pub(crate) use row::*;
pub(crate) use searchable_dropdown::*;
pub(crate) use searchable_list::*;
pub(crate) use searchable_select::*;
pub(crate) use segmented::*;
pub(crate) use select::*;
pub(crate) use sized_button::*;
pub(crate) use slider::*;
pub(crate) use stock_segmented::*;
pub(crate) use tag_list::*;
pub(crate) use text::*;
pub(crate) use textarea::*;

use super::store::SettingsStore;
use gpui::{Context, Entity, SharedString, Subscription};
use std::collections::HashMap;

/// Implemented by every Settings page entity so the shared fields can read and save through
/// the page's store and keep their widget state on the page.
pub(crate) trait SettingsPage: 'static + Sized {
    fn settings_store(&self) -> &Entity<SettingsStore>;
    fn field_states(&mut self) -> &mut FieldStates;

    /// Called once a drag of item `index` in reorder list `list` is about to lift it. A page whose
    /// item is taller while open closes it here and returns true; the list then waits for the
    /// item's new layout before lifting it, so the lifted row is the closed one.
    fn reorder_prepare_lift(
        &mut self,
        _list: &str,
        _index: usize,
        _cx: &mut Context<Self>,
    ) -> bool {
        false
    }
}

/// The widget state of one page's fields, created on first render and keyed by field id
/// (usually the setting key).
#[derive(Default)]
pub(crate) struct FieldStates {
    pub(crate) selects: HashMap<SharedString, SelectState>,
    pub(crate) sliders: HashMap<SharedString, SliderFieldState>,
    pub(crate) texts: HashMap<SharedString, TextFieldState>,
    pub(crate) textareas: HashMap<SharedString, TextareaFieldState>,
    /// Drag-to-reorder lists in progress, keyed by list id.
    pub(crate) reorders: HashMap<SharedString, ReorderState>,
    pub(crate) colors: HashMap<SharedString, ColorFieldState>,
    pub(crate) ignored_ports: Option<IgnoredPortsState>,
    pub(crate) tag_list: Option<TagListState>,
    pub(crate) hover_actions: HoverActionsState,
    pub(crate) diagnostic_logging: DiagnosticLoggingState,
    /// The one dropdown open on the page.
    pub(crate) open_select: Option<SharedString>,
    /// A dropdown to open on its next render (the preview binary's `select` state).
    pub(crate) pending_open_select: Option<SharedString>,
    pub(crate) subscriptions: Vec<Subscription>,
}

impl FieldStates {
    /// Closes the open dropdown, if any.
    pub(crate) fn close_select(&mut self) {
        if let Some(id) = self.open_select.take()
            && let Some(select) = self.selects.get_mut(&id)
        {
            select.select.close();
        }
    }
}

/// The icon paths the fields draw (Tabler outline icons, copied into assets/modals/settings/).
pub(crate) mod icon {
    pub(crate) const ALERT_TRIANGLE: &str = "modals/settings/alert-triangle.svg";
    pub(crate) const ASTERISK: &str = "modals/settings/asterisk.svg";
    pub(crate) const CHEVRON_DOWN: &str = "modals/settings/chevron-down.svg";
    pub(crate) const CHEVRON_RIGHT: &str = "modals/settings/chevron-right.svg";
    pub(crate) const DOWNLOAD: &str = "modals/settings/download.svg";
    pub(crate) const EYE: &str = "modals/settings/eye.svg";
    pub(crate) const EYE_OFF: &str = "modals/settings/eye-off.svg";
    pub(crate) const FOLDER_OPEN: &str = "modals/settings/folder-open.svg";
    pub(crate) const GRIP_VERTICAL: &str = "modals/settings/grip-vertical.svg";
    pub(crate) const INFO_CIRCLE: &str = "modals/settings/info-circle.svg";
    pub(crate) const MINUS: &str = "modals/settings/minus.svg";
    pub(crate) const PALETTE: &str = "modals/settings/palette.svg";
    pub(crate) const PHOTO: &str = "modals/settings/photo.svg";
    pub(crate) const PLAYER_PLAY: &str = "modals/settings/player-play.svg";
    pub(crate) const PLUS: &str = "modals/settings/plus.svg";
    pub(crate) const SEARCH: &str = "modals/settings/search.svg";
    pub(crate) const SELECTOR: &str = "modals/settings/selector.svg";
    pub(crate) const TRASH: &str = "modals/settings/trash.svg";
    pub(crate) const X: &str = "modals/settings/x.svg";
}
