//! Screenshots and recordings over HTTP, for agents and scripts:
//!
//! - `GET /screenshot`: the screen as a PNG.
//! - `GET /record?seconds=N`: records N seconds, returns an MP4.
//! - `POST /record/start` -> `{"id": n}`, then `POST /record/stop?id=n`: returns an MP4
//!   of everything in between.
//!
//! A recording is the H.264 stream voxl already encodes, rewrapped as MP4: the recorder
//! joins as one more viewer that acknowledges frames at once, so it costs almost no
//! CPU and never slows real viewers down. Like the stream, it holds frames only when
//! the screen changes, and it doesn't include the cursor (viewers draw it locally).

use crate::hub::{Client, Hub, Out};
use crate::mp4;
use axum::extract::{Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};

/// How long a started recording that ran into the length limit waits for its
/// `/record/stop` before it is discarded (the caller may have crashed or forgotten).
const UNCLAIMED: Duration = Duration::from_secs(60);

pub struct Recorder {
    pub hub: Arc<Hub>,
    /// Longest recording; a started one stops on its own after this.
    pub max: Duration,
    active: Mutex<HashMap<u64, Active>>,
    next: AtomicU64,
}

struct Active {
    stop: oneshot::Sender<()>,
    done: tokio::task::JoinHandle<Result<Vec<u8>, String>>,
}

impl Recorder {
    pub fn new(hub: Arc<Hub>, max: Duration) -> Arc<Self> {
        Arc::new(Recorder {
            hub,
            max,
            active: Mutex::new(HashMap::new()),
            next: AtomicU64::new(1),
        })
    }
}

/// The screen as captured (BGRX, 4 bytes a pixel, rows packed), for a screenshot.
pub struct Snapshot {
    pub width: usize,
    pub height: usize,
    pub bgrx: Vec<u8>,
    /// The latest change (`/wait`) the pixels include.
    pub seq: u64,
}

impl Snapshot {
    /// RGB (8 bits per channel), exactly as on screen.
    fn to_rgb(&self) -> Vec<u8> {
        let mut out = vec![0u8; self.width * self.height * 3];
        for (o, p) in out.chunks_exact_mut(3).zip(self.bgrx.chunks_exact(4)) {
            o.copy_from_slice(&[p[2], p[1], p[0]]);
        }
        out
    }
}

fn err(code: StatusCode, msg: impl Into<String>) -> Response {
    (
        code,
        [(header::CONTENT_TYPE, "application/json")],
        serde_json::json!({ "error": msg.into() }).to_string(),
    )
        .into_response()
}

fn video(mp4: Vec<u8>) -> Response {
    (
        [
            (header::CONTENT_TYPE, "video/mp4"),
            (header::CONTENT_DISPOSITION, "inline; filename=\"voxl.mp4\""),
        ],
        mp4,
    )
        .into_response()
}

pub async fn screenshot(State(r): State<Arc<Recorder>>) -> Response {
    let (tx, rx) = oneshot::channel();
    r.hub.snapshots.lock().push(tx);
    r.hub.wake();
    let Ok(Ok(frame)) = tokio::time::timeout(Duration::from_secs(5), rx).await else {
        return err(StatusCode::SERVICE_UNAVAILABLE, "no frame from capture");
    };
    let seq = frame.seq;
    let png = tokio::task::spawn_blocking(move || {
        let (w, h) = (frame.width as u32, frame.height as u32);
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        enc.write_header()
            .and_then(|mut w| w.write_image_data(&frame.to_rgb()))
            .map(|_| out)
            .map_err(|e| e.to_string())
    })
    .await;
    match png {
        Ok(Ok(png)) => {
            let mut res = (
                [
                    (header::CONTENT_TYPE, "image/png"),
                    (header::CACHE_CONTROL, "no-store"),
                ],
                png,
            )
                .into_response();
            // For `/wait?change&since=`: changes after this screenshot.
            res.headers_mut()
                .insert("x-voxl-seq", header::HeaderValue::from(seq));
            res
        }
        Ok(Err(e)) => err(StatusCode::INTERNAL_SERVER_ERROR, e),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

/// Joins as a viewer and collects frames until `stop`, `max` or a geometry change.
#[allow(clippy::result_large_err)]
fn begin(
    r: &Arc<Recorder>,
    stop: oneshot::Receiver<()>,
    max: Duration,
) -> Result<tokio::task::JoinHandle<Result<Vec<u8>, String>>, Response> {
    if r.hub.stripes.lock().len() > 1 {
        return Err(err(
            StatusCode::CONFLICT,
            "recording needs a single-stripe stream (--stripes 1)",
        ));
    }
    let (tx, rx) = mpsc::unbounded_channel();
    let client = r.hub.add_client(tx);
    client.video.store(true, Ordering::Relaxed);
    r.hub.wake();
    let hub = r.hub.clone();
    Ok(tokio::spawn(async move {
        let res = collect(&hub, &client, rx, stop, max).await;
        hub.remove_client(client.id);
        res
    }))
}

async fn collect(
    hub: &Hub,
    client: &Client,
    mut rx: mpsc::UnboundedReceiver<Out>,
    mut stop: oneshot::Receiver<()>,
    max: Duration,
) -> Result<Vec<u8>, String> {
    let start = Instant::now();
    let deadline = tokio::time::sleep(max);
    tokio::pin!(deadline);
    let mut frames: Vec<mp4::Frame> = Vec::new();
    let mut size = (0, 0);
    loop {
        tokio::select! {
            _ = &mut stop => break,
            _ = &mut deadline => break,
            m = rx.recv() => match m {
                None => break,
                Some(Out::Bin(b)) if b.len() > 16 && b[0] == 0x01 => {
                    // Acknowledge at once, so the recorder never holds the stream back.
                    client.on_ack(u32::from_be_bytes([b[2], b[3], b[4], b[5]]));
                    hub.wake();
                    if frames.is_empty() {
                        size = (hub.width.load(Ordering::Relaxed), hub.height.load(Ordering::Relaxed));
                    }
                    // Time the frame was captured: now, less the pipeline time in its header.
                    let pipe = u32::from_be_bytes([b[6], b[7], b[8], b[9]]) as u64 / 10;
                    let ms = (start.elapsed().as_millis() as u64).saturating_sub(pipe);
                    let ms = frames.last().map_or(ms, |l: &mp4::Frame| ms.max(l.ms + 1));
                    frames.push(mp4::Frame {
                        ms,
                        key: b[1] & 1 == 1,
                        data: b.slice(16..),
                    });
                }
                // A capture restart or closed subscription ends this recording.
                Some(Out::Text(t)) if !frames.is_empty()
                    && (t.contains("\"hello\"") || t.contains("\"close\"")) => break,
                _ => {}
            },
        }
    }
    let end = start.elapsed().as_millis() as u64;
    tokio::task::spawn_blocking(move || mp4::write(&frames, size.0, size.1, end))
        .await
        .map_err(|e| e.to_string())?
}

/// How long `GET /record?seconds=` records: `secs`, at most `max`. None unless positive.
fn record_len(secs: f64, max: Duration) -> Option<Duration> {
    if secs.is_nan() || secs <= 0.0 {
        return None;
    }
    // Cap before converting: from_secs_f64 panics on infinite or huge values, and
    // with panic = "abort" that would take the whole server down.
    Some(Duration::from_secs_f64(secs.min(max.as_secs_f64())))
}

pub async fn record(
    State(r): State<Arc<Recorder>>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let secs: f64 = q.get("seconds").and_then(|s| s.parse().ok()).unwrap_or(5.0);
    let Some(len) = record_len(secs, r.max) else {
        return err(StatusCode::BAD_REQUEST, "seconds must be positive");
    };
    // The deadline ends it; the stop side is never used.
    let (_keep, stop) = oneshot::channel();
    let done = match begin(&r, stop, len) {
        Ok(d) => d,
        Err(e) => return e,
    };
    match done.await {
        Ok(Ok(mp4)) => video(mp4),
        Ok(Err(e)) => err(StatusCode::INTERNAL_SERVER_ERROR, e),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn start(State(r): State<Arc<Recorder>>) -> Response {
    let (stop_tx, stop) = oneshot::channel();
    let done = match begin(&r, stop, r.max) {
        Ok(d) => d,
        Err(e) => return e,
    };
    let id = r.next.fetch_add(1, Ordering::Relaxed);
    r.active.lock().insert(
        id,
        Active {
            stop: stop_tx,
            done,
        },
    );
    log::info!("recording {id} started");
    // Nobody may ever stop it: once it has ended on its own, drop it after a grace period
    // instead of holding the clip in memory forever.
    let rec = r.clone();
    tokio::spawn(async move {
        tokio::time::sleep(rec.max + UNCLAIMED).await;
        if rec.active.lock().remove(&id).is_some() {
            log::info!("recording {id} was never stopped; discarded");
        }
    });
    (
        [(header::CONTENT_TYPE, "application/json")],
        serde_json::json!({ "id": id, "max_seconds": r.max.as_secs() }).to_string(),
    )
        .into_response()
}

pub async fn stop(
    State(r): State<Arc<Recorder>>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let id = q.get("id").and_then(|s| s.parse::<u64>().ok());
    let Some(a) = id.and_then(|id| r.active.lock().remove(&id)) else {
        return err(StatusCode::NOT_FOUND, "no such recording");
    };
    let _ = a.stop.send(());
    log::info!("recording {} stopped", id.unwrap_or(0));
    match a.done.await {
        Ok(Ok(mp4)) => video(mp4),
        Ok(Err(e)) => err(StatusCode::INTERNAL_SERVER_ERROR, e),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_len_is_positive_and_capped() {
        let max = Duration::from_secs(300);
        assert_eq!(record_len(2.5, max), Some(Duration::from_millis(2500)));
        assert_eq!(record_len(1e100, max), Some(max));
        assert_eq!(record_len(f64::INFINITY, max), Some(max));
        assert_eq!(record_len(0.0, max), None);
        assert_eq!(record_len(-1.0, max), None);
        assert_eq!(record_len(f64::NAN, max), None);
    }
}
