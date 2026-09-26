//! The desktop's record lines (`gx_store/diagnostics.rs`) are disk logs gated by a diagnostic scenario. The page has no log files, so each record the shared executor files write is a no-op here, under the same name and arity.
#[derive(Default)]
pub(crate) struct GxStoreDiagnostics;

macro_rules! no_op_records {
    ($($name:ident($($arg:ident),*);)*) => {
        #[allow(dead_code, non_camel_case_types)]
        impl GxStoreDiagnostics {
            $(pub(crate) fn $name<$($arg),*>(&mut self, $(_: $arg),*) {})*
        }
    };
}

no_op_records! {
    sidebar_action_ran(a, b, c, d);
    sidebar_modal_opened(a, b, c);
    sidebar_lifecycle_ran(a, b, c, d);
    sidebar_batch_ran(a, b, c);
    sidebar_reload_ran(a, b);
    sidebar_move_ran(a, b);
    sidebar_order_write_ran(a, b, c);
    sidebar_split_ran(a, b);
    sidebar_bulk_ran(a, b);
    sidebar_snooze_action_ran(a, b);
    sidebar_snooze_ran(a, b, c, d);
    sidebar_flags_ran(a, b, c);
    sidebar_fork_ran(a, b, c);
    sidebar_close_ran(a, b, c);
    workspace_groups_write_refused(a);
    workspace_groups_read_failed(a);
    workspace_groups_read_unavailable();
    workspace_groups_write_failed(a);
    workspace_groups_pushed(a, b);
    workspace_groups_summary(a, b);
    workspace_groups_record(a, b, c);
    client_document_read_failed(a, b);
    client_document_read_unavailable(a);
    client_document_write_failed(a, b);
    client_document_write_refused(a, b);
    client_document_echo_unparsable(a);
    client_document_summary(a, b, c);
    project_move_summary(a);
    project_move_ran(a, b);
    collection_menu_ran(a, b, c);
    sidebar_open_ran(a, b, c, d);
    sidebar_state_action_ran(a, b, c, d);
}

/// A free-standing record line: nothing in the page.
#[allow(dead_code)]
pub(crate) fn record(_event: &'static str, _details: serde_json::Value) {}

/// Whether routine records are on: never in the page.
#[allow(dead_code)]
pub(crate) fn routine_logging_enabled() -> bool {
    false
}

#[allow(dead_code)]
impl GxStoreDiagnostics {
    pub(crate) fn client_document_record(
        &mut self,
        _name: &'static str,
        _ok: Option<bool>,
        _counters: super::client_document::ClientDocumentCounters,
    ) {
    }
}
