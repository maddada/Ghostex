//! Family e2's picker keys: the model menu and its context, the model
//! provider, the selection outbox and the fork branch strip.
//!
//! Port of the slice of `publish` in `packages/shared/session-chat-controller/native-host.ts`
//! that read the picker's own state (`native-host.ts:482`, `:483`, `:493`, plus the
//! `modelProvider`, `modelMenuContext`, `modelSelection` and `pendingModelSelection` keys that
//! ride in on `...viewState`).

use ghostex_gx_protocol::Tri;
use serde_json::Value;

use crate::document::Document;
use crate::menus::picker::fork_branches::fork_branches_projection;
use crate::menus::picker::inputs::{menu_inputs, MenuInputs};
use crate::menus::picker::projection::model_menu_projection;
use crate::state::{ChatContext, ChatState};

/// Writes family e2's picker and menu keys into `into`.
pub fn document(state: &ChatState, context: &ChatContext, into: &mut Document) {
    // `modelMenuContext` is `computeNativeChatOptions`'s output, which is family e1's compute. It
    // is read here rather than carried on `ChatState`, so a frame can never draw a menu against a
    // stale catalog. `modelProvider` rides in from the same compute and family e1 publishes it.
    document_with(state, context, &menu_inputs(state, context), into);
}

/// [`document`] with the menu inputs family e's document already built for this frame.
pub fn document_with(
    state: &ChatState,
    context: &ChatContext,
    inputs: &MenuInputs,
    into: &mut Document,
) {
    let pickers = &state.pickers;

    into.model_menu_context = match inputs.menu.as_ref() {
        Some(menu) => Tri::Value(menu.to_json()),
        None => Tri::Null,
    };
    into.model_menu = match inputs.menu.as_ref() {
        Some(menu) => Tri::Value(model_menu_projection(
            menu,
            &pickers.model_menu_view,
            &inputs.catalogs,
            &pickers.model_favorites,
            &state.menus.model_catalog,
        )),
        None => Tri::Null,
    };
    into.model_selection = Tri::Value(pickers.model_selection.to_json());
    into.pending_model_selection = match &state.session.pending_model_selection {
        Tri::Absent => Tri::Absent,
        Tri::Null => Tri::Null,
        Tri::Value(value) => Tri::Value(value.clone()),
    };
    into.model_hold_card =
        crate::menus::picker::hold_card::model_hold_card(state, inputs.menu.as_ref());

    into.fork_branches =
        match fork_branches_projection(&pickers.fork_branches.branches, context.now_ms) {
            Value::Null => Tri::Null,
            value => Tri::Value(value),
        };
}
