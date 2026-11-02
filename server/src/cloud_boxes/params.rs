//! Reading typed params, and the errors the route answers with.

use ghostex_gx_protocol::cloud_box::error_codes;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use crate::domain::DomainStateError;

/// The action's params as their contract type (`ghostex_gx_protocol::cloud_box::rpc`).
pub(super) fn typed<T: DeserializeOwned>(
    params: &Map<String, Value>,
) -> Result<T, DomainStateError> {
    serde_json::from_value(Value::Object(params.clone()))
        .map_err(|error| DomainStateError::bad_request(format!("Bad Cloud Boxes params: {error}")))
}

pub(super) fn to_value<T: serde::Serialize>(value: &T) -> Result<Value, DomainStateError> {
    serde_json::to_value(value).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Couldn't write the Cloud Boxes answer: {error}"),
    })
}

pub(super) fn internal(message: impl Into<String>) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: message.into(),
    }
}

pub(super) fn provider_unavailable(message: impl Into<String>) -> DomainStateError {
    DomainStateError {
        code: error_codes::PROVIDER_UNAVAILABLE,
        message: message.into(),
    }
}

pub(super) fn provider_error(message: impl Into<String>) -> DomainStateError {
    DomainStateError {
        code: error_codes::PROVIDER_ERROR,
        message: message.into(),
    }
}

pub(super) fn wrong_state(message: impl Into<String>) -> DomainStateError {
    DomainStateError {
        code: error_codes::WRONG_STATE,
        message: message.into(),
    }
}

pub(super) fn not_connected(message: impl Into<String>) -> DomainStateError {
    DomainStateError {
        code: error_codes::PROVIDER_NOT_CONNECTED,
        message: message.into(),
    }
}

pub(super) async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, DomainStateError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| internal(format!("Cloud Boxes task failed: {error}")))
}
