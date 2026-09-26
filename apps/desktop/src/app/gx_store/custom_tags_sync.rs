//! The custom session tag catalog Settings writes, pushed to gxserver from Rust.
//!
//! CDXC:Sessions 2026-09-25 WHY:
//! Settings runs in an app-modal window, so its catalog write arrives as `updateCustomSessionTags`
//! through `dispatch_gpui_sidebar_host_message` (sidebar_dispatch.rs bounds it first), and the old
//! runtime used to perform it: this computer's catalog as a debounced write-through with a retry
//! (`queueCustomSessionTagsServerSync`), a remote machine's as one direct call down that machine's
//! tunnel (`updateRemoteCustomSessionTags`). Both are here now, with the same timers
//! (gx-core `CustomTagsPush`) and the same remote door the runtime's request went through
//! (`start_gpui_remote_sidebar_rpc`). The store does not edit the catalog locally: every reader
//! keeps drawing the daemon's copy, which arrives on the presentation stream as it did.
//!
//! SEE-ALSO: packages/gx-core/src/workspace_groups/custom_tags_push.rs,
//! apps/desktop/src/app/sidebar_dispatch.rs (`forward_gpui_custom_session_tags_update_to_sidebar`).

use std::time::Duration;

use ghostex_gx_core::{CustomTagsPush, CustomTagsPushEffect};
use serde_json::{Value, json};

use super::gx_rpc;
use crate::GhostexGpuiApp;
use crate::app::helpers::gpui_normalize_remote_machine_id;
use crate::app::remote_conn::sidebar_rpc::GpuiRemoteSidebarRpcMode;

/// `requestRemoteGxserver`'s default timeout, which `updateRemoteCustomSessionTags` did not override.
const REMOTE_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Default)]
pub(crate) struct CustomTagsSyncHost {
    push: CustomTagsPush,
    counters: CustomTagsSyncCounters,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct CustomTagsSyncCounters {
    queued: u64,
    pushes: u64,
    push_failures: u64,
    remote_writes: u64,
}

impl GhostexGpuiApp {
    /// `updateCustomSessionTags`: a remote machine's catalog goes down its tunnel at once, this
    /// computer's is queued.
    pub(crate) fn gx_store_update_custom_session_tags(
        &mut self,
        message: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(state) = message.get("state").cloned() else {
            return;
        };
        if let Some(machine_id) = message.get("remoteMachineId").and_then(Value::as_str) {
            let Some(machine_id) = gpui_normalize_remote_machine_id(machine_id) else {
                return;
            };
            self.gx_store.custom_tags.counters.remote_writes += 1;
            // The machine's answer reaches the store on its own presentation stream; the runtime's
            // copy of it was the only reader of the reply, and a refusal was never shown to anyone.
            self.start_gpui_remote_sidebar_rpc(
                &machine_id,
                "/api/updateCustomSessionTags",
                Some(json!({ "state": state })),
                REMOTE_TIMEOUT,
                GpuiRemoteSidebarRpcMode::Awaited,
                cx,
            )
            .detach();
            return;
        }
        self.gx_store.custom_tags.counters.queued += 1;
        let effect = self.gx_store.custom_tags.push.queue(state);
        self.gx_store_run_custom_tags_effect(effect, cx);
    }

    fn gx_store_run_custom_tags_effect(
        &mut self,
        effect: CustomTagsPushEffect,
        cx: &mut gpui::Context<Self>,
    ) {
        match effect {
            CustomTagsPushEffect::Arm { booking, delay_ms } => {
                cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(delay_ms))
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        if let Some(effect) = this.gx_store.custom_tags.push.timer_fired(booking) {
                            this.gx_store_run_custom_tags_effect(effect, cx);
                        }
                    });
                })
                .detach();
            }
            CustomTagsPushEffect::Push { revision, state } => {
                self.gx_store.custom_tags.counters.pushes += 1;
                cx.spawn(async move |this, cx| {
                    let result = gx_rpc(
                        None,
                        "/api/updateCustomSessionTags",
                        json!({ "state": state }),
                    )
                    .await;
                    let _ = this.update(cx, |this, cx| {
                        let ok = result.is_ok();
                        if !ok {
                            this.gx_store.custom_tags.counters.push_failures += 1;
                        }
                        if let Some(effect) =
                            this.gx_store.custom_tags.push.push_finished(revision, ok)
                        {
                            this.gx_store_run_custom_tags_effect(effect, cx);
                        }
                    });
                })
                .detach();
            }
        }
    }
}
