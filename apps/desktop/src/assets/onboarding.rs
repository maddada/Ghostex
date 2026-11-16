//! The onboarding's own pictures: its line icon set (`ICON_PATHS` in
//! packages/core-ui/onboarding/primitives.tsx (deleted 2026-10-01)), the Ghostex logo and the agent cursor. Icons are asked for as `icon/<name>/<stroke width x10>.svg` because the React icons take
//! a stroke width per use and GPUI tints an SVG by its alpha, so each width is its own picture.
use std::borrow::Cow;

const ICON_PATHS: &[(&str, &str)] = &[
    ("terminal", r#"<path d="M5 7l5 5-5 5M12.5 18h6.5"/>"#),
    // The Workspace panel's Unified sidebar row: a window with its sidebar column.
    (
        "sidebar",
        r#"<rect x="3" y="4.5" width="18" height="15" rx="2.5"/><path d="M9.5 4.5v15M5.6 8.5h1.6M5.6 11.5h1.6"/>"#,
    ),
    (
        "browser",
        r#"<rect x="3" y="4.5" width="18" height="15" rx="2.5"/><path d="M3 9h18"/><path d="M6.3 6.8h.01M8.8 6.8h.01" stroke-width="2.2"/>"#,
    ),
    (
        "phone",
        r#"<rect x="7" y="2.5" width="10" height="19" rx="2.6"/><path d="M11 18.6h2"/>"#,
    ),
    (
        "home",
        r#"<path d="M4 11.2 12 4l8 7.2v8.3a1 1 0 0 1-1 1h-4.6v-5.8H9.6v5.8H5a1 1 0 0 1-1-1z" fill="black" stroke="none"/>"#,
    ),
    (
        "users",
        r#"<circle cx="9" cy="8.5" r="3.2"/><path d="M3.5 19c.6-3 2.8-4.8 5.5-4.8s4.9 1.8 5.5 4.8"/><circle cx="16.6" cy="9.4" r="2.5"/><path d="M15.7 14.3c2.3.1 4.1 1.8 4.6 4.7"/>"#,
    ),
    (
        "org",
        r#"<rect x="9.5" y="3" width="5" height="4.5" rx="1"/><rect x="3" y="15.5" width="5" height="4.5" rx="1"/><rect x="16" y="15.5" width="5" height="4.5" rx="1"/><rect x="9.5" y="15.5" width="5" height="4.5" rx="1"/><path d="M12 7.5v8M5.5 15.5v-3h13v3"/>"#,
    ),
    (
        "grid",
        r#"<rect x="3.5" y="3.5" width="7" height="7" rx="1.2"/><rect x="13.5" y="3.5" width="7" height="7" rx="1.2"/><rect x="3.5" y="13.5" width="7" height="7" rx="1.2"/><rect x="13.5" y="13.5" width="7" height="7" rx="1.2"/>"#,
    ),
    (
        "bell",
        r#"<path d="M6 16.5V11a6 6 0 0 1 12 0v5.5l1.5 2h-15z"/><path d="M10 20.5a2 2 0 0 0 4 0"/>"#,
    ),
    (
        "monitor",
        r#"<rect x="3" y="4" width="18" height="12.5" rx="1.8"/><path d="M9 20.5h6M12 16.5v4"/>"#,
    ),
    (
        "lock",
        r#"<rect x="5" y="10.5" width="14" height="10" rx="2"/><path d="M8 10.5V7.5a4 4 0 0 1 8 0v3"/>"#,
    ),
    (
        "shield",
        r#"<path d="M12 3l7 3v5.5c0 4.5-3 8-7 9.5-4-1.5-7-5-7-9.5V6z"/>"#,
    ),
    (
        "refresh",
        r#"<path d="M19.5 11.5A7.5 7.5 0 0 0 6 7.3M5 4v4h4M4.5 12.5A7.5 7.5 0 0 0 18 16.7M19 20v-4h-4"/>"#,
    ),
    ("check", r#"<path d="M5 12.5l4.5 4.5L19 7.5"/>"#),
    (
        "checkCircle",
        r#"<circle cx="12" cy="12" r="9"/><path d="M8 12.3l2.8 2.8 5.5-5.6"/>"#,
    ),
    (
        "folder",
        r#"<path d="M3 7a1.5 1.5 0 0 1 1.5-1.5h4.3l2 2.2h8.7A1.5 1.5 0 0 1 21 9.2V18a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 18z"/>"#,
    ),
    (
        "chat",
        r#"<path d="M4 6.5A2.5 2.5 0 0 1 6.5 4h11A2.5 2.5 0 0 1 20 6.5v8a2.5 2.5 0 0 1-2.5 2.5H10l-4.5 3.5V17A2.5 2.5 0 0 1 4 14.5z"/><path d="M8.5 10.5h.01M12 10.5h.01M15.5 10.5h.01" stroke-width="2.2"/>"#,
    ),
    ("arrowR", r#"<path d="M4 12h15M13 6l6 6-6 6"/>"#),
    ("arrowL", r#"<path d="M20 12H5M11 6l-6 6 6 6"/>"#),
    ("chevR", r#"<path d="M9 5l7 7-7 7"/>"#),
    ("chevL", r#"<path d="M15 5l-7 7 7 7"/>"#),
    ("plus", r#"<path d="M12 5v14M5 12h14"/>"#),
    ("x", r#"<path d="M6 6l12 12M18 6 6 18"/>"#),
    (
        "external",
        r#"<path d="M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5"/>"#,
    ),
    (
        "code",
        r#"<path d="M8 7l-5 5 5 5M16 7l5 5-5 5M13.5 5l-3 14"/>"#,
    ),
    (
        "kanban",
        r#"<rect x="3.5" y="3.5" width="17" height="17" rx="2"/><rect x="7.5" y="8.5" width="6" height="8" rx=".6"/>"#,
    ),
    (
        "bolt",
        r#"<path d="M13 2.5 5 13.5h6l-1 8 8-11h-6z" fill="black" stroke="none"/>"#,
    ),
    (
        "target",
        r#"<circle cx="12" cy="12" r="8.5"/><circle cx="12" cy="12" r="4.2"/>"#,
    ),
    ("list", r#"<path d="M5 7h14M5 12h14M5 17h14"/>"#),
    (
        "send",
        r#"<path d="M4 12 20 4l-6 16-2.5-6.5z" fill="black" stroke="none"/>"#,
    ),
    (
        "copy",
        r#"<rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V5.5A1.5 1.5 0 0 0 14.5 4h-9A1.5 1.5 0 0 0 4 5.5v9A1.5 1.5 0 0 0 5.5 16H8"/>"#,
    ),
    (
        "signal",
        r#"<path d="M4 18v-2M8 18v-5M12 18V9M16 18V6" stroke-width="2.2"/>"#,
    ),
    (
        "wifi",
        r#"<path d="M3.5 9.5a12 12 0 0 1 17 0M6.5 12.8a7.5 7.5 0 0 1 11 0M9.6 16a3 3 0 0 1 4.8 0"/>"#,
    ),
    ("pulse", r#"<path d="M3 12h4l2-5 4 10 2-5h6"/>"#),
    // The intro video still's play button (window/onboarding/intro_video.rs).
    (
        "play",
        r#"<path d="M8.5 5.6v12.8a.9.9 0 0 0 1.37.77l10.2-6.4a.9.9 0 0 0 0-1.54L9.87 4.83a.9.9 0 0 0-1.37.77z" fill="black" stroke="none"/>"#,
    ),
    (
        "layers",
        r#"<path d="M12 3 3 8l9 5 9-5z"/><path d="M3 12.5l9 5 9-5M3 16.5l9 5 9-5"/>"#,
    ),
    (
        "sparkle",
        r#"<path d="M12 3c.6 4.5 2.5 6.4 7 7-4.5.6-6.4 2.5-7 7-.6-4.5-2.5-6.4-7-7 4.5-.6 6.4-2.5 7-7z"/>"#,
    ),
    // A screenshot frame: the Floating Capture row on the Mobile panel.
    (
        "capture",
        r#"<path d="M4 8.5V5.5a1.5 1.5 0 0 1 1.5-1.5h3M15.5 4h3A1.5 1.5 0 0 1 20 5.5v3M20 15.5v3a1.5 1.5 0 0 1-1.5 1.5h-3M8.5 20h-3A1.5 1.5 0 0 1 4 18.5v-3"/><circle cx="12" cy="12" r="3"/>"#,
    ),
    // The devices page (window/onboarding/mobile.rs): another computer, a Cloud Box, Easy
    // Connect's code and Tailscale's network.
    (
        "laptop",
        r#"<rect x="4.5" y="5" width="15" height="10.5" rx="1.6"/><path d="M2.5 18.5h19"/>"#,
    ),
    (
        "cloud",
        r#"<path d="M7.2 18.5h9.6a4 4 0 0 0 .5-7.97 5.5 5.5 0 0 0-10.6 1.5A3.25 3.25 0 0 0 7.2 18.5z"/>"#,
    ),
    (
        "qr",
        r#"<rect x="4" y="4" width="6" height="6" rx="1"/><rect x="14" y="4" width="6" height="6" rx="1"/><rect x="4" y="14" width="6" height="6" rx="1"/><path d="M14 14h2.5v2.5H14zM17.5 17.5H20V20h-2.5zM14 19.6h1.2M19.6 14v1.2"/>"#,
    ),
    (
        "globe",
        r#"<circle cx="12" cy="12" r="8.5"/><path d="M3.5 12h17M12 3.5c2.3 2.4 3.4 5.2 3.4 8.5s-1.1 6.1-3.4 8.5c-2.3-2.4-3.4-5.2-3.4-8.5s1.1-6.1 3.4-8.5z"/>"#,
    ),
    (
        "wrench",
        r#"<path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z"/>"#,
    ),
];

/// The agent cursor drawn over the Browser and Computer Use previews (`.agent-cursor` in
/// packages/core-ui/onboarding/styles/workspace.css, deleted 2026-10-01).
const AGENT_CURSOR: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M4 2l16 10-7 1.6L9.5 21z" fill="#fff" stroke="#2d5ae8" stroke-width="1.6" stroke-linejoin="round"/></svg>"##;

pub(crate) fn asset(key: &str) -> Option<Cow<'static, [u8]>> {
    match key {
        "logo.webp" => {
            return Some(Cow::Borrowed(include_bytes!(
                "../../../../packages/core-ui/onboarding/assets/ghostex-logo.webp"
            )));
        }
        "cursor.svg" => return Some(Cow::Borrowed(AGENT_CURSOR.as_bytes())),
        // The Mobile panel's tilted phone, captured from the React story (window/onboarding/mobile.rs).
        "phone/shell.webp" => {
            return Some(Cow::Borrowed(include_bytes!(
                "../../assets/onboarding/phone/shell.webp"
            )));
        }
        "phone/get.webp" => {
            return Some(Cow::Borrowed(include_bytes!(
                "../../assets/onboarding/phone/get.webp"
            )));
        }
        "phone/loading.webp" => {
            return Some(Cow::Borrowed(include_bytes!(
                "../../assets/onboarding/phone/loading.webp"
            )));
        }
        "phone/open.webp" => {
            return Some(Cow::Borrowed(include_bytes!(
                "../../assets/onboarding/phone/open.webp"
            )));
        }
        "phone/connect.webp" => {
            return Some(Cow::Borrowed(include_bytes!(
                "../../assets/onboarding/phone/connect.webp"
            )));
        }
        "phone/connect-tap.webp" => {
            return Some(Cow::Borrowed(include_bytes!(
                "../../assets/onboarding/phone/connect-tap.webp"
            )));
        }
        "phone/cam.webp" => {
            return Some(Cow::Borrowed(include_bytes!(
                "../../assets/onboarding/phone/cam.webp"
            )));
        }
        "phone/paired.webp" => {
            return Some(Cow::Borrowed(include_bytes!(
                "../../assets/onboarding/phone/paired.webp"
            )));
        }
        "phone/home-notify.webp" => {
            return Some(Cow::Borrowed(include_bytes!(
                "../../assets/onboarding/phone/home-notify.webp"
            )));
        }
        "phone/home-notify-nobanner.webp" => {
            return Some(Cow::Borrowed(include_bytes!(
                "../../assets/onboarding/phone/home-notify-nobanner.webp"
            )));
        }
        "phone/home-quiet.webp" => {
            return Some(Cow::Borrowed(include_bytes!(
                "../../assets/onboarding/phone/home-quiet.webp"
            )));
        }
        // The Welcome panel's Coordinators picture (window/onboarding/welcome.rs), 810x440 points at 2x
        // with transparency, drawn from the first-run art folder's welcome-coordinators@2x.png.
        "welcome-coordinators.webp" => {
            return Some(Cow::Borrowed(include_bytes!(
                "../../assets/onboarding/welcome-coordinators.webp"
            )));
        }
        // A frame of the intro video (10.2s), drawn under the intro page's player and in place of it
        // where there is no web view or no network (window/onboarding/intro_video.rs).
        "intro-video.webp" => {
            return Some(Cow::Borrowed(include_bytes!(
                "../../assets/onboarding/intro-video.webp"
            )));
        }

        _ => {}
    }
    let rest = key.strip_prefix("icon/")?.strip_suffix(".svg")?;
    let (name, stroke) = rest.split_once('/')?;
    let stroke = stroke.parse::<u32>().ok().filter(|stroke| *stroke <= 60)?;
    let paths = ICON_PATHS
        .iter()
        .find(|(icon, _)| *icon == name)
        .map(|(_, paths)| *paths)?;
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round">{paths}</svg>"#,
        stroke as f32 / 10.0
    );
    Some(Cow::Owned(svg.into_bytes()))
}
