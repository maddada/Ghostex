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
use crate::document::SharedValue;
use crate::menus::catalog::AgentModelCatalog;
use crate::menus::picker::fork_branches::fork_branches_projection;
use crate::menus::picker::inputs::{menu_inputs, MenuInputs};
use crate::menus::picker::model_menu::{ModelMenuCatalogs, ModelMenuView};
use crate::menus::picker::projection::{model_menu_projection, ModelMenuContext};
use crate::state::{ChatContext, ChatState, Stamped};

/// The model menu with the five values it was projected from (`document::AssembleCache`).
///
/// CDXC:SessionChat 2026-11-10 WHY: the model menu (every provider's tab with its models, its search and its favourites) was projected again on every event of every chat although the menu is closed nearly always, 8 to 20% of the chat host thread in the live profile with nine agents working. The projection reads nothing but its five arguments and no clock, so it is kept while they hold: the agent model catalog by stamp ([`Stamped`]), the provider sweep by pointer (it is shared while its key holds, `picker::inputs`), and the three small values by value. Each check falls back to comparing values, so nothing can serve a stale menu.
#[derive(Clone, Debug)]
pub struct KeptModelMenu {
    context: ModelMenuContext,
    view: ModelMenuView,
    catalogs: std::sync::Arc<ModelMenuCatalogs>,
    favorites: Vec<String>,
    catalog: Stamped<AgentModelCatalog>,
    menu: SharedValue,
}

impl KeptModelMenu {
    fn matches(
        &mut self,
        context: &ModelMenuContext,
        view: &ModelMenuView,
        catalogs: &std::sync::Arc<ModelMenuCatalogs>,
        favorites: &[String],
        catalog: &Stamped<AgentModelCatalog>,
    ) -> bool {
        if self.context != *context || self.view != *view || self.favorites != favorites {
            return false;
        }
        if !std::sync::Arc::ptr_eq(&self.catalogs, catalogs) {
            if *self.catalogs != **catalogs {
                return false;
            }
            self.catalogs = std::sync::Arc::clone(catalogs);
        }
        self.catalog.same_as(catalog)
    }
}

/// [`model_menu_projection`], reused while its five arguments are the ones `kept` was built from.
fn model_menu_kept(
    state: &ChatState,
    context: &ModelMenuContext,
    catalogs: &std::sync::Arc<ModelMenuCatalogs>,
    kept: &mut Option<KeptModelMenu>,
) -> SharedValue {
    let pickers = &state.pickers;
    if let Some(kept) = kept.as_mut() {
        if kept.matches(
            context,
            &pickers.model_menu_view,
            catalogs,
            &pickers.model_favorites,
            &state.menus.model_catalog,
        ) {
            return kept.menu.clone();
        }
    }
    let menu = SharedValue::from(model_menu_projection(
        context,
        &pickers.model_menu_view,
        catalogs,
        &pickers.model_favorites,
        &state.menus.model_catalog,
    ));
    *kept = Some(KeptModelMenu {
        context: context.clone(),
        view: pickers.model_menu_view.clone(),
        catalogs: std::sync::Arc::clone(catalogs),
        favorites: pickers.model_favorites.clone(),
        catalog: state.menus.model_catalog.clone(),
        menu: menu.clone(),
    });
    menu
}

/// Writes family e2's picker and menu keys into `into`.
pub fn document(state: &ChatState, context: &ChatContext, into: &mut Document) {
    // `modelMenuContext` is `computeNativeChatOptions`'s output, which is family e1's compute. It
    // is read here rather than carried on `ChatState`, so a frame can never draw a menu against a
    // stale catalog. `modelProvider` rides in from the same compute and family e1 publishes it.
    document_with(
        state,
        context,
        &menu_inputs(state, context),
        into,
        &mut None,
    );
}

/// [`document`] with the menu inputs family e's document already built for this frame.
pub fn document_with(
    state: &ChatState,
    context: &ChatContext,
    inputs: &MenuInputs,
    into: &mut Document,
    kept: &mut Option<KeptModelMenu>,
) {
    let pickers = &state.pickers;

    into.model_menu_context = match inputs.menu.as_ref() {
        Some(menu) => Tri::Value(menu.to_json()),
        None => Tri::Null,
    };
    into.model_menu = match inputs.menu.as_ref() {
        Some(menu) => Tri::Value(model_menu_kept(state, menu, &inputs.catalogs, kept)),
        None => {
            *kept = None;
            Tri::Null
        }
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
