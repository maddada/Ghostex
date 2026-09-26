//! The More menu's two sort rows, "Last Active Sorting" (`sortLastActivity`) and "Manual Sorting"
//! (`sortManual`).
//!
//! CDXC:Sessions 2026-09-21 WHY:
//! On the desktop app both rows ENDED IN A NO-OP, so the store's answer is the empty plan. Followed to
//! the last function in the TypeScript (all deleted since; see git history): `runNativeSidebarAction`
//! (sidebar-page-frozen/navigation.ts) posted
//! `{ type: 'setActiveSessionsSortMode', sortMode, manualSessionIdsByGroup }` through the
//! controller's `post`, which was `runtime.vscode.postMessage`, which was
//! `GpuiSidebarRuntime.handleSidebarMessage` (gxserver-runtime/core.ts). That switch had no
//! `setActiveSessionsSortMode` case, so the message reached `default:` and
//! `handleUnsupportedSidebarMessage`, the documented no-op. Nothing is written to any storage key,
//! document or daemon, and the sort mode itself cannot move: both HUD builders pin
//! `activeSessionsSortMode: 'lastActivity'` (gx-core `compose_sidebar_hud` in
//! hud/mod.rs, and the hydrate in
//! app/helpers/sidebar/settings_messages_and_width.rs).
//!
//! So this file builds NO layout. A frozen order computed here would be work thrown away on every
//! click, which is what the TypeScript did. If a desktop host ever gains a
//! `setActiveSessionsSortMode` handler, the answer stops being empty and the frozen order has to be
//! built, and its source is fixed by what `createDisplaySessionLayout` reads: the DISPLAY ORDER
//! under the current mode over the FULL membership of every group of every machine (the sidebar
//! store's `sessionIdsByGroup`, `sessionsById` and `workspaceGroupIds`), hidden groups and chat
//! collections included. The tag filter, the machine filter, Space and section visibility and
//! hidden items are applied AFTER that layout, in the page's own model, so seeding from the
//! drawn rows (`SidebarView::groups`) would silently drop every row a filter hides and every group
//! of another machine. (The sort gate, `tooling/gx-core/sort-parity.ts`, guarded this while the
//! TypeScript still ran; it was deleted with it.)
//!
//! Both rows return BEFORE `runNativeSidebarAction`'s `closeAppModal`, so there is no close either.
//!
//! SEE-ALSO: packages/shared/active-sessions-sort.ts (`createDisplaySessionLayout`),
//! packages/gx-core/src/sidebar_view/ordering.rs.

use super::plan::SidebarActionPlan;

/// The two `sidebarAction` values this file answers.
pub const SORT_ACTIONS: [&str; 2] = ["sortManual", "sortLastActivity"];

/// The answer to a sort row, or `None` when the action is not one of the two.
pub fn plan_sort_action(action: &str) -> Option<SidebarActionPlan> {
    SORT_ACTIONS
        .contains(&action)
        .then(SidebarActionPlan::nothing)
}
