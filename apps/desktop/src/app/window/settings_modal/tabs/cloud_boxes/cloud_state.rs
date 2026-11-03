//! What the Cloud Boxes part of the page (Instabox, Boat, and the debug-only Local provider)
//! reads from gxserver's `/api/cloudBoxes`, and the reads themselves. The shapes are
//! `ghostex_gx_protocol::cloud_box`; a read that fails keeps the last answer and shows the error.
use super::super::super::store::store_gxserver_rpc;
use super::CloudBoxesTab;
use ghostex_gx_protocol::cloud_box::{
    AgentLoginsResult, CloudAgentLogin, CloudAgentLoginState, CloudBox, CloudBoxSize,
    CloudProvider, CloudSpend, ListResult, ProvidersResult, SizesResult,
};
use gpui::{App, Context, Entity, Task};
use gpui_component::input::InputState;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

pub(super) const CLOUD_BOXES_ROUTE: &str = ghostex_gx_protocol::cloud_box::CLOUD_BOXES_ROUTE;
const READ_TIMEOUT: Duration = Duration::from_secs(30);
/// While a sign-in waits for the browser or a box is on its way, the page re-reads this often.
const POLL_INTERVAL: Duration = Duration::from_secs(3);

/// Which Connect form is open under a provider row.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum ConnectForm {
    /// Paste an API key.
    Key,
    /// Boat's browser sign-in asks for the account's email first.
    Email,
    /// A browser sign-in is waiting for the user.
    Waiting {
        connect_id: String,
        auth_url: Option<String>,
        user_code: Option<String>,
    },
}

#[derive(Default)]
pub(super) struct CloudState {
    pub(super) providers: Option<Vec<CloudProvider>>,
    pub(super) providers_error: Option<String>,
    pub(super) spend: Option<CloudSpend>,
    pub(super) spend_error: Option<String>,
    pub(super) boxes: Option<Vec<CloudBox>>,
    pub(super) boxes_error: Option<String>,
    pub(super) logins: Option<Vec<CloudAgentLogin>>,
    pub(super) logins_error: Option<String>,
    pub(super) sizes: Vec<CloudBoxSize>,
    /// Reads in flight, by action.
    pub(super) loading: HashSet<&'static str>,
    /// Provider ids, box ids and account ids with an action in flight.
    pub(super) busy: HashSet<String>,
    /// The provider whose Connect form is open, and the form.
    pub(super) connect: Option<(String, ConnectForm)>,
    pub(super) connect_error: Option<String>,
    /// The masked key box and the email box of the open form (made on first render, which has
    /// the window).
    pub(super) key_input: Option<Entity<InputState>>,
    pub(super) email_input: Option<Entity<InputState>>,
    /// The box whose Delete waits for its confirmation.
    pub(super) confirm_delete: Option<String>,
    /// Boxes whose delete was refused (unpushed work): the reason, and Delete Anyway is offered.
    pub(super) needs_force: HashMap<String, String>,
    poll: Option<Task<()>>,
}

impl CloudState {
    /// Something on the page is still moving: a browser sign-in, a Claude box token being made, or
    /// a box starting, stopping or deleting.
    fn wants_poll(&self) -> bool {
        matches!(self.connect, Some((_, ConnectForm::Waiting { .. })))
            || self.logins.as_ref().is_some_and(|logins| {
                logins
                    .iter()
                    .any(|login| login.state == CloudAgentLoginState::Waiting)
            })
            || self.boxes.as_ref().is_some_and(|boxes| {
                boxes.iter().any(|row| {
                    row.state.is_starting()
                        || matches!(row.state.as_str(), "deleting" | "creating" | "booting")
                })
            })
    }

    pub(super) fn provider(&self, id: &str) -> Option<&CloudProvider> {
        self.providers
            .as_ref()?
            .iter()
            .find(|provider| provider.id.as_str() == id)
    }
}

fn parse<T: DeserializeOwned>(result: Result<Value, String>) -> Result<T, String> {
    let value = result.map_err(|error| {
        if error.trim().is_empty() {
            "gxserver did not answer.".to_string()
        } else {
            error
        }
    })?;
    serde_json::from_value(value).map_err(|error| format!("Unexpected answer: {error}"))
}

impl CloudBoxesTab {
    /// One `/api/cloudBoxes` call; `reply` runs on the page.
    pub(super) fn cloud_rpc(
        &self,
        action: &str,
        params: Value,
        timeout: Duration,
        reply: impl FnOnce(&mut Self, Result<Value, String>, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let mut body = match params {
            Value::Object(map) => map,
            _ => Map::new(),
        };
        body.insert("action".into(), json!(action));
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            CLOUD_BOXES_ROUTE,
            Value::Object(body),
            timeout,
            move |result, cx: &mut App| {
                let _ = this.update(cx, |page, cx| {
                    reply(page, result, cx);
                    page.cloud_watch(cx);
                    cx.notify();
                });
            },
            cx,
        );
    }

    fn cloud_read<T: DeserializeOwned + 'static>(
        &mut self,
        action: &'static str,
        params: Value,
        apply: impl FnOnce(&mut CloudState, Result<T, String>) + 'static,
        cx: &mut Context<Self>,
    ) {
        if !self.rpc_available(cx) || !self.cloud.loading.insert(action) {
            return;
        }
        self.cloud_rpc(
            action,
            params,
            READ_TIMEOUT,
            move |page, result, _cx| {
                page.cloud.loading.remove(action);
                apply(&mut page.cloud, parse(result));
            },
            cx,
        );
    }

    pub(super) fn load_cloud_providers(&mut self, cx: &mut Context<Self>) {
        self.cloud_read::<ProvidersResult>(
            "providers",
            json!({}),
            |cloud, result| match result {
                Ok(answer) => {
                    cloud.providers = Some(answer.providers);
                    cloud.providers_error = None;
                }
                Err(error) => cloud.providers_error = Some(error),
            },
            cx,
        );
        self.cloud_read::<SizesResult>(
            "sizes",
            json!({}),
            |cloud, result| {
                if let Ok(answer) = result {
                    cloud.sizes = answer.sizes;
                }
            },
            cx,
        );
    }

    pub(super) fn load_cloud_spend(&mut self, refresh: bool, cx: &mut Context<Self>) {
        self.cloud_read::<CloudSpend>(
            "spend",
            json!({ "refresh": refresh }),
            |cloud, result| match result {
                Ok(spend) => {
                    cloud.spend = Some(spend);
                    cloud.spend_error = None;
                }
                Err(error) => cloud.spend_error = Some(error),
            },
            cx,
        );
    }

    pub(super) fn load_cloud_boxes(&mut self, refresh: bool, cx: &mut Context<Self>) {
        self.cloud_read::<ListResult>(
            "list",
            json!({ "refresh": refresh }),
            |cloud, result| match result {
                Ok(answer) => {
                    cloud.boxes = Some(answer.boxes);
                    cloud.boxes_error = None;
                }
                Err(error) => cloud.boxes_error = Some(error),
            },
            cx,
        );
    }

    pub(super) fn load_cloud_logins(&mut self, cx: &mut Context<Self>) {
        self.cloud_read::<AgentLoginsResult>(
            "agentLogins",
            json!({}),
            |cloud, result| match result {
                Ok(answer) => {
                    cloud.logins = Some(answer.logins);
                    cloud.logins_error = None;
                }
                Err(error) => cloud.logins_error = Some(error),
            },
            cx,
        );
    }

    pub(super) fn load_cloud_all(&mut self, cx: &mut Context<Self>) {
        self.load_cloud_providers(cx);
        self.load_cloud_spend(false, cx);
        self.load_cloud_boxes(false, cx);
        self.load_cloud_logins(cx);
    }

    /// Re-reads what is moving every `POLL_INTERVAL` until nothing is.
    pub(super) fn cloud_watch(&mut self, cx: &mut Context<Self>) {
        if self.cloud.poll.is_some() || !self.cloud.wants_poll() {
            return;
        }
        self.cloud.poll = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL_INTERVAL).await;
                let keep_going = this
                    .update(cx, |page, cx| {
                        if !page.cloud.wants_poll() {
                            page.cloud.poll = None;
                            return false;
                        }
                        page.poll_connect(cx);
                        page.load_cloud_boxes(false, cx);
                        page.load_cloud_logins(cx);
                        true
                    })
                    .unwrap_or(false);
                if !keep_going {
                    break;
                }
            }
        }));
    }
}
