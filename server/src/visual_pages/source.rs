//! Refreshing a published page from the HTML file it came from: `ghostex show` hands gxserver the
//! file's path (publish.rs), the desktop's page window asks here whether the file changed while
//! the page is open, and on the reader's click the file is prepared again (local images inlined,
//! theme added, as `ghostex show` does) and replaces the page under the same address, so its card,
//! answers and draft stay with it.
//!
//! CDXC:SessionChat 2026-10-22 SEE-ALSO:
//! The user's decision is on apps/desktop/src/app/window/visual_page_modal/source.rs, which asks and shows the refresh button.

use std::{fs, path::Path};

use axum::http::StatusCode;
use serde_json::{json, Value};

use crate::{
    domain::{read_domain_rpc_params, DomainStateError},
    protocol::rpc_success,
    server::{domain_error_response, routed_json, AppState, RoutedResponse},
};

use super::{
    prepare::{page_too_large, prepare_page},
    store::{
        is_page_id, read_record, replace_page_html, source_modified_ms, StoreAnswersError,
        VISUAL_PAGE_MAX_BYTES,
    },
};

pub(crate) const READ_VISUAL_PAGE_SOURCE_ENDPOINT: &str = "/api/readVisualPageSource";
pub(crate) const REFRESH_VISUAL_PAGE_ENDPOINT: &str = "/api/refreshVisualPage";

/// The file the page was published from, when the record names one that still exists.
fn source_path(record: &Value) -> Option<&Path> {
    record
        .get("sourcePath")
        .and_then(Value::as_str)
        .map(Path::new)
        .filter(|path| path.is_absolute() && path.is_file())
}

fn page_id_param(body: &Value) -> Result<String, DomainStateError> {
    let params = read_domain_rpc_params(body)?;
    params
        .get("pageId")
        .and_then(Value::as_str)
        .filter(|id| is_page_id(id))
        .map(str::to_string)
        .ok_or_else(|| DomainStateError::bad_request("This call requires pageId."))
}

/// `/api/readVisualPageSource` `{pageId}`: `{pageId, refreshable, changed, questionsLeft}`.
/// `refreshable` says the page knows its file and the file is there; `changed` that the file
/// changed since the page was published or last refreshed; `questionsLeft` that a question on it
/// has no answer yet (`page_questions_left`; added 2026-11-11, so an older gxserver leaves it out);
/// `questionsAnswered` and `questionsTotal` how many of its questions the reader's draft answers
/// and how many it asks (`page_question_counts`, null for a page with no questions; added
/// 2026-11-11 for the dock's page rows).
pub(crate) fn handle_read_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let id = match page_id_param(body) {
        Ok(id) => id,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let record = read_record(&state.paths.root_dir, &id);
    let source = record.as_ref().and_then(source_path);
    let changed = match (source, &record) {
        (Some(source), Some(record)) => {
            let published = record.get("sourceModifiedMs").and_then(Value::as_u64);
            published.is_some() && source_modified_ms(source) != published
        }
        _ => false,
    };
    let counts = super::questions::page_question_counts(&state.paths.root_dir, &id);
    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(
            request_id,
            json!({
                "pageId": id,
                "refreshable": source.is_some(),
                "changed": changed,
                "questionsLeft": super::questions::page_questions_left(&state.paths.root_dir, &id),
                "questionsAnswered": counts.map(|(answered, _)| answered),
                "questionsTotal": counts.map(|(_, total)| total),
            }),
        ),
    )
}

/// `/api/refreshVisualPage` `{pageId}`: prepares the page's file again and serves it at the same
/// address; answers `{pageId, refreshed: true}`, or an error that says why the file could not be
/// used (gone, too large, a local image that cannot be embedded).
pub(crate) fn handle_refresh_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let id = match page_id_param(body) {
        Ok(id) => id,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let state_dir = &state.paths.root_dir;
    let Some(record) = read_record(state_dir, &id) else {
        return refused(endpoint_path, request_id, "notFound", "No such page.");
    };
    let Some(source) = source_path(&record) else {
        return refused(
            endpoint_path,
            request_id,
            "badRequest",
            "The page's file is gone, so it cannot be refreshed.",
        );
    };
    // Read before the file is: an edit made while it is prepared shows as a change again.
    let modified = source_modified_ms(source);
    let prepared = fs::metadata(source)
        .map_err(|error| format!("Could not read {}: {error}", source.display()))
        .and_then(|metadata| {
            if metadata.len() > VISUAL_PAGE_MAX_BYTES as u64 {
                return Err(page_too_large(
                    usize::try_from(metadata.len()).unwrap_or(usize::MAX),
                ));
            }
            fs::read_to_string(source)
                .map_err(|error| format!("Could not read {}: {error}", source.display()))
        })
        .and_then(|html| prepare_page(&html, source.parent().unwrap_or(Path::new("/"))));
    let prepared = match prepared {
        Ok(prepared) => prepared,
        Err(message) => return refused(endpoint_path, request_id, "badRequest", &message),
    };
    match replace_page_html(state_dir, &id, &prepared.html, modified) {
        Ok(()) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, json!({ "pageId": id, "refreshed": true })),
        ),
        Err(StoreAnswersError::NotFound) => {
            refused(endpoint_path, request_id, "notFound", "No such page.")
        }
        Err(StoreAnswersError::AlreadyAnswered(_)) | Err(StoreAnswersError::Io(_)) => refused(
            endpoint_path,
            request_id,
            "internalError",
            "Could not save the refreshed page.",
        ),
    }
}

fn refused(
    endpoint_path: String,
    request_id: String,
    code: &'static str,
    message: &str,
) -> RoutedResponse {
    domain_error_response(
        endpoint_path,
        request_id,
        DomainStateError {
            code,
            message: message.to_string(),
        },
    )
}
