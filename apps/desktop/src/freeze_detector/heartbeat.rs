use gpui::App;

use super::watchdog;

/// Starts the freeze detector: a main-thread heartbeat and a watchdog thread that writes the
/// main thread's stack to `gpui-freezes.log` in the app's logs folder whenever the main thread
/// stops ticking for a few seconds. Nothing is written unless a freeze happens.
///
/// CDXC:Diagnostics 2026-11-19 DECISION: After the Linux app froze for about 80 seconds and quit
/// with nothing recorded, the user asked for a freeze detector "but only on these debug ones i
/// have on my machine dont do this for the app we publish to users please". It is the
/// `freeze-detector` cargo feature, which only the local-start build scripts turn on
/// (build-macos-rust.sh with GHOSTEX_LOCAL_START=1, build-linux-rust.sh); `cargo xtask build`
/// and every release build leave it off, so published apps do not contain it, and there is no
/// runtime setting that could turn it on. GPUI's own hang monitor (the `profiler` feature) was
/// not used: it reports a hang only after the stuck work returns, so a freeze that ends in a
/// quit or a kill is never reported, and it names no stack.
/// SEE-ALSO: apps/desktop/scripts/build-macos-rust.sh, apps/desktop/scripts/build-linux-rust.sh.
pub fn start(cx: &mut App) {
    watchdog::tick();
    watchdog::start(crate::support_logs::logs_directory().join("gpui-freezes.log"));
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor().timer(watchdog::TICK_EVERY).await;
            watchdog::tick();
        }
    })
    .detach();
}
