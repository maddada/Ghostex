//! A fake Expo push service on a loopback port, for tests of the real sender.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{body::Bytes, extract::State, http::Uri, response::IntoResponse, Router};
use serde_json::{json, Value};

use super::runtime::PushClock;

pub(crate) const SEND_PATH: &str = "/--/api/v2/push/send";
pub(crate) const RECEIPTS_PATH: &str = "/--/api/v2/push/getReceipts";

#[derive(Clone, Debug)]
pub(crate) struct Captured {
    pub(crate) path: String,
    pub(crate) body: Value,
    /// The sender's clock when the request arrived.
    pub(crate) at: i64,
}

pub(crate) struct Reply {
    pub(crate) status: u16,
    pub(crate) headers: Vec<(&'static str, String)>,
    pub(crate) body: String,
    pub(crate) delay: Duration,
}

impl Reply {
    pub(crate) fn new(status: u16, body: impl Into<String>) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: body.into(),
            delay: Duration::ZERO,
        }
    }
}

/// Answers scripted replies first, then accepts every message with a fresh ticket id and reports
/// no receipts.
pub(crate) struct FakeExpo {
    pub(crate) origin: String,
    state: Arc<Mutex<FakeExpoState>>,
}

struct FakeExpoState {
    requests: Vec<Captured>,
    replies: VecDeque<Reply>,
    tickets: usize,
    clock: Arc<dyn PushClock>,
}

impl FakeExpo {
    pub(crate) async fn start(clock: Arc<dyn PushClock>) -> Self {
        let state = Arc::new(Mutex::new(FakeExpoState {
            requests: Vec::new(),
            replies: VecDeque::new(),
            tickets: 0,
            clock,
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fake expo");
        let origin = format!("http://{}", listener.local_addr().expect("address"));
        let app = Router::new().fallback(handle).with_state(state.clone());
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Self { origin, state }
    }

    pub(crate) fn reply(&self, reply: Reply) {
        self.state.lock().unwrap().replies.push_back(reply);
    }

    pub(crate) fn requests(&self) -> Vec<Captured> {
        self.state.lock().unwrap().requests.clone()
    }

    pub(crate) fn sends(&self) -> Vec<Captured> {
        self.requests()
            .into_iter()
            .filter(|request| request.path == SEND_PATH)
            .collect()
    }
}

async fn handle(
    State(fake): State<Arc<Mutex<FakeExpoState>>>,
    uri: Uri,
    body: Bytes,
) -> impl IntoResponse {
    let reply = {
        let mut fake = fake.lock().unwrap();
        let at = fake.clock.now_ms();
        fake.requests.push(Captured {
            path: uri.path().to_string(),
            body: serde_json::from_slice(&body).unwrap_or(Value::Null),
            at,
        });
        match fake.replies.pop_front() {
            Some(reply) => reply,
            None if uri.path() == SEND_PATH => {
                fake.tickets += 1;
                Reply::new(
                    200,
                    json!({ "data": [{ "status": "ok", "id": format!("ticket-{}", fake.tickets) }] })
                        .to_string(),
                )
            }
            None => Reply::new(200, json!({ "data": {} }).to_string()),
        }
    };
    if !reply.delay.is_zero() {
        tokio::time::sleep(reply.delay).await;
    }
    let mut response = axum::http::Response::builder()
        .status(reply.status)
        .header("content-type", "application/json");
    for (name, value) in reply.headers {
        response = response.header(name, value);
    }
    response.body(axum::body::Body::from(reply.body)).unwrap()
}
