//! Box mode: what gxserver and the `ghostex box …` verbs do on a box (contract in
//! `ghostex_gx_protocol::cloud_box::box_side`). One file per verb or concern: `boot` (identity
//! reset, gxserver, tailcat with the allow-list, the relay for `local`), `prepare_session`,
//! `snapshot` (prepare/finish), `idle` (countdown, self-stop), `work` (D13), `logins` (injected
//! accounts, ticked secrets, write-back), `state` (what box mode keeps on the box).

pub(crate) mod boot;
mod git;
pub(crate) mod idle;
pub(crate) mod logins;
pub(crate) mod prepare_session;
pub(crate) mod relay;
pub(crate) mod snapshot;
mod state;
pub(crate) mod work;

use crate::domain::DomainStateError;

/// The `box*` actions are answered only by a gxserver running on a box.
pub(crate) fn not_in_box_mode() -> DomainStateError {
    DomainStateError {
        code: "unsupported",
        message: "This gxserver is not running on a Cloud Box.".to_string(),
    }
}

/// Whether this machine is a box (`ghostex box boot` ran here).
pub(crate) fn is_box() -> bool {
    state::ticket_file().is_file()
}

/// `boxIdleStatus`, `boxWorkStatus` and `boxWriteBack` over the box link: the same answers as the
/// `idle-status`, `work-status` and `write-back` verbs, so a connected client need not exec.
pub(crate) async fn answer_box_action(
    action: ghostex_gx_protocol::cloud_box::CloudBoxesAction,
    ack: Vec<String>,
) -> Result<serde_json::Value, DomainStateError> {
    use ghostex_gx_protocol::cloud_box::CloudBoxesAction;
    if !is_box() {
        return Err(not_in_box_mode());
    }
    let answer = tokio::task::spawn_blocking(move || match action {
        CloudBoxesAction::BoxIdleStatus => idle::run_idle_status().and_then(to_json),
        CloudBoxesAction::BoxWorkStatus => work::run_work_status().and_then(to_json),
        _ => logins::run_write_back(&ack).and_then(to_json),
    })
    .await
    .map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Box mode task failed: {error}"),
    })?;
    answer.map_err(|message| DomainStateError {
        code: "internalError",
        message,
    })
}

fn to_json<T: serde::Serialize>(value: T) -> Result<serde_json::Value, String> {
    serde_json::to_value(value).map_err(|error| error.to_string())
}
