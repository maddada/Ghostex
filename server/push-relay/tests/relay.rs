//! The relay end to end against a stand-in for Apple's service on loopback (HTTP/2 without TLS), with a
//! throwaway key made for each test. Run with `--nocapture` to see each request the stand-in received.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::Router;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use ghostex_push_relay::apns::{Apple, Credentials, Target};
use ghostex_push_relay::limits::LimitConfig;
use ghostex_push_relay::{LogSink, Options, Relay, TOPIC};
use p256::ecdsa::signature::Verifier;
use p256::ecdsa::{Signature, SigningKey, VerifyingKey};
use p256::pkcs8::{EncodePrivateKey, LineEnding};
use serde_json::{json, Value};

const KEY_ID: &str = "KEYID12345";
const TEAM_ID: &str = "TEAMID6789";
/// Text that appears only inside content states, to find in the log if it ever leaks.
const SECRET_TEXT: &str = "secret-project-name";

#[derive(Clone, Debug)]
struct Received {
    environment: String,
    token: String,
    headers: HeaderMap,
    payload: Value,
}

#[derive(Default)]
struct StandIn {
    received: Mutex<Vec<Received>>,
    /// Token → Apple's answer other than 200.
    answers: Mutex<HashMap<String, (u16, &'static str)>>,
}

async fn apple(
    State(stand_in): State<Arc<StandIn>>,
    Path((environment, token)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let payload = serde_json::from_slice(&body).unwrap_or(Value::Null);
    stand_in.received.lock().unwrap().push(Received {
        environment,
        token: token.clone(),
        headers,
        payload,
    });
    match stand_in.answers.lock().unwrap().get(&token) {
        Some((status, reason)) => (
            StatusCode::from_u16(*status).unwrap(),
            [("apns-id", "stand-in")],
            json!({ "reason": reason }).to_string(),
        ),
        None => (
            StatusCode::OK,
            [("apns-id", "8F1D5C2A-0000-0000-0000-000000000001")],
            String::new(),
        ),
    }
}

struct Harness {
    relay_url: String,
    stand_in: Arc<StandIn>,
    logs: Arc<Mutex<Vec<String>>>,
    public_key: VerifyingKey,
    key_pem: String,
    http: reqwest::Client,
}

fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ghostex-push-relay-test-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

async fn serve(router: Router) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    address
}

async fn harness(configured: bool, limits: LimitConfig) -> Harness {
    let stand_in = Arc::new(StandIn::default());
    let apple_address = serve(
        Router::new()
            .route("/{environment}/3/device/{token}", post(apple))
            .with_state(stand_in.clone()),
    )
    .await;

    // A throwaway key, written as Apple's .p8 is, and read back through the relay's own loader.
    let key = SigningKey::random(&mut rand_core::OsRng);
    let key_pem = key.to_pkcs8_pem(LineEnding::LF).unwrap().to_string();
    let dir = temp_dir();
    let key_path = dir.join("AuthKey.p8");
    let config_path = dir.join("apns.conf");
    std::fs::write(&key_path, &key_pem).unwrap();
    std::fs::write(
        &config_path,
        format!("# test\nKEY_ID={KEY_ID}\nTEAM_ID={TEAM_ID}\n"),
    )
    .unwrap();
    let apple_result = if configured {
        let credentials =
            Credentials::load(&key_path, &config_path).expect("the throwaway key loads");
        Apple::new(
            credentials,
            Target::loopback(&format!("http://{apple_address}")).unwrap(),
        )
    } else {
        Err("no key".to_string())
    };

    let logs = Arc::new(Mutex::new(Vec::new()));
    let sink_logs = logs.clone();
    let log: LogSink = Arc::new(move |line: &str| sink_logs.lock().unwrap().push(line.to_string()));
    let relay = Relay::new(Options {
        behind_proxy: false,
        apple: apple_result,
        limits,
        log,
    });
    let relay_address = serve(relay.router()).await;
    Harness {
        relay_url: format!("http://{relay_address}/live-activity"),
        stand_in,
        logs,
        public_key: VerifyingKey::from(&key),
        key_pem,
        http: reqwest::Client::new(),
    }
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn token(seed: u8) -> String {
    hex::encode([seed; 40])
}

fn content() -> Value {
    json!({ "updatedAt": "2026-11-11T08:14:00.000Z", "counts": { "working": 1, "waiting": 0 }, "moreCount": 0,
        "agents": [{ "target": { "machineId": "mini", "sessionId": "S1" }, "agentName": "Claude", "agentIcon": "claude",
            "agentTint": 14251863, "project": SECRET_TEXT, "state": "working", "startedAt": "2026-11-11T08:13:00.000Z" }] })
}

impl Harness {
    async fn send(&self, body: Value) -> (u16, Value) {
        let response = self
            .http
            .post(format!("{}/v1/send", self.relay_url))
            .json(&body)
            .send()
            .await
            .unwrap();
        let status = response.status().as_u16();
        (status, response.json().await.unwrap_or(Value::Null))
    }

    fn received(&self) -> Vec<Received> {
        self.stand_in.received.lock().unwrap().clone()
    }

    /// The log holds no token, no content and nothing from the key's files.
    fn assert_log_is_clean(&self, tokens: &[String]) {
        let logs = self.logs.lock().unwrap();
        let key_body: String = self
            .key_pem
            .lines()
            .filter(|line| !line.starts_with("-----"))
            .collect();
        for line in logs.iter() {
            for token in tokens {
                assert!(
                    !line.contains(token.as_str()),
                    "a token reached the log: {line}"
                );
                assert!(
                    !line.contains(&token[..16]),
                    "part of a token reached the log: {line}"
                );
            }
            assert!(
                !line.contains(SECRET_TEXT),
                "content reached the log: {line}"
            );
            assert!(
                !line.contains(TEAM_ID),
                "the team id reached the log: {line}"
            );
            assert!(
                !line.contains("PRIVATE KEY"),
                "the key reached the log: {line}"
            );
            for chunk in key_body.as_bytes().chunks(12) {
                let chunk = std::str::from_utf8(chunk).unwrap();
                assert!(
                    !line.contains(chunk),
                    "part of the key reached the log: {line}"
                );
            }
        }
    }
}

/// Checks the provider token in `authorization` against the throwaway key.
fn verify_provider_token(headers: &HeaderMap, public_key: &VerifyingKey) -> (Value, Value) {
    let authorization = headers.get("authorization").unwrap().to_str().unwrap();
    let jwt = authorization.strip_prefix("bearer ").expect("bearer token");
    let parts: Vec<&str> = jwt.split('.').collect();
    let signature = Signature::from_slice(&URL_SAFE_NO_PAD.decode(parts[2]).unwrap()).unwrap();
    public_key
        .verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &signature)
        .expect("the signature verifies");
    (
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[0]).unwrap()).unwrap(),
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap(),
    )
}

fn header<'a>(received: &'a Received, name: &str) -> &'a str {
    received.headers.get(name).unwrap().to_str().unwrap()
}

#[tokio::test]
async fn update_end_and_start_reach_apple_in_its_shape() {
    let h = harness(true, LimitConfig::default()).await;
    let t = now();
    let (status, body) = h.send(json!({ "token": token(1), "event": "update", "timestamp": t, "contentState": content(), "staleDate": t + 900, "priority": 5 })).await;
    assert_eq!((status, body["ok"].clone()), (200, json!(true)), "{body}");
    let (status, _) = h.send(json!({ "token": token(1), "event": "end", "timestamp": t + 1, "contentState": content(), "dismissalDate": t + 600 })).await;
    assert_eq!(status, 200);
    let (status, _) = h.send(json!({ "token": token(2), "event": "start", "timestamp": t, "contentState": content(), "environment": "sandbox" })).await;
    assert_eq!(status, 200);

    let received = h.received();
    assert_eq!(received.len(), 3);
    for push in &received {
        eprintln!(
            "stand-in received: POST /{}/3/device/{}…\n  apns-push-type: {}\n  apns-topic: {}\n  apns-priority: {}\n  apns-expiration: {}\n  payload: {}",
            push.environment, &push.token[..8], header(push, "apns-push-type"), header(push, "apns-topic"),
            header(push, "apns-priority"), header(push, "apns-expiration"), push.payload
        );
        assert_eq!(header(push, "apns-push-type"), "liveactivity");
        assert_eq!(header(push, "apns-topic"), TOPIC);
        let (jwt_header, claims) = verify_provider_token(&push.headers, &h.public_key);
        assert_eq!(jwt_header, json!({ "alg": "ES256", "kid": KEY_ID }));
        assert_eq!(claims["iss"], TEAM_ID);
        assert!((claims["iat"].as_i64().unwrap() - t).abs() < 60);
    }
    eprintln!(
        "provider token: header {} claims {} (signature verified against the throwaway key)",
        verify_provider_token(&received[0].headers, &h.public_key).0,
        verify_provider_token(&received[0].headers, &h.public_key).1
    );

    let update = &received[0];
    assert_eq!(
        (update.environment.as_str(), update.token.as_str()),
        ("production", token(1).as_str())
    );
    assert_eq!(header(update, "apns-priority"), "5");
    assert_eq!(
        update.payload,
        json!({ "aps": { "timestamp": t, "event": "update", "content-state": content(), "stale-date": t + 900 } })
    );

    let end = &received[1];
    assert_eq!(header(end, "apns-priority"), "10");
    assert_eq!(
        end.payload,
        json!({ "aps": { "timestamp": t + 1, "event": "end", "content-state": content(), "dismissal-date": t + 600 } })
    );

    let start = &received[2];
    assert_eq!(start.environment, "sandbox");
    assert_eq!(
        start.payload,
        json!({ "aps": { "timestamp": t, "event": "start", "content-state": content(),
        "attributes-type": "GhostexActivityAttributes", "attributes": {},
        "alert": { "title": "Ghostex", "body": "Your agents are working." } } })
    );

    // One provider token serves every push (Apple refuses one renewed more often than every 20 minutes).
    let first = header(&received[0], "authorization");
    assert!(received
        .iter()
        .all(|push| header(push, "authorization") == first));
    h.assert_log_is_clean(&[token(1), token(2)]);
    eprintln!("relay log:\n  {}", h.logs.lock().unwrap().join("\n  "));
}

#[tokio::test]
async fn a_caller_cannot_choose_the_topic_type_or_host() {
    let h = harness(true, LimitConfig::default()).await;
    let t = now();
    for (field, value) in [
        ("topic", json!("com.other.app.push-type.liveactivity")),
        ("pushType", json!("alert")),
        ("host", json!("api.example.com")),
        ("alert", json!({ "title": "Pay now" })),
        ("environment", json!("https://evil.example")),
    ] {
        let mut body = json!({ "token": token(3), "event": "update", "timestamp": t, "contentState": content() });
        body[field] = value;
        let (status, answer) = h.send(body).await;
        eprintln!("{field}: {status} {answer}");
        assert_eq!(
            (status, answer["error"].as_str()),
            (400, Some("bad-request")),
            "{field}"
        );
    }
    // A header naming another topic is not read at all.
    let response = h.http.post(format!("{}/v1/send", h.relay_url)).header("apns-topic", "com.other.app")
        .json(&json!({ "token": token(3), "event": "update", "timestamp": t, "contentState": content() })).send().await.unwrap();
    assert_eq!(response.status().as_u16(), 200);
    let received = h.received();
    assert_eq!(received.len(), 1);
    assert_eq!(header(&received[0], "apns-topic"), TOPIC);
    h.assert_log_is_clean(&[token(3)]);
}

#[tokio::test]
async fn oversize_pushes_are_refused() {
    let h = harness(true, LimitConfig::default()).await;
    let t = now();
    let (status, answer) = h.send(json!({ "token": token(4), "event": "update", "timestamp": t, "contentState": { "filler": "x".repeat(4200) } })).await;
    eprintln!("over Apple's 4 KB: {status} {answer}");
    assert_eq!((status, answer["error"].as_str()), (413, Some("too-large")));
    let response = h
        .http
        .post(format!("{}/v1/send", h.relay_url))
        .header("content-type", "application/json")
        .body(vec![b' '; 9 * 1024])
        .send()
        .await
        .unwrap();
    eprintln!("over the 8 KB body limit: {}", response.status());
    assert_eq!(response.status().as_u16(), 413);
    assert!(h.received().is_empty());
}

#[tokio::test]
async fn rate_limits_hold() {
    let h = harness(true, LimitConfig::default()).await;
    let t = now();
    let update = |seed| json!({ "token": token(seed), "event": "update", "timestamp": t, "contentState": content() });
    for _ in 0..10 {
        assert_eq!(h.send(update(5)).await.0, 200);
    }
    let (status, answer) = h.send(update(5)).await;
    eprintln!("11th update to one token within a minute: {status} {answer}");
    assert_eq!(
        (status, answer["error"].as_str()),
        (429, Some("rate-limited"))
    );
    // Another token is not held back by the first.
    assert_eq!(h.send(update(6)).await.0, 200);
    // Starts have their own, tighter bucket: 2, then one every 10 minutes.
    let start =
        json!({ "token": token(7), "event": "start", "timestamp": t, "contentState": content() });
    assert_eq!(h.send(start.clone()).await.0, 200);
    assert_eq!(h.send(start.clone()).await.0, 200);
    let (status, answer) = h.send(start).await;
    eprintln!("3rd start to one token: {status} {answer}");
    assert_eq!(status, 429);
    assert_eq!(h.received().len(), 13);

    // Per address: a small limit so the test stays short; the same code holds the real 60 a minute.
    let mut limits = LimitConfig::default();
    limits.per_address.burst = 3.0;
    let h = harness(true, limits).await;
    for seed in 10..13 {
        assert_eq!(h.send(update(seed)).await.0, 200);
    }
    let (status, _) = h.send(update(13)).await;
    eprintln!("4th request from one address with a burst of 3: {status}");
    assert_eq!(status, 429);
    h.assert_log_is_clean(&[token(10), token(11), token(12), token(13)]);
}

#[tokio::test]
async fn without_a_key_it_answers_health_and_refuses_pushes() {
    let h = harness(false, LimitConfig::default()).await;
    let health: Value = h
        .http
        .get(format!("{}/healthz", h.relay_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    eprintln!("healthz: {health}");
    assert_eq!(
        (health["ok"].clone(), health["configured"].clone()),
        (json!(true), json!(false))
    );
    let (status, answer) = h.send(json!({ "token": token(8), "event": "update", "timestamp": now(), "contentState": content() })).await;
    eprintln!("push without a key: {status} {answer}");
    assert_eq!(
        (status, answer["error"].as_str()),
        (503, Some("not-configured"))
    );
    assert!(h.received().is_empty());
}

#[tokio::test]
async fn a_token_apple_dropped_is_not_sent_again() {
    let h = harness(true, LimitConfig::default()).await;
    h.stand_in
        .answers
        .lock()
        .unwrap()
        .insert(token(9), (410, "Unregistered"));
    let body = json!({ "token": token(9), "event": "update", "timestamp": now(), "contentState": content() });
    let (status, answer) = h.send(body.clone()).await;
    eprintln!("Apple says Unregistered: {status} {answer}");
    assert_eq!(
        (status, answer["error"].as_str()),
        (410, Some("token-gone"))
    );
    let (status, _) = h.send(body).await;
    assert_eq!(status, 410);
    assert_eq!(h.received().len(), 1, "the second push reached Apple");
    h.assert_log_is_clean(&[token(9)]);
}
