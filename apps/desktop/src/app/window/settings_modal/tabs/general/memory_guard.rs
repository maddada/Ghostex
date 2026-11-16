//! Memory, on Power & Sleep: the memory guard's switch (on by default) and, under it while it is
//! on, whether it also notifies (CDXC:MemoryGuard 2026-11-14 in server/src/memory_guard/mod.rs).
//! gxserver reads both from the settings file on every tick (server/src/server/memory_guard_runtime.rs).
use super::super::super::fields::{PageAction, reset_key, settings_section, toggle_field_with};
use super::super::super::page::PageBlock;
use super::{GeneralCx, GeneralTab, save};
use gpui::{AnyElement, Context};
use serde_json::json;
use std::rc::Rc;

const SECTION: &str = "memoryGuard";
const ENABLED: &str = "memoryGuardEnabled";
const NOTIFY: &str = "memoryGuardNotify";

pub(super) fn memory_guard_section(
    _page: &mut GeneralTab,
    g: &GeneralCx,
    cx: &mut Context<GeneralTab>,
) -> Option<PageBlock> {
    if !g.search.subsection_visible(SECTION, g.power_visible) {
        return None;
    }
    let enabled = g.values.bool(ENABLED);
    let mut rows: Vec<AnyElement> = Vec::new();
    if g.visible(SECTION, ENABLED) {
        let spec = g.spec(
            ENABLED,
            "Stop sessions that run the computer out of memory",
            "When this computer is about to run out of memory, Ghostex stops the build, test run or dev server an agent started that uses the most, restarts an agent that grew too big, and as a last step stops the session so you can resume it. Apps outside Ghostex are never touched.",
        );
        let reset: PageAction<GeneralTab> = Rc::new(|page: &mut GeneralTab, _window, cx| {
            let store = page.store.clone();
            store.update(cx, |store, cx| {
                for key in [ENABLED, NOTIFY] {
                    store.reset_setting(key, cx);
                }
            });
        });
        rows.push(toggle_field_with(
            &g.p,
            ENABLED,
            spec,
            enabled,
            Some(reset),
            |page: &mut GeneralTab, next, _window, cx| save(page, ENABLED, json!(next), cx),
            cx,
        ));
    }
    // Hidden while the guard is off (CDXC:Settings 2026-11-14 DECISION in computer_startup.rs).
    if enabled && g.visible(SECTION, NOTIFY) {
        let spec = g
            .spec(
                NOTIFY,
                "Notify me when it steps in",
                "A notification on this computer and your phone whenever Ghostex stops or restarts something to free memory.",
            )
            .dependent();
        rows.push(toggle_field_with(
            &g.p,
            NOTIFY,
            spec,
            g.values.bool(NOTIFY),
            Some(reset_key::<GeneralTab>(NOTIFY)),
            |page: &mut GeneralTab, next, _window, cx| save(page, NOTIFY, json!(next), cx),
            cx,
        ));
    }
    settings_section(&g.p, g.section_title(SECTION, "Memory"), None, None, rows)
        .map(|section| PageBlock::section(SECTION, section))
}
