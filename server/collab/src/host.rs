//! The component's calls into gxserver (`POST /api/collabHost` with the host token gxserver gave
//! it at start), and gxserver's address for the guest reads the connector replays.
//!
//! Tests swap in their own [`Host`] with [`set`].

use std::sync::{Arc, Mutex, OnceLock, PoisonError, RwLock};
use std::time::{Duration, Instant};

use ghostex_gx_protocol::collab_component::{HostCall, COLLAB_HOST_ENDPOINT};
use serde_json::{json, Value};

pub trait Host: Send + Sync {
    /// The call's `result`, or the error message gxserver answered with.
    fn call(&self, call: &HostCall) -> Result<Value, String>;
}

/// gxserver over its loopback listener.
pub struct HttpHost {
    pub base_url: String,
    pub token: String,
}

impl Host for HttpHost {
    fn call(&self, call: &HostCall) -> Result<Value, String> {
        let response = crate::http::loopback_agent()
            .post(format!("{}{COLLAB_HOST_ENDPOINT}", self.base_url))
            .header("authorization", format!("Bearer {}", self.token))
            .send_json(json!({ "params": call }))
            .map_err(|error| format!("gxserver couldn't be reached: {error}"))?;
        let ok = response.status().is_success();
        let body: Value = serde_json::from_reader(response.into_body().into_reader())
            .map_err(|error| format!("gxserver answered something unreadable: {error}"))?;
        if ok {
            Ok(body.get("result").cloned().unwrap_or(Value::Null))
        } else {
            Err(body["message"]
                .as_str()
                .unwrap_or("gxserver refused the call.")
                .to_string())
        }
    }
}

/// Where gxserver is, for the connector's replayed guest reads.
#[derive(Clone, Debug, Default)]
pub struct HostAddress {
    pub base_url: String,
    pub gxserver_protocol_version: u32,
}

fn slot() -> &'static RwLock<Option<Arc<dyn Host>>> {
    static HOST: OnceLock<RwLock<Option<Arc<dyn Host>>>> = OnceLock::new();
    HOST.get_or_init(|| RwLock::new(None))
}

fn address_slot() -> &'static RwLock<HostAddress> {
    static ADDRESS: OnceLock<RwLock<HostAddress>> = OnceLock::new();
    ADDRESS.get_or_init(|| RwLock::new(HostAddress::default()))
}

pub fn set(host: Arc<dyn Host>) {
    *slot().write().unwrap_or_else(PoisonError::into_inner) = Some(host);
}

pub fn set_address(address: HostAddress) {
    *address_slot()
        .write()
        .unwrap_or_else(PoisonError::into_inner) = address;
}

pub fn address() -> HostAddress {
    address_slot()
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// One call into gxserver (blocking; loopback calls are quick).
pub fn call(call: HostCall) -> Result<Value, String> {
    let host = slot()
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
        .ok_or_else(|| "Collab isn't connected to gxserver.".to_string())?;
    host.call(&call)
}

/// [`call`] off the async threads.
pub async fn call_async(host_call: HostCall) -> Result<Value, String> {
    tokio::task::spawn_blocking(move || call(host_call))
        .await
        .unwrap_or_else(|_| Err("The call into gxserver stopped.".to_string()))
}

/// What gxserver knows about a chat.
#[derive(Clone, Debug, Default)]
pub struct ChatInfo {
    /// `None` when gxserver couldn't tell.
    pub exists: Option<bool>,
    pub title: Option<String>,
    pub project_name: String,
}

fn info_cache() -> &'static Mutex<std::collections::HashMap<(String, String), (Instant, ChatInfo)>>
{
    static CACHE: OnceLock<
        Mutex<std::collections::HashMap<(String, String), (Instant, ChatInfo)>>,
    > = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// A chat's title and whether it still exists, asked again at most every few seconds.
pub fn chat_info(project_id: &str, session_id: &str) -> ChatInfo {
    let key = (project_id.to_string(), session_id.to_string());
    if let Some((at, info)) = info_cache()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&key)
    {
        if at.elapsed() < Duration::from_secs(5) {
            return info.clone();
        }
    }
    let info = match call(HostCall::ChatInfo {
        project_id: project_id.to_string(),
        session_id: session_id.to_string(),
    }) {
        Ok(value) => ChatInfo {
            exists: value["exists"].as_bool(),
            title: value["title"]
                .as_str()
                .map(str::trim)
                .filter(|title| !title.is_empty())
                .map(str::to_string),
            project_name: value["projectName"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
        },
        Err(_) => ChatInfo::default(),
    };
    info_cache()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(key, (Instant::now(), info.clone()));
    info
}

/// A chat's title for its share, also what the guest's join page names.
pub fn share_title(share: &crate::shares::StoredShare) -> String {
    chat_info(&share.project_id, &share.session_id)
        .title
        .unwrap_or_else(|| "A chat".to_string())
}

/// A host for tests: answers from a function and records every call.
#[cfg(test)]
pub mod fake {
    use super::*;

    type Answer = dyn Fn(&HostCall) -> Result<Value, String> + Send + Sync;

    pub struct FakeHost {
        pub calls: Mutex<Vec<HostCall>>,
        answer: Box<Answer>,
    }

    impl FakeHost {
        pub fn install(
            answer: impl Fn(&HostCall) -> Result<Value, String> + Send + Sync + 'static,
        ) -> Arc<FakeHost> {
            let host = Arc::new(FakeHost {
                calls: Mutex::new(Vec::new()),
                answer: Box::new(answer),
            });
            info_cache()
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clear();
            set(host.clone());
            host
        }

        pub fn calls(&self) -> Vec<HostCall> {
            self.calls
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }
    }

    impl Host for FakeHost {
        fn call(&self, call: &HostCall) -> Result<Value, String> {
            self.calls
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(call.clone());
            (self.answer)(call)
        }
    }

    /// Every chat exists and is called "Fix login"; everything else succeeds.
    pub fn friendly(call: &HostCall) -> Result<Value, String> {
        match call {
            HostCall::ChatInfo { .. } => {
                Ok(json!({"exists": true, "title": "Fix login", "projectName": "Shop"}))
            }
            HostCall::SaveGuestImage { .. } => Ok(json!({"path": "/tmp/guest.png"})),
            HostCall::GuestPageAnswersMessage { .. } => Ok(json!({"message": "Answers: yes"})),
            _ => Ok(json!({})),
        }
    }
}
