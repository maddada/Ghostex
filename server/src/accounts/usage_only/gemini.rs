//! Gemini: a Google sign-in in the browser that comes back to a local page, and the Gemini quota of
//! the account's Google AI plan (its 5h and 7d windows) from Google's Cloud Code service.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::http::{self, agent, answer, url_encode, Failure};
use super::signin::{Progress, SignedIn};
use super::store::{Credential, UsageAccount};
use super::Reading;
use crate::accounts::model::UsageWindow;

const NAME: &str = "Gemini";
/// The Google "installed application" client the Antigravity app signs in with; its quota reads
/// answer only for it. Google treats an installed app's secret as public (every copy ships it).
const CLIENT_ID: &str = "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com";
const CLIENT_SECRET: &str = "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";
const SCOPES: &str =
    "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email";
/// The callback port the client was registered with.
const PORT: u16 = 51121;
const CALLBACK: &str = "/oauth-callback";
const CLOUD_CODE: [&str; 2] = [
    "https://cloudcode-pa.googleapis.com",
    "https://daily-cloudcode-pa.googleapis.com",
];
/// Google marks other callers as an unsupported client and leaves the plan out.
const CLIENT_AGENT: &str = if cfg!(target_arch = "aarch64") {
    "antigravity/2.24.0 (aidev_client; os_type=darwin; arch=arm64)"
} else {
    "antigravity/2.24.0 (aidev_client; os_type=darwin; arch=amd64)"
};

pub(crate) fn sign_in(progress: &Progress) -> Result<SignedIn, Failure> {
    let (verifier, challenge) = http::pkce();
    let state = http::random_state();
    let redirect = format!("http://localhost:{PORT}{CALLBACK}");
    let listener = TcpListener::bind(("127.0.0.1", PORT)).map_err(|_| {
        Failure::Unavailable(
            "Another sign-in is using the Google sign-in port. Close it and try again.".into(),
        )
    })?;
    listener
        .set_nonblocking(true)
        .map_err(|error| Failure::Unavailable(error.to_string()))?;
    progress.show_url(
        &format!(
            "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}&state={state}&code_challenge={challenge}&code_challenge_method=S256&access_type=offline&prompt=consent",
            url_encode(CLIENT_ID),
            url_encode(&redirect),
            url_encode(SCOPES),
        ),
        true,
    );
    let deadline = Instant::now() + Duration::from_secs(600);
    let code = loop {
        if progress.wait(Duration::from_millis(250)) {
            return Err(Failure::Unavailable("Sign-in cancelled.".into()));
        }
        if Instant::now() > deadline {
            return Err(Failure::Unavailable(
                "The sign-in took too long. Try again.".into(),
            ));
        }
        if let Some(pasted) = progress.take_input() {
            if let Some(code) = code_from(&pasted, &state) {
                break code;
            }
            progress.note("That doesn't look like the code or the address the browser showed.");
            continue;
        }
        match listener.accept() {
            Ok((stream, _)) => {
                if let Some(code) = callback(stream, &state) {
                    break code;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(Failure::Unavailable(error.to_string())),
        }
    };
    drop(listener);
    progress.saving();
    let token = answer(
        agent()
            .post("https://oauth2.googleapis.com/token")
            .send_form([
                ("client_id", CLIENT_ID),
                ("client_secret", CLIENT_SECRET),
                ("code", code.as_str()),
                ("grant_type", "authorization_code"),
                ("redirect_uri", redirect.as_str()),
                ("code_verifier", verifier.as_str()),
            ]),
        NAME,
    )?;
    let access = token.body["access_token"]
        .as_str()
        .ok_or_else(|| Failure::Unavailable("Google didn't finish the sign-in. Try again.".into()))?
        .to_string();
    let user = answer(
        agent()
            .get("https://www.googleapis.com/oauth2/v1/userinfo?alt=json")
            .header("Authorization", &format!("Bearer {access}"))
            .call(),
        NAME,
    )?;
    let email = user.body["email"].as_str().unwrap_or_default().to_string();
    let id = user.body["id"]
        .as_str()
        .map(str::to_string)
        .or_else(|| (!email.is_empty()).then(|| email.clone()))
        .ok_or_else(|| user.unavailable("Google"))?;
    Ok(SignedIn {
        identity: format!("gemini:{id}"),
        name: if email.is_empty() {
            "Gemini".into()
        } else {
            email.clone()
        },
        email,
        credential: Credential {
            access_token: access,
            refresh_token: token.body["refresh_token"].as_str().map(str::to_string),
            expires_at: token.body["expires_in"]
                .as_i64()
                .map(|seconds| http::now_ms() + seconds * 1000),
            extra: Default::default(),
        },
    })
}

/// The code from what the user pasted: the address the browser ended on, or the code alone.
fn code_from(pasted: &str, state: &str) -> Option<String> {
    let pasted = pasted.trim();
    if pasted.is_empty() {
        return None;
    }
    if let Ok(url) = url::Url::parse(pasted) {
        let pairs: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        if pairs.get("state").is_some_and(|got| got != state) {
            return None;
        }
        return pairs.get("code").cloned();
    }
    (!pasted.contains(char::is_whitespace)).then(|| pasted.to_string())
}

/// Answers the browser's return to the local page; the code when it carries this sign-in's state.
fn callback(mut stream: TcpStream, state: &str) -> Option<String> {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line).ok()?;
    let target = line.split_whitespace().nth(1)?;
    let url = url::Url::parse(&format!("http://localhost{target}")).ok()?;
    let code = (url.path() == CALLBACK)
        .then(|| {
            let pairs: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
            (pairs.get("state").map(String::as_str) == Some(state))
                .then(|| pairs.get("code").cloned())
                .flatten()
        })
        .flatten();
    let page = if code.is_some() {
        "Signed in. You can close this tab and go back to Ghostex."
    } else {
        "This sign-in didn't finish. Go back to Ghostex and try again."
    };
    let body = format!("<!doctype html><meta charset=utf-8><title>Ghostex</title><p style=\"font:16px system-ui;margin:3em\">{page}</p>");
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    code
}

fn renew(credential: &Credential) -> Result<Credential, Failure> {
    let refresh = credential
        .refresh_token
        .as_deref()
        .ok_or(Failure::SignInExpired)?;
    let reply = answer(
        agent()
            .post("https://oauth2.googleapis.com/token")
            .send_form([
                ("client_id", CLIENT_ID),
                ("client_secret", CLIENT_SECRET),
                ("refresh_token", refresh),
                ("grant_type", "refresh_token"),
            ]),
        NAME,
    )?;
    if reply.body["error"].as_str() == Some("invalid_grant") || reply.refused() {
        return Err(Failure::SignInExpired);
    }
    let access = reply.body["access_token"]
        .as_str()
        .ok_or_else(|| reply.unavailable(NAME))?;
    let mut renewed = credential.clone();
    renewed.access_token = access.to_string();
    renewed.expires_at =
        Some(http::now_ms() + reply.body["expires_in"].as_i64().unwrap_or(3600) * 1000);
    Ok(renewed)
}

fn cloud_code(
    base: &str,
    method: &str,
    token: &str,
    body: &Value,
) -> Result<http::Answer, Failure> {
    answer(
        agent()
            .post(&format!("{base}/v1internal:{method}"))
            .header("Authorization", &format!("Bearer {token}"))
            .header("Accept", "application/json")
            .header("User-Agent", CLIENT_AGENT)
            .send_json(body),
        NAME,
    )
}

pub(crate) fn read(account: &UsageAccount) -> Result<(Reading, Option<Credential>), Failure> {
    let mut credential = account.credential.clone();
    let mut renewed = false;
    if credential
        .expires_at
        .is_none_or(|at| at - http::now_ms() < 60_000)
    {
        credential = renew(&credential)?;
        renewed = true;
    }
    let token = credential.access_token.clone();
    let load = json!({"metadata":{"ideType":"ANTIGRAVITY","platform":"PLATFORM_UNSPECIFIED","pluginType":"GEMINI"}});
    let mut plan = None;
    let mut project = credential
        .extra
        .get("project")
        .and_then(Value::as_str)
        .map(str::to_string);
    for base in CLOUD_CODE {
        let reply = cloud_code(base, "loadCodeAssist", &token, &load)?;
        if reply.refused() {
            return Err(Failure::SignInExpired);
        }
        if reply.ok() {
            plan = plan_name(&reply.body);
            if let Some(found) = reply.body["cloudaicompanionProject"]
                .as_str()
                .filter(|p| !p.is_empty())
            {
                if project.as_deref() != Some(found) {
                    credential.extra.insert("project".into(), json!(found));
                    renewed = true;
                }
                project = Some(found.to_string());
            }
            break;
        }
    }
    let body = project
        .map(|project| json!({"project": project}))
        .unwrap_or_else(|| json!({}));
    let mut last = None;
    for base in CLOUD_CODE {
        let reply = cloud_code(base, "retrieveUserQuotaSummary", &token, &body)?;
        if reply.ok() {
            let reading = map(&reply.body, plan.clone());
            if reading
                .usage
                .iter()
                .filter(|window| window.model.is_none())
                .count()
                == 2
            {
                return Ok((reading, renewed.then_some(credential)));
            }
            last = Some(Ok(reading));
            continue;
        }
        if reply.status == 401 {
            return Err(Failure::SignInExpired);
        }
        if last.is_none() {
            last = Some(Err(if reply.status == 403 {
                Failure::Unavailable(
                    "This Google account has no Gemini plan with a quota Ghostex can read.".into(),
                )
            } else {
                reply.unavailable(NAME)
            }));
        }
    }
    match last {
        Some(Ok(reading)) if !reading.usage.is_empty() => {
            Ok((reading, renewed.then_some(credential)))
        }
        Some(Err(failure)) => Err(failure),
        _ => Err(Failure::Unavailable(
            "Google didn't report a Gemini quota for this account.".into(),
        )),
    }
}

fn plan_name(load: &Value) -> Option<String> {
    let paid = &load["paidTier"];
    let known = match paid["id"].as_str() {
        Some("g1-pro-tier") => Some("Google AI Pro"),
        Some("g1-ultra-tier") => Some("Google AI Ultra"),
        Some("g1-ultra-lite-tier") => Some("Google AI Ultra Lite"),
        _ => None,
    };
    known
        .map(str::to_string)
        .or_else(|| {
            paid["name"]
                .as_str()
                .or(load["currentTier"]["name"].as_str())
                .map(str::to_string)
        })
        .filter(|name| !name.trim().is_empty())
}

/// Every quota group of the answer, as agy's `/usage` lists them. The Gemini group's 5h and weekly
/// buckets are the account's own `5h` and `7d` windows; every other group (Claude and GPT models)
/// gets the same windows scoped to its name ("Claude/GPT 5h"), as Claude's Sonnet and Codex's
/// Spark limits are.
///
/// CDXC:AgentProviders 2026-11-08 WHY:
/// Gregg on Discord: "Gemini usage is not Antigravity usage it seems. Because those numbers from
/// the Gemini account do not match what I see when I type /usage in agy." Google's answer carries
/// a "Gemini Models" group and a "Claude and GPT models" group (`3p-5h`, `3p-weekly`), each with
/// its own 5h and weekly limit, and agy's `/usage` shows both. Ghostex read only the Gemini group,
/// so running Claude or GPT-OSS in agy used up a limit Ghostex never showed.
fn map(body: &Value, plan: Option<String>) -> Reading {
    let groups = body["response"]["groups"]
        .as_array()
        .or_else(|| body["groups"].as_array())
        .cloned()
        .unwrap_or_default();
    let mut main = Vec::new();
    let mut scoped = Vec::new();
    for group in &groups {
        let buckets = group["buckets"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let gemini = buckets.iter().any(|bucket| {
            bucket["bucketId"]
                .as_str()
                .is_some_and(|id| id.starts_with("gemini-"))
        });
        let model = (!gemini).then(|| group_name(group, buckets));
        let mut group_windows = Vec::new();
        for bucket in buckets {
            let Some((short, seconds)) = bucket_window(bucket) else {
                continue;
            };
            let Some(left) = http::number(&bucket["remainingFraction"]) else {
                continue;
            };
            let bucket_id = bucket["bucketId"].as_str().unwrap_or(short);
            let window = UsageWindow {
                id: match (&model, short) {
                    (None, "5h") => "fiveHour".into(),
                    (None, _) => "sevenDay".into(),
                    (Some(_), _) => bucket_id.into(),
                },
                label: match &model {
                    Some(model) => format!("{model} {short}"),
                    None => short.into(),
                },
                used_percent: http::clamp_percent(((1.0 - left.clamp(0.0, 1.0)) * 100.0).round()),
                limit_window_seconds: Some(seconds),
                resets_at: bucket["resetTime"].as_str().map(str::to_string),
                model: model.clone(),
            };
            if model.is_some() {
                group_windows.push(window);
            } else if !main.iter().any(|w: &UsageWindow| w.id == window.id) {
                main.push(window);
            }
        }
        group_windows.sort_by_key(|window| window.limit_window_seconds);
        scoped.extend(group_windows);
    }
    // The Gemini 5h window first, then its 7d, as Ghostex always listed them.
    main.sort_by_key(|window| window.id != "fiveHour");
    let mut usage = main;
    usage.extend(scoped);
    Reading {
        usage,
        plan,
        details: Vec::new(),
    }
}

/// A bucket's window as Ghostex labels it and its length: Google's `window` ("5h", "weekly"), else
/// its id's suffix (`gemini-5h`, `3p-weekly`).
fn bucket_window(bucket: &Value) -> Option<(&'static str, i64)> {
    let window = bucket["window"].as_str().unwrap_or_default();
    let id = bucket["bucketId"].as_str().unwrap_or_default();
    if window == "5h" || id.ends_with("-5h") {
        Some(("5h", 5 * 3600))
    } else if window == "weekly" || id.ends_with("-weekly") {
        Some(("7d", 7 * 86400))
    } else {
        None
    }
}

/// The short name a non-Gemini group's windows carry: "Claude/GPT" for the third-party models'
/// group, else Google's group name without a trailing "models".
fn group_name(group: &Value, buckets: &[Value]) -> String {
    let third_party = buckets.iter().any(|bucket| {
        bucket["bucketId"]
            .as_str()
            .is_some_and(|id| id.starts_with("3p-"))
    });
    if third_party {
        return "Claude/GPT".into();
    }
    let name = group["displayName"].as_str().unwrap_or_default().trim();
    let lower = name.to_lowercase();
    let name = if lower.ends_with(" models") {
        name[..name.len() - " models".len()].trim()
    } else {
        name
    };
    if name.is_empty() {
        "Other".into()
    } else {
        name.to_string()
    }
}
