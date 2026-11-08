//! Settings › Dictation › Dictation history: every Dictate recording gxserver keeps on this
//! computer (`/api/listDictations`, server/src/voice_agent/recordings.rs), newest first and grouped
//! by day, with what became of it: its text, "This transcription was dismissed" with Recover, or
//! why it failed with Try again. Hovering a row offers Play, Download, Copy, Transcribe (the text
//! goes to the clipboard too) and Delete; the search field filters by text.
//!
//! CDXC:VoiceMode 2026-10-20 DECISION:
//! User: "i want to have a history area at the bototm of the settings voice page that lets you see all that you transcribed and be able to recover stuff you dismissed and be able to play old recordings or download them or transcribe them and get text into your clipboard. recover basically sends that voice recording to the api to get you the text basically (wasn't send because u dismissed)". The look follows Wispr Flow's history (rows grouped by day, the time on the left, actions on hover, a search field). Recordings stay until the user deletes them.

use super::super::super::native_modal_kit::*;
use super::super::fields::{ButtonSize, ButtonVariant, settings_button_sized, settings_text_input};
use super::super::palette::SettingsPalette;
use super::super::store::{SettingsStore, store_copy_to_clipboard, store_gxserver_rpc};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Task, Window, div, px,
};
use gpui_component::input::{InputEvent, InputState};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const LIST_TIMEOUT: Duration = Duration::from_secs(10);
const TRANSCRIBE_TIMEOUT: Duration = Duration::from_secs(180);
/// Rows drawn at first; Show more adds this many again.
const PAGE: usize = 100;

pub(crate) struct DictationHistory {
    store: Entity<SettingsStore>,
    search: Entity<InputState>,
    items: Vec<Value>,
    count: u64,
    total_bytes: u64,
    loaded: bool,
    error: Option<String>,
    shown: usize,
    /// Rows waiting for gxserver (transcribing, deleting).
    busy: HashSet<String>,
    /// The row whose Delete was pressed once and asks again.
    confirm_delete: Option<String>,
    /// A row's own note (a download saved, a failure), by id.
    notes: std::collections::HashMap<String, (bool, String)>,
    playing: Option<(String, Arc<Mutex<std::process::Child>>)>,
    _poll: Option<Task<()>>,
    _search_subscription: gpui::Subscription,
    search_task: Option<Task<()>>,
}

impl DictationHistory {
    pub(crate) fn new(
        store: Entity<SettingsStore>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search dictations"));
        let subscription = cx.subscribe(&search, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                // Typing settles before gxserver is asked.
                this.search_task = Some(cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(250))
                        .await;
                    let _ = this.update(cx, |this, cx| this.load(cx));
                }));
            }
        });
        let mut history = Self {
            store,
            search,
            items: Vec::new(),
            count: 0,
            total_bytes: 0,
            loaded: false,
            error: None,
            shown: PAGE,
            busy: HashSet::new(),
            confirm_delete: None,
            notes: Default::default(),
            playing: None,
            _poll: None,
            _search_subscription: subscription,
            search_task: None,
        };
        history.load(cx);
        history
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        let query = self.search.read(cx).value().trim().to_string();
        let page = cx.entity().downgrade();
        store_gxserver_rpc(
            &self.store,
            "/api/listDictations",
            json!({ "query": query }),
            LIST_TIMEOUT,
            move |result, cx| {
                let _ = page.update(cx, |page, cx| {
                    page.loaded = true;
                    match result {
                        Ok(answer) => {
                            page.error = None;
                            page.items =
                                answer["dictations"].as_array().cloned().unwrap_or_default();
                            page.count = answer["count"].as_u64().unwrap_or(0);
                            page.total_bytes = answer["totalBytes"].as_u64().unwrap_or(0);
                        }
                        Err(error) => page.error = Some(error),
                    }
                    cx.notify();
                });
            },
            cx,
        );
    }

    fn replace_item(&mut self, dictation: &Value) {
        if let Some(item) = self
            .items
            .iter_mut()
            .find(|item| item["id"] == dictation["id"])
        {
            *item = dictation.clone();
        }
    }

    /// Transcribe, Recover and Try again: the text comes back and goes to the clipboard.
    fn transcribe(&mut self, id: String, cx: &mut Context<Self>) {
        self.busy.insert(id.clone());
        self.notes.remove(&id);
        let page = cx.entity().downgrade();
        let store = self.store.clone();
        store_gxserver_rpc(
            &self.store,
            "/api/transcribeDictation",
            json!({ "id": id }),
            TRANSCRIBE_TIMEOUT,
            move |result, cx| {
                let _ = page.update(cx, |page, cx| {
                    page.busy.remove(&id);
                    match result {
                        Ok(answer) => {
                            let dictation = &answer["dictation"];
                            page.replace_item(dictation);
                            match dictation["text"].as_str().map(str::trim) {
                                Some(text)
                                    if dictation["status"] == "transcribed" && !text.is_empty() =>
                                {
                                    store_copy_to_clipboard(&store, text.to_string(), cx);
                                }
                                Some(_) if dictation["status"] == "transcribed" => {
                                    page.notes.insert(
                                        id.clone(),
                                        (
                                            false,
                                            "No words were heard in this recording.".to_string(),
                                        ),
                                    );
                                }
                                _ => {}
                            }
                        }
                        Err(error) => {
                            page.notes.insert(id.clone(), (false, error));
                        }
                    }
                    cx.notify();
                });
            },
            cx,
        );
        cx.notify();
    }

    fn delete(&mut self, id: String, cx: &mut Context<Self>) {
        if self.confirm_delete.as_deref() != Some(id.as_str()) {
            self.confirm_delete = Some(id);
            cx.notify();
            return;
        }
        self.confirm_delete = None;
        if self
            .playing
            .as_ref()
            .is_some_and(|(playing, _)| *playing == id)
        {
            self.stop_playing(cx);
        }
        self.busy.insert(id.clone());
        let page = cx.entity().downgrade();
        store_gxserver_rpc(
            &self.store,
            "/api/deleteDictation",
            json!({ "id": id }),
            LIST_TIMEOUT,
            move |result, cx| {
                let _ = page.update(cx, |page, cx| {
                    page.busy.remove(&id);
                    match result {
                        Ok(_) => {
                            page.items
                                .retain(|item| item["id"].as_str() != Some(id.as_str()));
                            page.count = page.count.saturating_sub(1);
                        }
                        Err(error) => {
                            page.notes.insert(id.clone(), (false, error));
                        }
                    }
                    cx.notify();
                });
            },
            cx,
        );
        cx.notify();
    }

    /// Plays a recording with macOS's own `afplay`; pressing again (or another Play) stops it.
    fn toggle_play(&mut self, id: String, path: String, cx: &mut Context<Self>) {
        let same = self
            .playing
            .as_ref()
            .is_some_and(|(playing, _)| *playing == id);
        self.stop_playing(cx);
        if same {
            return;
        }
        let child = std::process::Command::new("/usr/bin/afplay")
            .arg(&path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        match child {
            Ok(child) => {
                let child = Arc::new(Mutex::new(child));
                self.playing = Some((id, child.clone()));
                self._poll = Some(cx.spawn(async move |this, cx| {
                    loop {
                        cx.background_executor()
                            .timer(Duration::from_millis(300))
                            .await;
                        let finished = child
                            .lock()
                            .map(|mut child| !matches!(child.try_wait(), Ok(None)))
                            .unwrap_or(true);
                        if finished {
                            let _ = this.update(cx, |this, cx| {
                                if this
                                    .playing
                                    .as_ref()
                                    .is_some_and(|(_, playing)| Arc::ptr_eq(playing, &child))
                                {
                                    this.playing = None;
                                    cx.notify();
                                }
                            });
                            break;
                        }
                    }
                }));
            }
            Err(error) => {
                self.notes
                    .insert(id, (false, format!("Couldn't play the recording: {error}")));
            }
        }
        cx.notify();
    }

    fn stop_playing(&mut self, cx: &mut Context<Self>) {
        if let Some((_, child)) = self.playing.take()
            && let Ok(mut child) = child.lock()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
        self._poll = None;
        cx.notify();
    }

    /// Saves a copy of the audio where the reader picks (Downloads to start with).
    fn download(&mut self, id: String, path: String, created_at: i64, cx: &mut Context<Self>) {
        let extension = std::path::Path::new(&path)
            .extension()
            .map(|ext| ext.to_string_lossy().into_owned())
            .unwrap_or_else(|| "m4a".to_string());
        let when = chrono::DateTime::from_timestamp_millis(created_at)
            .map(|at| {
                at.with_timezone(&chrono::Local)
                    .format("%Y-%m-%d %H.%M")
                    .to_string()
            })
            .unwrap_or_else(|| id.clone());
        let suggested = format!("Dictation {when}.{extension}");
        let downloads = dirs_downloads();
        let chosen = cx.prompt_for_new_path(&downloads, Some(&suggested));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(target))) = chosen.await else {
                return;
            };
            let copied = std::fs::copy(&path, &target);
            let _ = this.update(cx, |this, cx| {
                this.notes.insert(
                    id,
                    match copied {
                        Ok(_) => (true, format!("Saved to {}", target.display())),
                        Err(error) => (false, format!("Couldn't save it: {error}")),
                    },
                );
                cx.notify();
            });
        })
        .detach();
    }

    fn render_row(&self, p: &SettingsPalette, item: &Value, cx: &mut Context<Self>) -> AnyElement {
        let id = item["id"].as_str().unwrap_or_default().to_string();
        let path = item["path"].as_str().unwrap_or_default().to_string();
        let created_at = item["createdAt"].as_i64().unwrap_or(0);
        let status = item["status"].as_str().unwrap_or_default();
        let text = item["text"]
            .as_str()
            .map(str::trim)
            .filter(|text| !text.is_empty());
        let busy = self.busy.contains(&id) || status == "transcribing";
        let time = chrono::DateTime::from_timestamp_millis(created_at)
            .map(|at| at.with_timezone(&chrono::Local).format("%H:%M").to_string())
            .unwrap_or_default();
        let length = clock(item["durationMs"].as_u64().unwrap_or(0));
        let group: SharedString = format!("dictation-row-{id}").into();
        let (body, action): (AnyElement, Option<(&'static str, bool)>) = if busy {
            (muted(p, "Transcribing…"), None)
        } else if status == "dismissed" && text.is_none() {
            (
                muted(p, "This transcription was dismissed."),
                Some(("Recover", true)),
            )
        } else if status == "failed" {
            let reason = item["error"]["message"]
                .as_str()
                .unwrap_or("The transcription failed.");
            (
                div()
                    .text_size(px(13.0))
                    .text_color(hsla(p.destructive))
                    .child(format!("Transcription failed: {reason}"))
                    .into_any_element(),
                Some(("Try again", true)),
            )
        } else if let Some(text) = text {
            (
                div()
                    .text_size(px(13.5))
                    .text_color(hsla(p.foreground))
                    .child(text.to_string())
                    .into_any_element(),
                None,
            )
        } else if status == "transcribed" {
            (muted(p, "No words were heard."), None)
        } else {
            (muted(p, "Not transcribed yet."), Some(("Transcribe", true)))
        };
        let playing = self
            .playing
            .as_ref()
            .is_some_and(|(playing, _)| *playing == id);
        let note = self.notes.get(&id).cloned();
        let confirm = self.confirm_delete.as_deref() == Some(id.as_str());
        let small = |label: &'static str,
                     key: &str,
                     on_click: Box<dyn Fn(&mut Self, &mut Window, &mut Context<Self>)>,
                     cx: &mut Context<Self>| {
            settings_button_sized(
                p,
                SharedString::from(format!("dictation-{key}-{id}")),
                label,
                None,
                ButtonVariant::Outline,
                ButtonSize::Xs,
                false,
                None,
                move |page: &mut Self, window, cx| on_click(page, window, cx),
                cx,
            )
        };
        let mut actions = h_flex().gap(px(6.0)).flex_shrink_0();
        if let Some((label, _)) = action {
            let id = id.clone();
            actions = actions.child(small(
                label,
                "transcribe",
                Box::new(move |page, _, cx| page.transcribe(id.clone(), cx)),
                cx,
            ));
        }
        let hover_actions = {
            let mut row = h_flex().gap(px(6.0)).flex_shrink_0();
            {
                let (id, path) = (id.clone(), path.clone());
                row = row.child(small(
                    if playing { "Stop" } else { "Play" },
                    "play",
                    Box::new(move |page, _, cx| page.toggle_play(id.clone(), path.clone(), cx)),
                    cx,
                ));
            }
            {
                let (id, path) = (id.clone(), path.clone());
                row = row.child(small(
                    "Download",
                    "download",
                    Box::new(move |page, _, cx| {
                        page.download(id.clone(), path.clone(), created_at, cx)
                    }),
                    cx,
                ));
            }
            if let Some(text) = text {
                let text = text.to_string();
                let store = self.store.clone();
                row = row.child(small(
                    "Copy",
                    "copy",
                    Box::new(move |_, _, cx| store_copy_to_clipboard(&store, text.clone(), cx)),
                    cx,
                ));
            } else if action.is_none() && !busy {
                let id = id.clone();
                row = row.child(small(
                    "Transcribe",
                    "retranscribe",
                    Box::new(move |page, _, cx| page.transcribe(id.clone(), cx)),
                    cx,
                ));
            }
            {
                let id = id.clone();
                row = row.child(small(
                    if confirm { "Delete for good" } else { "Delete" },
                    "delete",
                    Box::new(move |page, _, cx| page.delete(id.clone(), cx)),
                    cx,
                ));
            }
            row
        };
        let reveal = playing || confirm;
        h_flex()
            .id(SharedString::from(format!("dictation-{id}")))
            .group(group.clone())
            .w_full()
            .px(px(16.0))
            .py(px(10.0))
            .gap(px(14.0))
            .items_start()
            .hover(|style| style.bg(hsla(css_fade(p.raised_hover, 0.35))))
            .child(
                v_flex()
                    .w(px(52.0))
                    .flex_shrink_0()
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(hsla(p.muted))
                            .child(time),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(hsla(p.muted))
                            .child(length),
                    ),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(6.0))
                    .child(body)
                    .when_some(note, |this, (ok, note)| {
                        this.child(
                            div()
                                .text_size(px(12.0))
                                .text_color(hsla(if ok { p.muted } else { p.destructive }))
                                .child(note),
                        )
                    })
                    .child(
                        h_flex().gap(px(6.0)).flex_wrap().child(actions).child(
                            hover_actions
                                .when(!reveal, |this| this.opacity(0.0))
                                .group_hover(group.clone(), |style| style.opacity(1.0)),
                        ),
                    ),
            )
            .into_any_element()
    }
}

fn muted(p: &SettingsPalette, text: &'static str) -> AnyElement {
    div()
        .text_size(px(13.0))
        .text_color(hsla(p.muted))
        .child(text)
        .into_any_element()
}

/// `m:ss` for a recording's length.
fn clock(duration_ms: u64) -> String {
    let seconds = duration_ms / 1000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn dirs_downloads() -> std::path::PathBuf {
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .map(|home| home.join("Downloads"))
        .filter(|path| path.is_dir())
        .unwrap_or_else(std::env::temp_dir)
}

/// "Today", "Yesterday", or the date.
fn day_label(created_at: i64) -> String {
    let Some(at) = chrono::DateTime::from_timestamp_millis(created_at) else {
        return String::new();
    };
    let day = at.with_timezone(&chrono::Local).date_naive();
    let today = chrono::Local::now().date_naive();
    if day == today {
        "TODAY".to_string()
    } else if Some(day) == today.pred_opt() {
        "YESTERDAY".to_string()
    } else {
        day.format("%B %-d, %Y").to_string().to_uppercase()
    }
}

impl Render for DictationHistory {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.store.read(cx).palette();
        let megabytes = self.total_bytes as f64 / 1_000_000.0;
        let summary = if self.count == 0 {
            "Every dictation is kept here, on this computer, until you delete it.".to_string()
        } else {
            format!(
                "{} recording{} · {megabytes:.1} MB on this computer, kept until you delete them (about 0.3 MB a minute).",
                self.count,
                if self.count == 1 { "" } else { "s" }
            )
        };
        let mut list = v_flex().w_full();
        let mut last_day = String::new();
        let items: Vec<Value> = self.items.iter().take(self.shown).cloned().collect();
        for item in &items {
            let day = day_label(item["createdAt"].as_i64().unwrap_or(0));
            if day != last_day {
                list = list.child(
                    div()
                        .px(px(16.0))
                        .pt(px(14.0))
                        .pb(px(4.0))
                        .text_size(px(11.0))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(hsla(p.muted))
                        .child(day.clone()),
                );
                last_day = day;
            }
            list = list.child(self.render_row(&p, item, cx));
        }
        let more = self.items.len() > self.shown;
        v_flex()
            .w_full()
            .gap(px(4.0))
            .child(
                h_flex()
                    .w_full()
                    .px(px(16.0))
                    .pt(px(12.0))
                    .gap(px(10.0))
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(12.5))
                            .text_color(hsla(p.muted))
                            .child(summary),
                    )
                    .child(div().w(px(220.0)).child(settings_text_input(
                        &p,
                        &self.search,
                        None,
                        false,
                        window,
                        cx,
                    ))),
            )
            .when_some(self.error.clone(), |this, error| {
                this.child(
                    div()
                        .px(px(16.0))
                        .text_size(px(12.5))
                        .text_color(hsla(p.destructive))
                        .child(format!("Couldn't read the dictation history: {error}")),
                )
            })
            .when(
                self.loaded && self.items.is_empty() && self.error.is_none(),
                |this| {
                    this.child(
                        div()
                            .px(px(16.0))
                            .py(px(12.0))
                            .text_size(px(13.0))
                            .text_color(hsla(p.muted))
                            .child(if self.search.read(cx).value().trim().is_empty() {
                                "No dictations yet. Press the microphone in a chat box to dictate."
                            } else {
                                "No dictation matches your search."
                            }),
                    )
                },
            )
            .child(list)
            .when(more, |this| {
                this.child(div().px(px(16.0)).py(px(10.0)).child(settings_button_sized(
                    &p,
                    "dictation-history-more",
                    "Show more",
                    None,
                    ButtonVariant::Outline,
                    ButtonSize::Sm,
                    false,
                    None,
                    |page: &mut Self, _, cx| {
                        page.shown += PAGE;
                        cx.notify();
                    },
                    cx,
                )))
            })
            .pb(px(8.0))
    }
}

impl Drop for DictationHistory {
    fn drop(&mut self) {
        if let Some((_, child)) = self.playing.take()
            && let Ok(mut child) = child.lock()
        {
            let _ = child.kill();
        }
    }
}
