//! The UI scale the Linux app runs at. The whole app is an X11 client (see
//! `force_gpui_x11_backend_for_windowed_cef` in `main.rs`), so under a Wayland compositor it
//! runs on XWayland, where X11 carries the scale the user picked only when the desktop says it
//! (`Xft.dpi`, XSETTINGS) and the compositor otherwise leaves it to each app.

use std::env;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const GPUI_X11_SCALE_FACTOR: &str = "GPUI_X11_SCALE_FACTOR";

static PINNED_SCALE_FACTOR: OnceLock<f32> = OnceLock::new();
static DETECTED_SCALE: OnceLock<DetectedScale> = OnceLock::new();

/// Where the scale the app runs at came from, for Settings > Interface > Display scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
// The setup launcher (bin/ghostex_gpui_cef_bootstrap.rs) shares this file and reads none of it.
#[allow(dead_code)]
pub(crate) enum ScaleSource {
    /// `GPUI_X11_SCALE_FACTOR` was set before Ghostex started; it wins over Settings too.
    Environment,
    /// `Xft.dpi` in the X resource database (GNOME, KDE), which GPUI reads itself.
    XResources,
    /// Hyprland's focused monitor.
    Hyprland,
    /// The desktop's XSETTINGS (COSMIC, and any desktop with an XSettings manager).
    XSettings,
    /// Nothing announced a scale under XWayland, so 1.0.
    Default,
    /// A plain X11 session: GPUI picks the scale itself.
    Gpui,
}

/// What the scale picker found at launch.
#[derive(Clone, Copy, Debug)]
// The setup launcher (bin/ghostex_gpui_cef_bootstrap.rs) shares this file and reads none of it.
#[allow(dead_code)]
pub(crate) struct DetectedScale {
    /// The scale Automatic gives, when Ghostex knows it (`None` when GPUI decides).
    pub automatic: Option<f32>,
    pub source: ScaleSource,
    /// The fixed Display scale from Settings that the app runs at instead, if any.
    pub fixed: Option<f32>,
}

/// CDXC:PlatformSupport 2026-10-01 WHY:
/// Without `Xft.dpi`, GPUI's X11 client derives its scale from the monitor's physical size (RandR millimetres), so a 1920x1080 14-inch panel on Omarchy/Hyprland came out at 1.5x and the chat, sidebar and Settings were far larger than every other app. Under XWayland Ghostex therefore picks the scale itself: `Xft.dpi` when the desktop publishes one (GNOME, KDE; GPUI and Chromium both read it), otherwise Hyprland's focused-monitor scale when `xwayland:force_zero_scaling` hands X clients native pixels (1.0 when Hyprland scales X clients itself), otherwise (since 2026-11-08, below) the desktop's XSETTINGS, otherwise 1.0, and never the physical-size guess. `GDK_SCALE` is not a source (superseding the 2026-09-30 version): it is GTK's whole-number hint and Omarchy ships it at 2 regardless of the monitor, so a 1.25 monitor drew Ghostex at 2x whenever Hyprland's answer was missing. XWayland counts as detected from the X server's `XWAYLAND` extension, an inherited `WAYLAND_DISPLAY`, or a reachable Hyprland socket, because session launchers (uwsm, systemd units) can drop any one of them. An explicit `GPUI_X11_SCALE_FACTOR` still wins.
/// SEE-ALSO: `append_platform_command_line_switches` in `cef/linux_x11.rs` gives Chromium the same scale.
pub(crate) fn pin_gpui_x11_scale_factor() {
    /*
    CDXC:PlatformSupport 2026-11-08 DECISION:
    User (issue #503, option B): COSMIC announces the scale X11 apps should draw at only through XSETTINGS (`Xft/DPI`, `Gdk/WindowScalingFactor`, `Gdk/UnscaledDPI`; its xrdb call writes only the cursor), so the picker fell back to 1.0 and the whole app, web pages included, drew at half size on a 200% desktop. XSETTINGS now comes after `Xft.dpi` and Hyprland, so those sessions keep exactly what they had, and before the 1.0 fallback, read the way GTK reads it. A fixed Display scale from Settings (`linuxDisplayScale`) wins over all of it except an explicit `GPUI_X11_SCALE_FACTOR`, and also applies on a plain X11 session. Chromium gets the same value (`pinned_scale_factor`).
    */
    if let Some(scale) = env::var(GPUI_X11_SCALE_FACTOR)
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        let _ = DETECTED_SCALE.set(DetectedScale {
            automatic: parse_scale(&scale),
            source: ScaleSource::Environment,
            fixed: None,
        });
        return;
    }
    let Some(server) = XServerScaleHints::read() else {
        return;
    };
    let hyprland = HyprlandSocket::find();
    let xwayland =
        server.xwayland || hyprland.is_some() || crate::linux_inherited_wayland_display().is_some();
    // The scale Automatic gives, and whether Ghostex has to pin it for GPUI.
    let (automatic, source, pin_automatic) = if let Some(dpi) = server.xft_dpi {
        (Some(dpi / 96.0), ScaleSource::XResources, false)
    } else if !xwayland {
        (None, ScaleSource::Gpui, false)
    } else if let Some(scale) = hyprland.and_then(|socket| socket.xwayland_scale()) {
        (Some(scale), ScaleSource::Hyprland, true)
    } else if let Some(scale) = server.xsettings_scale {
        (Some(scale), ScaleSource::XSettings, true)
    } else {
        (Some(1.0), ScaleSource::Default, true)
    };
    let fixed = fixed_display_scale();
    let _ = DETECTED_SCALE.set(DetectedScale {
        automatic,
        source,
        fixed,
    });
    let scale = match (fixed, automatic) {
        (Some(fixed), _) => fixed,
        (None, Some(automatic)) if pin_automatic => automatic,
        _ => return,
    };
    let _ = PINNED_SCALE_FACTOR.set(scale);
    // SAFETY: called from main() before GPUI starts background threads or framework-owned
    // environment readers, so no concurrent environment access is possible.
    unsafe { env::set_var(GPUI_X11_SCALE_FACTOR, scale.to_string()) };
}

/// What the scale picker found at launch, once it has run.
// The setup launcher (bin/ghostex_gpui_cef_bootstrap.rs) shares this file and reads none of it.
#[allow(dead_code)]
pub(crate) fn detected_scale() -> Option<DetectedScale> {
    DETECTED_SCALE.get().copied()
}

/// Settings > Interface > Display scale when it is fixed, read straight from the settings file
/// because this runs before anything else has loaded it. Only read: a damaged file is left for
/// the app's own settings load to repair, and counts as Automatic here.
fn fixed_display_scale() -> Option<f32> {
    let path = ghostex_paths::GhostexPaths::resolve().sidebar_settings_file();
    let bytes = std::fs::read(path).ok()?;
    // A byte order mark, as the settings reader allows (packages/paths/src/settings_file.rs).
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    let settings: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(bytes).ok()?;
    ghostex_settings_catalog::interface_scale::linux_display_scale_override(&settings)
        .map(|scale| scale as f32)
}

/// The scale `pin_gpui_x11_scale_factor` chose, when it chose one.
pub(crate) fn pinned_scale_factor() -> Option<f32> {
    PINNED_SCALE_FACTOR.get().copied()
}

fn parse_scale(value: &str) -> Option<f32> {
    value
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|scale| scale.is_finite() && *scale > 0.0)
}

struct XServerScaleHints {
    xwayland: bool,
    /// `Xft.dpi` from the root window's `RESOURCE_MANAGER` string (what `xrdb` loads), the value
    /// GPUI's X11 client reads first.
    xft_dpi: Option<f32>,
    /// The scale the screen's XSettings manager announces, read the way GTK reads it.
    xsettings_scale: Option<f32>,
}

impl XServerScaleHints {
    fn read() -> Option<Self> {
        use x11rb::connection::{Connection as _, RequestConnection as _};
        use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _};

        let (connection, screen_index) = x11rb::connect(None).ok()?;
        let root = connection.setup().roots.get(screen_index)?.root;
        let xwayland = connection
            .extension_information("XWAYLAND")
            .ok()
            .flatten()
            .is_some();
        let xft_dpi = connection
            .get_property(
                false,
                root,
                AtomEnum::RESOURCE_MANAGER,
                AtomEnum::STRING,
                0,
                u32::MAX / 4,
            )
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .and_then(|reply| {
                String::from_utf8_lossy(&reply.value)
                    .lines()
                    .find_map(|line| {
                        let (key, value) = line.split_once(':')?;
                        (key.trim() == "Xft.dpi").then(|| value.trim().to_string())
                    })
                    .filter(|value| !value.is_empty())
            })
            .map(|value| parse_scale(&value).unwrap_or(96.0));
        let xsettings_scale = read_xsettings(&connection, screen_index)
            .as_deref()
            .and_then(scale_from_xsettings);
        Some(Self {
            xwayland,
            xft_dpi,
            xsettings_scale,
        })
    }
}

/// The scale an XSettings blob announces, read as GTK reads it: the window scale
/// (`Gdk/WindowScalingFactor`, 1 when missing) times the font DPI GTK uses (`Gdk/UnscaledDPI`,
/// else `Xft/DPI`) over 96. COSMIC at 200% sends 2 and 96 dpi (2.0); at 150%, 1 and 144 dpi (1.5).
/// `None` when it names neither DPI, or a value out of range.
fn scale_from_xsettings(settings: &[(String, i32)]) -> Option<f32> {
    let int = |name: &str| {
        settings
            .iter()
            .find(|(setting, _)| setting == name)
            .map(|(_, value)| *value)
    };
    let window_scale = int("Gdk/WindowScalingFactor")
        .filter(|scale| *scale >= 1)
        .unwrap_or(1);
    // Both DPI settings are in 1024ths of a dot per inch.
    let dpi = int("Gdk/UnscaledDPI")
        .or_else(|| int("Xft/DPI"))
        .map(|dpi| dpi as f32 / 1024.0)
        .filter(|dpi| (50.0..=500.0).contains(dpi))?;
    let scale = window_scale as f32 * dpi / 96.0;
    (scale.is_finite() && (0.5..=4.0).contains(&scale)).then_some(scale)
}

/// The integer settings the screen's XSettings manager publishes, read from the
/// `_XSETTINGS_SETTINGS` property of the `_XSETTINGS_S<screen>` selection owner (freedesktop
/// XSettings specification). The same reader as GPUI's (`gpui_linux/src/fast/x11_scale.rs`),
/// which GPUI keeps private.
fn read_xsettings(
    connection: &impl x11rb::connection::Connection,
    screen_index: usize,
) -> Option<Vec<(String, i32)>> {
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _};

    let selection = connection
        .intern_atom(true, format!("_XSETTINGS_S{screen_index}").as_bytes())
        .ok()?
        .reply()
        .ok()?
        .atom;
    let property = connection
        .intern_atom(true, b"_XSETTINGS_SETTINGS")
        .ok()?
        .reply()
        .ok()?
        .atom;
    if selection == x11rb::NONE || property == x11rb::NONE {
        return None;
    }
    let owner = connection
        .get_selection_owner(selection)
        .ok()?
        .reply()
        .ok()?
        .owner;
    if owner == x11rb::NONE {
        return None;
    }
    let reply = connection
        .get_property(false, owner, property, AtomEnum::ANY, 0, u32::MAX / 4)
        .ok()?
        .reply()
        .ok()?;
    if reply.format != 8 {
        return None;
    }
    parse_xsettings_ints(&reply.value)
}

/// Parses an XSettings blob, keeping the integer settings. Strings and colours are skipped; a
/// truncated blob yields `None`.
fn parse_xsettings_ints(data: &[u8]) -> Option<Vec<(String, i32)>> {
    let big_endian = match *data.first()? {
        0 => false,
        1 => true,
        _ => return None,
    };
    let mut cursor = 0usize;
    let mut take = |len: usize| -> Option<&[u8]> {
        let bytes = data.get(cursor..cursor.checked_add(len)?)?;
        cursor += len;
        Some(bytes)
    };
    let u16_of = |bytes: &[u8]| {
        let bytes = [bytes[0], bytes[1]];
        if big_endian {
            u16::from_be_bytes(bytes)
        } else {
            u16::from_le_bytes(bytes)
        }
    };
    let u32_of = |bytes: &[u8]| {
        let bytes = [bytes[0], bytes[1], bytes[2], bytes[3]];
        if big_endian {
            u32::from_be_bytes(bytes)
        } else {
            u32::from_le_bytes(bytes)
        }
    };
    let padded = |len: usize| len.div_ceil(4) * 4;

    take(4)?; // byte order and padding
    take(4)?; // serial
    let count = u32_of(take(4)?);
    let mut settings = Vec::new();
    for _ in 0..count {
        let header = take(4)?;
        let kind = header[0];
        let name_len = u16_of(&header[2..4]) as usize;
        let name = take(padded(name_len))?.get(..name_len)?;
        let name = String::from_utf8_lossy(name).into_owned();
        take(4)?; // last-change serial
        match kind {
            0 => settings.push((name, u32_of(take(4)?) as i32)),
            1 => {
                let len = u32_of(take(4)?) as usize;
                take(padded(len))?;
            }
            2 => {
                take(8)?;
            }
            _ => return None,
        }
    }
    Some(settings)
}

/// Hyprland's request socket (`$XDG_RUNTIME_DIR/hypr/<instance>/.socket.sock`, what `hyprctl`
/// talks to).
struct HyprlandSocket(PathBuf);

impl HyprlandSocket {
    /// The instance named by `HYPRLAND_INSTANCE_SIGNATURE`, or, when the launcher dropped that
    /// variable, the newest instance under the runtime directory whose socket answers.
    fn find() -> Option<Self> {
        let hypr_dir = runtime_dir()?.join("hypr");
        if let Some(signature) = env::var("HYPRLAND_INSTANCE_SIGNATURE")
            .ok()
            .filter(|value| !value.trim().is_empty())
        {
            let socket = Self(hypr_dir.join(signature.trim()).join(".socket.sock"));
            if socket.path().exists() {
                return Some(socket);
            }
        }
        let mut instances = std::fs::read_dir(&hypr_dir)
            .ok()?
            .filter_map(Result::ok)
            .map(|entry| entry.path().join(".socket.sock"))
            .filter(|socket| socket.exists())
            .filter_map(|socket| {
                let modified = std::fs::metadata(&socket).and_then(|m| m.modified()).ok()?;
                Some((modified, socket))
            })
            .collect::<Vec<_>>();
        instances.sort_by(|left, right| right.0.cmp(&left.0));
        instances
            .into_iter()
            .map(|(_, socket)| Self(socket))
            .find(|socket| socket.request("version").is_some())
    }

    fn path(&self) -> &Path {
        &self.0
    }

    /// The focused monitor's scale when Hyprland shows X clients at native pixels
    /// (`xwayland:force_zero_scaling`), 1.0 when Hyprland scales X clients itself, and nothing
    /// when Hyprland does not answer. Uses the plain `hyprctl` text format (`bool: true` /
    /// `int: 1`, and `Monitor … scale: 1.25 … focused: yes` blocks).
    fn xwayland_scale(&self) -> Option<f32> {
        let option = self.request("getoption xwayland:force_zero_scaling")?;
        let force_zero_scaling = option.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            match key.trim() {
                "bool" => Some(value.trim() == "true"),
                "int" => value.trim().parse::<i64>().ok().map(|value| value != 0),
                _ => None,
            }
        })?;
        if !force_zero_scaling {
            return Some(1.0);
        }
        let monitors = self.request("monitors")?;
        let mut blocks: Vec<(Option<f32>, bool)> = Vec::new();
        for line in monitors.lines() {
            if line.starts_with("Monitor ") {
                blocks.push((None, false));
                continue;
            }
            let Some(block) = blocks.last_mut() else {
                continue;
            };
            let Some((key, value)) = line.trim().split_once(':') else {
                continue;
            };
            match key.trim() {
                "scale" => block.0 = parse_scale(value),
                "focused" => block.1 = value.trim() == "yes",
                _ => {}
            }
        }
        blocks
            .iter()
            .find(|(_, focused)| *focused)
            .and_then(|(scale, _)| *scale)
            .or_else(|| blocks.iter().find_map(|(scale, _)| *scale))
    }

    fn request(&self, request: &str) -> Option<String> {
        use std::io::{Read as _, Write as _};
        use std::time::Duration;

        let mut stream = std::os::unix::net::UnixStream::connect(self.path()).ok()?;
        let timeout = Some(Duration::from_millis(500));
        stream.set_read_timeout(timeout).ok()?;
        stream.set_write_timeout(timeout).ok()?;
        stream.write_all(request.as_bytes()).ok()?;
        let mut response = Vec::new();
        stream.read_to_end(&mut response).ok()?;
        let response = String::from_utf8(response).ok()?;
        (!response.trim().is_empty()).then_some(response)
    }
}

/// `XDG_RUNTIME_DIR`, or `/run/user/<uid>` when the launcher did not pass it on.
fn runtime_dir() -> Option<PathBuf> {
    if let Some(dir) = env::var_os("XDG_RUNTIME_DIR").filter(|dir| !dir.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    use std::os::unix::fs::MetadataExt as _;
    let uid = std::fs::metadata("/proc/self").ok()?.uid();
    Some(PathBuf::from(format!("/run/user/{uid}")))
}
