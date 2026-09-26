//! Opt-in, bounded timing aggregation. No allocation or disk I/O in timed paths.
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

static ACTIVE: AtomicBool = AtomicBool::new(false);
static COUNTERS: [Counters; 7] = [const { Counters::new() }; 7];

#[derive(Clone, Copy)]
pub(crate) enum Metric {
    AppRender,
    PopupOpen,
    PopupBuild,
    PopupRender,
    ProjectFocus,
    AgentCreate,
    CefCreate,
}
const NAMES: [&str; 7] = [
    "appRender",
    "popupOpen",
    "popupBuild",
    "popupRender",
    "projectFocus",
    "agentCreate",
    "cefCreate",
];
struct Counters {
    count: AtomicU64,
    micros: AtomicU64,
    max_micros: AtomicU64,
    over_16ms: AtomicU64,
}
impl Counters {
    const fn new() -> Self {
        Self {
            count: AtomicU64::new(0),
            micros: AtomicU64::new(0),
            max_micros: AtomicU64::new(0),
            over_16ms: AtomicU64::new(0),
        }
    }
}
pub(crate) struct Span(Option<(Metric, Instant)>);
pub(crate) fn span(metric: Metric) -> Span {
    Span(
        ACTIVE
            .load(Ordering::Relaxed)
            .then(|| (metric, Instant::now())),
    )
}
impl Drop for Span {
    fn drop(&mut self) {
        if let Some((metric, start)) = self.0 {
            let elapsed = start.elapsed().as_micros().min(u64::MAX as u128) as u64;
            let counters = &COUNTERS[metric as usize];
            counters.micros.fetch_add(elapsed, Ordering::Relaxed);
            counters.max_micros.fetch_max(elapsed, Ordering::Relaxed);
            counters
                .over_16ms
                .fetch_add(u64::from(elapsed > 16_667), Ordering::Relaxed);
            counters.count.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// CDXC:Diagnostics 2026-09-05 DECISION:
/// User requested profiling enabled with a flag to investigate desktop performance and RAM use.
/// Aggregation keeps instrumentation disk writes off the UI thread and stops after 30 minutes.
/// The flag is the only opt-in: the Debugging page has no profiling switch (user decision 2026-09-26).
pub(crate) fn start() {
    if !std::env::args().any(|arg| arg == "--profile") {
        return;
    }
    ACTIVE.store(true, Ordering::Relaxed);
    std::thread::spawn(|| {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(1800) {
            std::thread::sleep(Duration::from_secs(1));
            let metrics: Vec<_> = COUNTERS
                .iter()
                .zip(NAMES)
                .map(|(c, name)| {
                    serde_json::json!({
                        "name": name,
                        "count": c.count.swap(0, Ordering::Relaxed),
                        "totalUs": c.micros.swap(0, Ordering::Relaxed),
                        "maxUs": c.max_micros.swap(0, Ordering::Relaxed),
                        "over16ms": c.over_16ms.swap(0, Ordering::Relaxed),
                    })
                })
                .collect();
            crate::support_logs::append_profile_sample(
                serde_json::json!({"pid": std::process::id(), "elapsedMs": start.elapsed().as_millis() as u64, "metrics": metrics}),
            );
        }
        ACTIVE.store(false, Ordering::Relaxed);
    });
}
