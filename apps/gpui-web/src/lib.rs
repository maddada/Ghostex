use std::borrow::Cow;

/// CDXC:WebGpui 2026-10-26 WHY: never glob-import `gpui::*` here. The shared desktop files reach the crate root through `use crate::*`, so a glob would hand them `gpui::test`, which shadows the built-in `#[test]` and expands back into `#[test]`; the test target then dies with "recursion limit reached while expanding `#[test]`" however high the limit goes.
use gpui::{App, AppCell, Application, KeyBinding, WindowOptions, prelude::*};
use gpui_component::{Root, theme::Theme};
use wasm_bindgen::prelude::*;

mod app;
// The desktop crate root doubles as a prelude (`use crate::*` in its modules), so the shared files expect the same names here.
#[allow(unused_imports)]
mod prelude {
    pub(crate) use anyhow::Result;
    pub(crate) use gpui::prelude::FluentBuilder as _;
    pub(crate) use gpui::{
        Action, AnyElement, App, AppContext as _, Bounds, ClipboardEntry, ClipboardItem,
        ContentMask, DismissEvent, Element, ElementId, Entity, FocusHandle, Focusable as _,
        FontWeight, GlobalElementId, Hitbox, Hsla, Image, InteractiveElement as _, IntoElement,
        KeyBinding, KeyDownEvent, Keystroke, LayoutId, Modifiers, MouseButton, MouseDownEvent,
        MouseUpEvent, ParentElement as _, Pixels, Point, PressureStage, Render, RenderOnce,
        ScrollDelta, ScrollHandle, Size, StatefulInteractiveElement as _, Style, Styled as _,
        Window, WindowBounds, WindowControlArea, WindowOptions, canvas, div, point, px, relative,
        rgb, rgba, size, svg,
    };
    pub(crate) use gpui_component::menu::PopupMenu;
    pub(crate) use gpui_component::scroll::Scrollbar;
    pub(crate) use gpui_component::tooltip::Tooltip;
    pub(crate) use gpui_component::{Root, Selectable, h_flex, v_flex};
    pub(crate) use std::cell::RefCell;
    pub(crate) use std::collections::{HashMap, HashSet};
    pub(crate) use std::ops::Range;
    pub(crate) use std::path::{Path, PathBuf};
    pub(crate) use std::rc::Rc;
    pub(crate) use std::sync::Arc;
    pub(crate) use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    pub(crate) use std::time::Duration;
    pub(crate) use web_time::{Instant, SystemTime, UNIX_EPOCH};
}
pub(crate) use crate::app::consts::*;
pub(crate) use crate::app::helpers::*;
pub(crate) use crate::app::hotkeys::*;
pub(crate) use crate::app::model::*;
pub(crate) use crate::app::sidebar_direct_focus::*;
pub(crate) use crate::app::web_app::GhostexGpuiApp;
pub(crate) use prelude::*;

mod assets;
mod cef;
/// The sidebar's and Settings' image cache and data URL decoders, the desktop's file.
#[allow(dead_code)]
mod data_url_image;
/// The web build has the sidebar's folds but not the Settings rail, which uses the rest.
#[allow(dead_code)]
mod fold_motion;
#[allow(dead_code)]
mod ghostty_vt;
mod host_name;
mod hotkey_label;
/// The desktop's notification feed reads a bridge the browser build does not have; the bell stays hidden, so its state is always empty.
#[allow(dead_code)]
mod notification_feed {
    include!(concat!(env!("OUT_DIR"), "/notification_feed.rs"));

    #[derive(Default)]
    pub(crate) struct GpuiNotificationFeedState {
        pub(crate) unread_count: usize,
    }
}
mod shared_settings;
mod shell;
mod support_logs;
#[allow(dead_code)]
mod terminal_element;
mod terminal_gpui_engine;
#[allow(dead_code)]
mod terminal_model;
#[allow(dead_code)]
mod terminal_scrollbar_reveal;
#[allow(dead_code)]
mod terminal_wheel;
mod ui_fonts;

// Linux-only window identity on the desktop; a canvas has neither.
fn gpui_platform_window_app_id() -> Option<String> {
    None
}

fn gpui_platform_window_icon() -> Option<Arc<image::RgbaImage>> {
    None
}

/// The faces `gpui_web` used to bundle: IBM Plex Sans (the UI font) and
/// Lilex (the fallback for a monospace family the browser does not have).
///
/// CDXC:WebGpui 2026-09-26 WHY: upstream `gpui_web` stopped bundling IBM Plex Sans and Lilex (zed `ef075910c9`, applications provide their fonts) but still names IBM Plex Sans as the system UI font and `.ZedMono` (Lilex) as the first fallback, so without these faces no font resolves and the first text layout panics the page, and inline code falls back to the sans face. The faces come from the Zed checkout's own assets, the files the platform used to bundle.
fn platform_fonts() -> Vec<Cow<'static, [u8]>> {
    macro_rules! zed_font {
        ($path:literal) => {
            include_bytes!(concat!("../../../.dependencies/zed/assets/fonts/", $path)).as_slice()
        };
    }
    [
        zed_font!("ibm-plex-sans/IBMPlexSans-Regular.ttf"),
        zed_font!("ibm-plex-sans/IBMPlexSans-Italic.ttf"),
        zed_font!("ibm-plex-sans/IBMPlexSans-SemiBold.ttf"),
        zed_font!("ibm-plex-sans/IBMPlexSans-SemiBoldItalic.ttf"),
        zed_font!("lilex/Lilex-Regular.ttf"),
        zed_font!("lilex/Lilex-Bold.ttf"),
        zed_font!("lilex/Lilex-Italic.ttf"),
        zed_font!("lilex/Lilex-BoldItalic.ttf"),
    ]
    .into_iter()
    .map(Cow::Borrowed)
    .collect()
}

#[wasm_bindgen]
pub fn run() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Info).expect("Failed to initialize logger");

    #[cfg(target_family = "wasm")]
    gpui_platform::web_init();
    // CDXC:Settings 2026-10-08 SEE-ALSO: `?retention=on` (with `?a11y=off`, since GPUI Fast retains
    // nothing while the accessibility mirror is on) turns on the desktop's Faster rendering for
    // this page, so a retained frame can be tested in a browser (`app/view_retention.rs`).
    gpui::set_default_view_retention(
        web_sys::window()
            .and_then(|window| window.location().search().ok())
            .is_some_and(|search| search.contains("retention=on")),
    );
    #[cfg(not(target_family = "wasm"))]
    let app = gpui_platform::application();
    #[cfg(target_family = "wasm")]
    let app = {
        let app = gpui_platform::single_threaded_web();
        // `run()` returns to JavaScript instead of blocking in a native run loop, so the app cell is leaked to stay alive (same workaround as gpui-component's story-web).
        struct WasmApplication(std::rc::Rc<AppCell>);
        let wasm_app = unsafe { std::mem::transmute::<Application, WasmApplication>(app) };
        std::mem::forget(wasm_app.0.clone());
        unsafe { std::mem::transmute::<WasmApplication, Application>(wasm_app) }
    };

    // `?settings=<url-encoded JSON object>` stands in for the settings the page has no source for
    // (README, "Settings are empty"), so a setting-driven look can be tested in a browser, for
    // example `?settings={"markedHandovers":true,"markedHandoversStyle":"badge"}`.
    if let Some(settings) = web_sys::window()
        .and_then(|window| window.location().search().ok())
        .and_then(|search| {
            search
                .trim_start_matches('?')
                .split('&')
                .find_map(|pair| pair.strip_prefix("settings=").map(str::to_string))
        })
        .and_then(|encoded| js_sys::decode_uri_component(&encoded).ok())
        .and_then(|decoded| {
            serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&String::from(
                decoded,
            ))
            .ok()
        })
    {
        shared_settings::install(settings);
        app::helpers::apply_settings_theme_colours();
    }
    app.with_assets(assets::GhostexAssets).run(|cx: &mut App| {
        // A Mac browser gets the Mac text editing keys in every text input (the chat composer's Cmd+Backspace and friends).
        cx.set_global(gpui_component::input::MacTextKeys(
            terminal_element::uses_macos_text_editing(),
        ));
        gpui_component::init(cx);
        ui_fonts::register(cx);
        // Match desktop's terminal keymap: Root's focus traversal otherwise
        // consumes Tab/Shift+Tab before the shared terminal encoder sees them.
        cx.bind_keys([
            KeyBinding::new(
                "tab",
                gpui::NoAction {},
                Some(terminal_element::TERMINAL_KEY_CONTEXT),
            ),
            KeyBinding::new(
                "shift-tab",
                gpui::NoAction {},
                Some(terminal_element::TERMINAL_KEY_CONTEXT),
            ),
        ]);

        let emoji_font = Cow::Borrowed(include_bytes!("../fonts/NotoEmoji-Regular.ttf").as_slice());
        let mono_font =
            Cow::Borrowed(include_bytes!("../fonts/JetBrainsMono-Regular.ttf").as_slice());
        // CDXC:WebGpui 2026-10-22 DECISION: The user asked for Noto Sans Arabic in the web build: a browser page has no system fonts and the Canvas fallback only covers emoji and CJK, so Arabic, Persian and Urdu text drew as boxes. Text shaping finds it as a fallback face for those letters; the desktop app keeps using the system's fonts. The OFL licence sits beside the file (`fonts/NotoSansArabic-OFL.txt`).
        let arabic_font =
            Cow::Borrowed(include_bytes!("../fonts/NotoSansArabic-Regular.ttf").as_slice());
        cx.text_system()
            .add_fonts(vec![emoji_font, mono_font, arabic_font])
            .expect("Failed to load fonts");
        cx.text_system()
            .add_fonts(platform_fonts())
            .expect("Failed to load the platform fonts");
        app::helpers::apply_gpui_component_theme(cx);
        cx.global_mut::<Theme>().mono_font_family = "JetBrains Mono".into();
        // Fenced code reads this through the base theme: unsynced, it asked for a family the page
        // doesn't have, drew proportional and followed a picked app font.
        Theme::sync_base(cx);
        // A page has no system fonts: `?settings=` can only pick one of the faces it bundles, and
        // any other name leaves it on its default fonts.
        ui_fonts::apply_fonts(
            shared_settings::shared_sidebar_settings_snapshot().object(),
            cx,
        );

        cx.open_window(WindowOptions::default(), |window, cx| {
            let shell = cx.new(|cx| {
                let mut app = GhostexGpuiApp::new(cx);
                app.web_host.main_window = Some(window.window_handle());
                app::web_host::install_path_links(cx.entity().downgrade());
                if crate::app::collab_guest::guest().is_some() {
                    app.collab_guest_start(cx);
                } else {
                    app.gx_store_start(cx);
                }
                app
            });
            cx.new(|cx| Root::new(shell, window, cx))
        })
        .expect("Failed to open window");
        cx.activate(true);
    });

    Ok(())
}
