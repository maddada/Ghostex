//! voxl: low-latency desktop streaming (H.264 over WebSocket) for the browser, from an
//! X11 display on Linux or the screen of a Mac or a Windows machine.
//!
//! The `voxl` binary calls [`run`] with its command line; other programs (the Tembo CLI's
//! `tembo voxl`) embed the server the same way.

mod access;
#[cfg(all(target_os = "linux", feature = "pulseaudio"))]
mod audio;
#[cfg(target_os = "linux")]
mod capture;
mod convert;
#[cfg(target_os = "linux")]
mod corners;
#[cfg(target_os = "linux")]
mod desktop;
mod encoder;
mod files;
mod help;
mod hub;
#[cfg(target_os = "linux")]
mod input;
mod login;
#[cfg(target_os = "macos")]
mod mac;
mod mic;
mod mp4;
mod record;
#[cfg(target_os = "linux")]
mod screen;
mod server;
mod tls;
mod video;
mod wait;
#[cfg(windows)]
mod win;

// The macOS and Windows backends provide the same modules with the same interfaces.
#[cfg(target_os = "macos")]
use mac::{audio, capture, desktop, input, screen};
#[cfg(windows)]
use win::{audio, capture, desktop, input, screen};

use std::collections::HashMap;
use std::time::Instant;

/// Parses `--key=value` / `--key value` / `--flag`; unknown options are accepted so the
/// binary can take the same argument list as the Python selkies service it replaces.
fn parse_args(args: impl IntoIterator<Item = String>) -> HashMap<String, String> {
    let mut m = HashMap::new();
    let args: Vec<String> = args.into_iter().collect();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if let Some(k) = a.strip_prefix("--") {
            if let Some((k, v)) = k.split_once('=') {
                m.insert(k.replace('_', "-"), v.to_string());
            } else if i + 1 < args.len() && !args[i + 1].starts_with("--") {
                m.insert(k.replace('_', "-"), args[i + 1].clone());
                i += 1;
            } else {
                m.insert(k.replace('_', "-"), "true".into());
            }
        }
        i += 1;
    }
    m
}

/// Selkies ranges look like "60-60"; take the upper bound.
fn range_max(v: Option<&String>, default: f64) -> f64 {
    v.and_then(|s| s.rsplit('-').next()?.trim().parse().ok())
        .unwrap_or(default)
}

pub(crate) fn truthy(v: Option<&String>, default: bool) -> bool {
    v.map(|s| matches!(s.as_str(), "true" | "1" | "yes" | "on"))
        .unwrap_or(default)
}

/// Runs the server with these command-line flags (without the program name) until it's
/// told to stop. Takes over the calling thread, which on macOS has to be the main thread.
pub fn run(args: impl IntoIterator<Item = String>) {
    let args: Vec<String> = args.into_iter().collect();
    // --help and --version print and return instead of starting the server.
    if help::print_if_asked(&args) {
        return;
    }
    let a = parse_args(args);
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .try_init();
    // A panic on any thread ends the process, as in voxl's own release build (panic = "abort"),
    // rather than leaving the server up without that thread when the embedder unwinds.
    #[cfg(panic = "unwind")]
    {
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            hook(info);
            std::process::abort();
        }));
    }
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("tokio runtime");
    // AppKit and the keyboard layout APIs want the main thread, so on macOS it runs the
    // main dispatch queue and the server runs beside it.
    #[cfg(target_os = "macos")]
    {
        std::thread::spawn(move || rt.block_on(serve(a)));
        dispatch2::dispatch_main();
    }
    #[cfg(not(target_os = "macos"))]
    rt.block_on(serve(a));
}

async fn serve(a: HashMap<String, String>) {
    // macOS: --display picks a screen (index or display ID); DISPLAY there belongs to XQuartz.
    #[cfg(target_os = "linux")]
    let display = std::env::var("DISPLAY")
        .ok()
        .or_else(|| a.get("display").cloned());
    #[cfg(not(target_os = "linux"))]
    let display = a.get("display").cloned();
    #[cfg(target_os = "macos")]
    mac::init(&a);
    #[cfg(windows)]
    win::init(&a);
    // A Mac's or a PC's own screen is only served locally unless --addr says otherwise;
    // a VM's X11 display sits behind the platform's proxy.
    let default_addr = if cfg!(not(target_os = "linux")) {
        "127.0.0.1"
    } else {
        "0.0.0.0"
    };
    let addr = a
        .get("addr")
        .cloned()
        .unwrap_or_else(|| default_addr.into());
    let port: u16 = a.get("port").and_then(|p| p.parse().ok()).unwrap_or(6901);
    let ncpu = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4) as u32;
    let vcfg = video::VideoConfig {
        display: display.clone(),
        #[cfg(target_os = "macos")]
        enable_resize: truthy(a.get("enable-resize"), false) && !truthy(a.get("read-only"), false),
        fps: range_max(a.get("framerate"), 60.0) as u32,
        preset: a
            .get("preset")
            .cloned()
            .unwrap_or_else(|| "ultrafast".into()),
        // Four sliced threads: lower latency than one, and x264's slicing overhead grows
        // with thread count, so more threads only add CPU (measured on 2560x1600).
        threads: a
            .get("encoder-threads")
            .and_then(|v| v.parse().ok())
            .unwrap_or((ncpu / 4).clamp(1, 4)),
        convert_threads: a
            .get("convert-threads")
            .and_then(|v| v.parse().ok())
            .unwrap_or(4),
        crf: range_max(a.get("crf"), 24.0) as f32,
        // Selkies' --video-bitrate becomes the ceiling for adaptive bitrate.
        max_kbps: range_max(a.get("max-kbps").or(a.get("video-bitrate")), 30000.0) as u32,
        paintover_crf: range_max(a.get("paintover-crf"), 16.0) as f32,
        paintover_burst: range_max(a.get("paintover-burst"), 3.0) as u32,
        paintover_delay_ms: range_max(a.get("paintover-delay-ms"), 150.0) as u64,
        min_kbps: range_max(a.get("min-kbps"), 2000.0) as u32,
        stripes: range_max(a.get("stripes"), 1.0) as u32,
        // Opt-in: one thread unless a frame would miss the frame budget. x264's sliced
        // threads cost ~3x the CPU on desktop content, so this saves 20-55% CPU, but
        // small updates encode a few ms slower on one thread.
        adaptive_threads: truthy(a.get("adaptive-threads"), false),
        coalesce: std::time::Duration::from_micros(range_max(a.get("coalesce-us"), 0.0) as u64),
    };
    let clip = a
        .get("enable-clipboard")
        .map(|s| s.as_str())
        .unwrap_or("true");
    let clip_mode = desktop::ClipboardMode {
        to_server: matches!(clip, "true" | "in"),
        to_client: matches!(clip, "true" | "out"),
    };
    let audio_on = truthy(a.get("audio-enabled"), true);
    #[cfg(all(target_os = "linux", not(feature = "pulseaudio")))]
    let audio_on = audio_on && {
        log::info!("desktop audio off: this voxl was built without PulseAudio");
        false
    };

    let hub = hub::Hub::new();
    // --read-only: the viewer watches the desktop, it does not drive it. Only the
    // commands watching needs are accepted (see server::READ_ONLY_COMMANDS), uploads
    // are refused, and the page hides the controls that would send anything else.
    let read_only = truthy(a.get("read-only"), false);
    // --enable-resize (as in selkies): clients resize the desktop to their window. Not
    // for read-only viewers: a new size changes the desktop for everyone.
    let enable_resize = truthy(a.get("enable-resize"), false);
    if enable_resize && read_only {
        log::warn!("--enable-resize is ignored with --read-only: viewers can't resize the desktop");
    }
    let screen = (enable_resize && !read_only)
        .then(|| match screen::Screen::open(display.as_deref()) {
            Ok(s) => {
                log::info!("dynamic resolution on, up to {}x{}", s.max().0, s.max().1);
                Some(s)
            }
            Err(e) => {
                log::warn!("dynamic resolution unavailable: {e}");
                None
            }
        })
        .flatten();
    let resize_on = screen.is_some();
    // Desktop pixels per CSS pixel of the viewer's window. The default matches a
    // 144 DPI desktop (apps drawn at 1.5x), so windows look as big as in the viewer;
    // on a Mac, the capture's pixels per point.
    #[cfg(target_os = "linux")]
    let default_scale = 1.5;
    #[cfg(target_os = "macos")]
    let default_scale = mac::capture_scale();
    // On Windows, the display's scaling (150% -> 1.5).
    #[cfg(windows)]
    let default_scale = win::dpi_scale();
    let resize_scale = range_max(a.get("resize-scale"), default_scale);
    if let Err(e) = video::spawn(vcfg, hub.clone(), screen) {
        log::error!("video init failed: {e}");
        std::process::exit(1);
    }
    let radius = range_max(a.get("round-corners"), 0.0) as u16;
    // --round-windows=title:radius,...: round specific windows (docks included) by title.
    let named: Vec<(String, u16)> = a
        .get("round-windows")
        .map(|v| {
            v.split(',')
                .filter_map(|e| {
                    e.rsplit_once(':')
                        .and_then(|(n, r)| Some((n.to_string(), r.parse().ok()?)))
                })
                .collect()
        })
        .unwrap_or_default();
    if radius > 0 || !named.is_empty() {
        #[cfg(target_os = "linux")]
        if let Err(e) = corners::spawn(display.clone(), radius, named) {
            log::warn!("rounded corners unavailable: {e}");
        }
        #[cfg(not(target_os = "linux"))]
        log::info!("--round-corners/--round-windows only apply to X11 desktops");
    }
    let clipboard = match desktop::spawn(display.clone(), hub.clone(), clip_mode) {
        Ok(h) => Some(h),
        Err(e) => {
            log::error!("cursor/clipboard init failed: {e}");
            None
        }
    };
    let px_per_notch = range_max(a.get("scroll-px-per-notch"), 180.0);
    match input::spawn(display.clone(), clipboard, px_per_notch) {
        Ok(tx) => *hub.input.lock() = Some(tx),
        Err(e) => log::error!("input init failed: {e}"),
    }
    #[cfg(any(not(target_os = "linux"), feature = "pulseaudio"))]
    if audio_on {
        let dev = a
            .get("audio-device-name")
            .cloned()
            .unwrap_or_else(|| "output.monitor".into());
        let br = range_max(a.get("audio-bitrate"), 128000.0) as i32;
        audio::spawn(dev, br, hub.clone());
    }

    // --file-transfers=upload,download (as in selkies); "none" turns both off.
    let transfers = a
        .get("file-transfers")
        .cloned()
        .unwrap_or_else(|| "upload,download".into());
    let root = a
        .get("files-root")
        .cloned()
        .or_else(|| std::env::var("HOME").ok())
        .or_else(|| std::env::var("USERPROFILE").ok())
        .unwrap_or_else(|| "/".into());
    let root = std::fs::canonicalize(&root).unwrap_or_else(|_| root.into());
    let files = std::sync::Arc::new(files::Files {
        root,
        // Uploading writes to the desktop, so a read-only viewer does not get it.
        upload: !read_only && transfers.split(',').any(|t| t.trim() == "upload"),
        download: transfers.split(',').any(|t| t.trim() == "download"),
        default_dir: a
            .get("upload-dir")
            .cloned()
            .unwrap_or_else(|| "Downloads".into())
            .trim_matches('/')
            .to_string(),
        max_upload_bytes: (range_max(a.get("max-upload-mb"), 0.0) * 1048576.0) as u64,
    });
    log::info!("file transfer: {transfers} under {}", files.root.display());
    // --microphone-enabled (as in selkies): browser microphone -> a FIFO that a
    // PulseAudio module-pipe-source reads as the VM's microphone, or on macOS an
    // output device that loops back to an input (BlackHole and the like).
    #[cfg(target_os = "linux")]
    let mic_sink = Some(
        a.get("microphone-fifo")
            .cloned()
            .unwrap_or_else(|| "/tmp/voxl-mic.fifo".into()),
    );
    #[cfg(target_os = "macos")]
    let mic_sink = truthy(a.get("microphone-enabled"), false)
        .then(|| mac::mic::find_device(a.get("microphone-device").map(|s| s.as_str())))
        .flatten();
    // Windows: a virtual audio cable's playback side (VB-CABLE and the like).
    #[cfg(windows)]
    let mic_sink = truthy(a.get("microphone-enabled"), false)
        .then(|| win::mic::find_device(a.get("microphone-device").map(|s| s.as_str())))
        .flatten();
    let mic = truthy(a.get("microphone-enabled"), false)
        .then_some(())
        .and(mic_sink)
        .map(|sink| mic::spawn(sink, hub.clone()));
    {
        let mut extra = hub.hello_extra.lock();
        extra.insert("os".into(), std::env::consts::OS.into());
        extra.insert("audio".into(), audio_on.into());
        extra.insert("mic".into(), mic.is_some().into());
        if read_only {
            extra.insert("readOnly".into(), true.into());
        }
        extra.insert(
            "files".into(),
            serde_json::json!({"upload": files.upload, "download": files.download, "dir": files.default_dir}),
        );
        if resize_on {
            extra.insert(
                "resize".into(),
                serde_json::json!({ "scale": resize_scale }),
            );
        }
    }
    // --favicon: the tab's icon (default: voxl's own).
    let (favicon, favicon_type) = server::favicon(a.get("favicon").map(|s| s.as_str()))
        .unwrap_or_else(|e| {
            log::error!("{e}");
            std::process::exit(1);
        });
    let state = server::AppState {
        hub: hub.clone(),
        audio: audio_on,
        epoch: Instant::now(),
        files,
        mic,
        resize: enable_resize && !read_only,
        read_only,
        // GET /screenshot, /record; --max-record-seconds caps a recording's length.
        recorder: record::Recorder::new(
            hub.clone(),
            std::time::Duration::from_secs_f64(range_max(a.get("max-record-seconds"), 300.0)),
        ),
        // --title: the browser tab's title.
        index: server::index_html(
            a.get("title")
                .map_or("Voxl Computer Stream", |t| t.as_str()),
        ),
        favicon,
        favicon_type,
    };
    // --allowed-hosts and --allowed-origins: the host names and other sites voxl answers
    // besides IP addresses, localhost and its own page (see access.rs).
    let access = std::sync::Arc::new(access::Access::new(
        a.get("allowed-hosts").map(|s| s.as_str()),
        a.get("allowed-origins").map(|s| s.as_str()),
    ));
    // The login (see login.rs): a password, given or made up at startup, unless
    // --enable-basic-auth=false.
    let login =
        login::Login::from_args(&a).map(|(l, generated)| (std::sync::Arc::new(l), generated));
    let mut app = server::router(state);
    if let Some((l, _)) = &login {
        app = app.layer(axum::middleware::from_fn_with_state(
            l.clone(),
            login::check,
        ));
    }
    // Added last, so it runs first: a request for the wrong host or from another site's
    // page is turned away before the login is asked for.
    let app = app.layer(axum::middleware::from_fn_with_state(access, access::check));
    // HTTPS (see tls.rs). A self-signed certificate names this machine, the bound address
    // and the --allowed-hosts names.
    let tls = {
        let mut names = vec!["localhost".to_string(), "127.0.0.1".into(), "::1".into()];
        for v in ["COMPUTERNAME", "HOSTNAME"] {
            if let Ok(h) = std::env::var(v) {
                names.push(h.to_ascii_lowercase());
            }
        }
        if !matches!(
            addr.as_str(),
            "0.0.0.0" | "::" | "127.0.0.1" | "::1" | "localhost"
        ) {
            names.push(addr.clone());
        }
        for h in a.get("allowed-hosts").map_or("", |s| s.as_str()).split(',') {
            let h = h.trim();
            if !h.is_empty() && h != "*" {
                names.push(
                    h.strip_prefix('.')
                        .map_or(h.to_string(), |d| format!("*.{d}")),
                );
            }
        }
        names.sort();
        names.dedup();
        tls::from_args(&a, names)
    };
    let scheme = if tls.is_some() { "https" } else { "http" };
    let listener = tokio::net::TcpListener::bind((addr.as_str(), port))
        .await
        .expect("bind");
    log::info!("listening on {scheme}://{addr}:{port}");
    // Where to point a browser on this machine.
    let page = match addr.as_str() {
        "0.0.0.0" | "::" => format!("{scheme}://localhost:{port}/"),
        a if a.contains(':') => format!("{scheme}://[{a}]:{port}/"),
        a => format!("{scheme}://{a}:{port}/"),
    };
    match &login {
        Some((l, true)) => log::info!(
            "open {page}?token={} to sign in (the password is {0}; choose one with --basic-auth-password or VOXL_PASSWORD)",
            l.password()
        ),
        Some((_, false)) => log::info!(
            "login on: sign in at {page} with the password, or open {page}?token=<password>"
        ),
        None => log::info!("login off (--enable-basic-auth=false): anyone who reaches {page} can use this desktop"),
    }
    let shutdown_hub = hub.clone();
    tokio::spawn(async move {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let mut term = signal(SignalKind::terminate()).unwrap();
            let mut int = signal(SignalKind::interrupt()).unwrap();
            tokio::select! { _ = term.recv() => {}, _ = int.recv() => {} }
        }
        #[cfg(windows)]
        let _ = tokio::signal::ctrl_c().await;
        log::info!("shutting down");
        if let Some(tx) = shutdown_hub.input.lock().as_ref() {
            let _ = tx.send(input::InputEvent::Shutdown);
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        std::process::exit(0);
    });
    if let Some(config) = tls {
        return tls::serve(listener, app, config).await;
    }
    use axum::serve::ListenerExt;
    let listener = listener.tap_io(|tcp| {
        let _ = tcp.set_nodelay(true);
    });
    axum::serve(listener, app).await.unwrap();
}
