//! Streaming Grok dictation: the client's audio goes to xAI's streaming speech-to-text and the
//! text comes back as it is recognized. Nothing is spoken and nothing is sent; the client puts the
//! text into the draft. Dictate itself records first and transcribes the saved file since
//! 2026-10-20 (`recordings.rs`, `transcribe.rs`); this socket stays for app versions released
//! before that and for `ghostex voice --dictate`.
//!
//! CDXC:VoiceMode 2026-10-20 DECISION:
//! User: "use grok bot's api for the dictation mic button"; the chat-box button "just pastes whatever i say into the chat box, while the one above is more general and for discussing". Dictation uses xAI speech-to-text with the xAI key kept on the computer and only fills the draft, never sends (the 2026-10-08 SessionChat decision stands).

use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt as _, StreamExt as _};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message as WsMessage;

use super::provider::{self, ConnectError, Provider};
use super::{keys, socket};
use crate::server::AppState;

/// Runs one dictation on `client` until the client ends or cancels it.
pub(super) async fn run(state: &AppState, mut client: WebSocket, language: Option<String>) {
    let key = match keys::read_key_async(&state.paths, Provider::Xai).await {
        Ok(Some(key)) => key,
        Ok(None) => {
            socket::fail(
                &mut client,
                "noKey",
                "Dictation uses your xAI API key: add it in Settings > Extensions > Voice agent.",
            )
            .await;
            return;
        }
        Err(message) => {
            socket::fail(&mut client, "noKey", &message).await;
            return;
        }
    };
    let upstream = match provider::connect_dictation(&key, language.as_deref()).await {
        Ok(upstream) => upstream,
        Err(ConnectError::BadKey) => {
            socket::fail(
                &mut client,
                "badKey",
                "xAI refused the API key. Check it in Settings > Extensions > Voice agent.",
            )
            .await;
            return;
        }
        Err(ConnectError::Refused(message)) => {
            super::warn_error(state, "voiceAgent.dictationRefused", &message, json!({}));
            socket::fail(&mut client, "providerRefused", &message).await;
            return;
        }
        Err(ConnectError::Unavailable(message)) => {
            super::warn(
                state,
                "voiceAgent.dictationUnavailable",
                json!({"message": message}),
            );
            socket::fail(
                &mut client,
                "providerUnavailable",
                &format!("Couldn't reach xAI: {message}"),
            )
            .await;
            return;
        }
    };
    let (mut up_sink, mut up_stream) = upstream.split();
    let mut ready = false;
    let mut early_audio: Vec<bytes::Bytes> = Vec::new();
    // Segments the recognizer has locked, by start time, and its current guess after them.
    let mut transcript = Transcript::default();
    let mut ending = false;
    loop {
        tokio::select! {
            frame = client.recv() => match frame {
                Some(Ok(Message::Binary(audio))) if !ending => {
                    if ready {
                        if up_sink.send(WsMessage::Binary(audio)).await.is_err() {
                            break;
                        }
                    } else {
                        early_audio.push(audio);
                    }
                }
                Some(Ok(Message::Text(text))) => {
                    let message: Value = serde_json::from_str(text.as_str()).unwrap_or(Value::Null);
                    match message.get("type").and_then(Value::as_str) {
                        Some("end") if !ending => {
                            ending = true;
                            let _ = up_sink.send(WsMessage::Text(json!({"type": "audio.done"}).to_string().into())).await;
                        }
                        Some("cancel") => break,
                        _ => {}
                    }
                }
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                Some(Ok(_)) => {}
            },
            frame = up_stream.next() => match frame {
                Some(Ok(WsMessage::Text(text))) => {
                    let event: Value = serde_json::from_str(text.as_str()).unwrap_or(Value::Null);
                    match event.get("type").and_then(Value::as_str).unwrap_or_default() {
                        "transcript.created" => {
                            ready = true;
                            let _ = client.send(Message::Text(json!({
                                "type": "ready",
                                "mode": "dictation",
                                "audio": {"encoding": "pcm16", "sampleRate": provider::SAMPLE_RATE, "channels": 1},
                            }).to_string().into())).await;
                            for audio in early_audio.drain(..) {
                                if up_sink.send(WsMessage::Binary(audio)).await.is_err() {
                                    break;
                                }
                            }
                        }
                        "transcript.partial" => {
                            let chunk = event.get("text").and_then(Value::as_str).unwrap_or_default().trim().to_string();
                            let is_final = event.get("is_final").and_then(Value::as_bool).unwrap_or(false);
                            let speech_final = event.get("speech_final").and_then(Value::as_bool).unwrap_or(false);
                            let start = event.get("start").and_then(Value::as_f64).map_or(-1, |start| (start * 1000.0).round() as i64);
                            transcript.update(start, chunk, is_final);
                            let text = transcript.text();
                            let _ = client.send(Message::Text(json!({
                                "type": "transcript",
                                "text": text,
                                "final": is_final,
                                "speechFinal": speech_final,
                            }).to_string().into())).await;
                        }
                        "transcript.done" => {
                            // `transcript.done` carries what was still unlocked when audio ended.
                            let tail = event.get("text").and_then(Value::as_str).unwrap_or_default().trim().to_string();
                            if !tail.is_empty() {
                                transcript.update(i64::MAX, tail, true);
                            }
                            let _ = client.send(Message::Text(json!({"type": "done", "text": transcript.locked_text()}).to_string().into())).await;
                            break;
                        }
                        "error" => {
                            let message = event.get("message").and_then(Value::as_str).unwrap_or("xAI speech-to-text failed.").to_string();
                            super::warn(state, "voiceAgent.dictationError", json!({"message": message}));
                            let _ = client.send(Message::Text(json!({"type": "error", "code": "providerError", "message": message, "fatal": true}).to_string().into())).await;
                            break;
                        }
                        _ => {}
                    }
                }
                Some(Ok(WsMessage::Close(_))) | None | Some(Err(_)) => {
                    if ending {
                        let _ = client.send(Message::Text(json!({"type": "done", "text": transcript.locked_text()}).to_string().into())).await;
                    } else {
                        let _ = client.send(Message::Text(json!({"type": "error", "code": "providerError", "message": "xAI ended the dictation.", "fatal": true}).to_string().into())).await;
                    }
                    break;
                }
                Some(Ok(_)) => {}
            },
        }
    }
    let _ = up_sink.send(WsMessage::Close(None)).await;
    let _ = client.send(Message::Close(None)).await;
}

/// The dictation so far. xAI sends each stretch of speech as interim guesses, then a chunk-final
/// and an utterance-final event that repeat the same segment (same `start`), so locked text is
/// kept per segment start rather than appended.
#[derive(Default)]
struct Transcript {
    locked: std::collections::BTreeMap<i64, String>,
    interim: Option<(i64, String)>,
}

impl Transcript {
    fn update(&mut self, start: i64, text: String, is_final: bool) {
        if is_final {
            if self.interim.as_ref().is_some_and(|(at, _)| *at <= start) {
                self.interim = None;
            }
            if text.is_empty() {
                self.locked.remove(&start);
            } else {
                self.locked.insert(start, text);
            }
        } else {
            self.interim = Some((start, text));
        }
    }

    fn locked_text(&self) -> String {
        self.locked
            .values()
            .map(String::as_str)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn text(&self) -> String {
        let mut parts: Vec<&str> = self
            .locked
            .iter()
            .filter(|(start, _)| self.interim.as_ref().is_none_or(|(at, _)| at != *start))
            .map(|(_, text)| text.as_str())
            .collect();
        if let Some((_, interim)) = &self.interim {
            parts.push(interim);
        }
        parts
            .into_iter()
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }
}
