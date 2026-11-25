//! The icon an agent draws for its app: a square SVG or PNG file beside the app's HTML, named by
//! `<link rel="icon">`. `ghostex app show` checks it before it publishes, and gxserver draws it
//! into the PNG it keeps.
//!
//! CDXC:GhostexApps 2026-11-25 DECISION:
//! User picked "the agent draws it" for an installed app's icon (Q4975), not an icon Ghostex makes up and not one the user picks. So an app must carry its own icon: `ghostex app show` refuses an app without a usable one and says how to add it, instead of falling back to a made-up icon.
//!
//! CDXC:GhostexApps 2026-11-26 WHY:
//! Fable's security review (finding 6) beat the first check, a list of things an SVG must not contain (`<svg/onload=…>`, `java&#115;cript:`, CSS `url(//host)`, `<feImage href="file:///…">`, an outside DOCTYPE, a PNG header claiming 60000 px). So the agent's file is never kept or served: gxserver draws it once into a 1024 px PNG and keeps only that. An SVG is drawn by resvg with no fonts, no outside files and no linked or embedded images (every picture `href` resolves to nothing), so nothing in it can run or reach anything; a PNG is decoded whole under a size cap and written out again. The Dock, the taskbar, `.icns` and Settings all start from that PNG.
//!
//! CDXC:GhostexApps 2026-11-26 WHY:
//! Fable's re-review of 970a63ba9: drawing the agent's SVG inside gxserver let a 7 KB file of 1000 nested `<g>` overflow its stack and abort it, and a 188 byte `feMorphology`, a 51 KB stack of blurs, nested opacity groups or a `<use>` tree keep a core busy for minutes or grow memory without limit. Now `guard_svg` refuses, before resvg sees the file, nesting deeper than 64, filters, masks, patterns and markers, a DOCTYPE, a `<use>` that copies a `<use>`, and too many elements or see-through groups; the drawing itself runs in a short-lived child of the same program (`gxserver __draw-ghostex-app-icon`), killed after 2 seconds, one drawing at a time, and publishes are limited per session (publish.rs). A crash or a stall costs only the child.

use std::{
    collections::HashMap,
    io::Cursor,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// The two kinds of icon an agent may draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IconKind {
    Svg,
    Png,
}

impl IconKind {
    pub(crate) fn extension(self) -> &'static str {
        match self {
            IconKind::Svg => "svg",
            IconKind::Png => "png",
        }
    }

    pub(crate) fn from_extension(extension: &str) -> Option<IconKind> {
        match extension.to_ascii_lowercase().as_str() {
            "svg" => Some(IconKind::Svg),
            "png" => Some(IconKind::Png),
            _ => None,
        }
    }
}

pub(crate) const SVG_ICON_MAX_BYTES: usize = 64 * 1024;
pub(crate) const PNG_ICON_MAX_BYTES: usize = 512 * 1024;
/// A PNG icon must be at least this wide (and as tall), so the Dock's large sizes stay sharp.
pub(crate) const PNG_ICON_MIN_SIDE: u32 = 256;
/// And at most this wide, so decoding it stays small.
pub(crate) const PNG_ICON_MAX_SIDE: u32 = 2048;
/// The side of the PNG gxserver keeps (the largest `.icns` size, 512 pt at 2x).
pub(crate) const ICON_SIDE: u32 = 1024;
/// How far from square a drawing may be (its width against its height).
const SQUARE_TOLERANCE: f32 = 0.01;

/// What an agent is told when its app has no usable icon.
pub(crate) const ICON_HOW_TO: &str = "Draw the app's icon as a square SVG file (a viewBox like \"0 0 512 512\") beside the HTML and name it in the head: <link rel=\"icon\" href=\"icon.svg\">. A square PNG of 256 to 2048 px works too. See `ghostex app guide`.";

/// Checks an icon, and gives the reason when it cannot be used.
pub(crate) fn check_icon(kind: IconKind, bytes: &[u8]) -> Result<(), String> {
    icon_png(kind, bytes).map(|_| ())
}

/// The icon drawn into the [`ICON_SIDE`] px PNG gxserver keeps. An SVG is drawn in a short-lived
/// child process ([`DRAW_ICON_COMMAND`]) under a deadline, one at a time.
pub(crate) fn icon_png(kind: IconKind, bytes: &[u8]) -> Result<Vec<u8>, String> {
    match kind {
        IconKind::Svg => {
            guard_svg(bytes)?;
            draw_in_child(bytes)
        }
        IconKind::Png => png_png(bytes),
    }
}

/// The hidden verb gxserver runs itself with to draw one SVG icon: the SVG on stdin, the PNG on
/// stdout (exit 0) or the reason it was refused (exit [`REFUSED_EXIT`]).
pub const DRAW_ICON_COMMAND: &str = "__draw-ghostex-app-icon";
const REFUSED_EXIT: i32 = 3;
/// How long one drawing may take before the child is killed.
const DRAW_DEADLINE: Duration = Duration::from_secs(2);
/// How long a publish waits for another drawing to finish.
const DRAW_QUEUE_WAIT: Duration = Duration::from_secs(5);
/// More than a 1024 px PNG of noise can be.
const DRAW_OUTPUT_MAX_BYTES: u64 = 16 * 1024 * 1024;

/// How deep elements may nest in an SVG icon.
pub(crate) const SVG_MAX_DEPTH: usize = 64;
/// How many elements an SVG icon may draw, counting each `<use>` as the elements it copies.
const SVG_MAX_ELEMENTS: usize = 10_000;
/// How many `<use>` elements an SVG icon may have.
const SVG_MAX_USES: usize = 64;
/// How many elements may need a layer of their own (opacity, clip path, blending), counting copies.
const SVG_MAX_LAYERS: usize = 64;
/// Elements an icon may not have: each can make one small file draw for minutes (filters,
/// `feMorphology`, blurs), take memory without limit, or repeat its content without limit.
const SVG_REFUSED_ELEMENTS: &[&str] = &["filter", "mask", "pattern", "marker"];

/// Refuses, before resvg sees it, an SVG whose drawing could crash or stall: deep nesting (resvg
/// recurses, and 1000 nested `<g>` overflowed gxserver's stack), filters, masks, patterns and
/// markers, `<use>` copying a `<use>`, and too many elements or layers. A DOCTYPE is refused too,
/// so the text checked is the text drawn (no entities).
pub(crate) fn guard_svg(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > SVG_ICON_MAX_BYTES {
        return Err(format!(
            "The icon is {} KiB; an SVG icon may be at most {} KiB.",
            bytes.len() / 1024,
            SVG_ICON_MAX_BYTES / 1024
        ));
    }
    let text =
        std::str::from_utf8(bytes).map_err(|_| "The SVG icon is not UTF-8 text.".to_string())?;
    let document = roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: 4 * SVG_MAX_ELEMENTS as u32,
            ..Default::default()
        },
    )
    .map_err(|error| format!("The SVG icon could not be read: {error}."))?;
    let nodes: Vec<roxmltree::Node> = document.descendants().collect();
    let index = |node: roxmltree::Node| node.id().get_usize();
    let size = nodes.iter().map(|node| index(*node)).max().unwrap_or(0) + 1;
    let mut depth = vec![0usize; size];
    let mut by_id = HashMap::new();
    for node in nodes.iter().filter(|node| node.is_element()) {
        let here = node.parent().map_or(0, |parent| depth[index(parent)]) + 1;
        if here > SVG_MAX_DEPTH {
            return Err(format!(
                "The icon nests elements more than {SVG_MAX_DEPTH} deep; draw it flatter."
            ));
        }
        depth[index(*node)] = here;
        let name = node.tag_name().name();
        if SVG_REFUSED_ELEMENTS
            .iter()
            .any(|refused| name.eq_ignore_ascii_case(refused))
            || name
                .get(..2)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("fe"))
        {
            return Err(format!(
                "The icon uses <{name}>; an icon may not use filters, masks, patterns or markers."
            ));
        }
        if uses_filter(*node) {
            return Err("The icon uses a filter; an icon may not use filters.".to_string());
        }
        if let Some(id) = node.attribute("id") {
            by_id.insert(id, index(*node));
        }
    }
    // Per element, bottom up (children come after their parent in document order): the elements
    // and layers its subtree draws, and whether it holds a `<use>`.
    let mut elements = vec![0usize; size];
    let mut layers = vec![0usize; size];
    let mut holds_use = vec![false; size];
    for node in nodes.iter().rev().filter(|node| node.is_element()) {
        let here = index(*node);
        elements[here] += 1;
        layers[here] += usize::from(is_layer(*node));
        holds_use[here] |= node.tag_name().name() == "use";
        if let Some(parent) = node.parent().filter(roxmltree::Node::is_element) {
            let parent = index(parent);
            elements[parent] = elements[parent].saturating_add(elements[here]);
            layers[parent] = layers[parent].saturating_add(layers[here]);
            holds_use[parent] |= holds_use[here];
        }
    }
    let (mut total, mut total_layers, mut uses) = (0usize, 0usize, 0usize);
    for node in nodes.iter().filter(|node| node.is_element()) {
        total += 1;
        total_layers += usize::from(is_layer(*node));
        // A clip path is drawn again for each element that uses it.
        let clip = node
            .attribute("clip-path")
            .and_then(|value| value.trim().strip_prefix("url(#"))
            .and_then(|rest| rest.split(')').next())
            .and_then(|id| by_id.get(id.trim()).copied());
        if let Some(clip) = clip {
            total = total.saturating_add(elements[clip]);
        }
        if node.tag_name().name() != "use" {
            continue;
        }
        uses += 1;
        let target = node
            .attributes()
            .find(|attribute| attribute.name() == "href")
            .and_then(|attribute| attribute.value().trim().strip_prefix('#'))
            .and_then(|id| by_id.get(id).copied());
        if let Some(target) = target {
            if holds_use[target] {
                return Err(
                    "The icon has a <use> that copies another <use>; copy shapes only once."
                        .to_string(),
                );
            }
            total = total.saturating_add(elements[target]);
            total_layers = total_layers.saturating_add(layers[target]);
        }
    }
    if uses > SVG_MAX_USES {
        return Err(format!(
            "The icon has {uses} <use> elements; an icon may have at most {SVG_MAX_USES}."
        ));
    }
    if total > SVG_MAX_ELEMENTS {
        return Err(format!(
            "The icon draws {total} elements; an icon may draw at most {SVG_MAX_ELEMENTS}."
        ));
    }
    if total_layers > SVG_MAX_LAYERS {
        return Err(format!(
            "The icon has {total_layers} see-through or clipped groups; an icon may have at most {SVG_MAX_LAYERS}."
        ));
    }
    Ok(())
}

/// Whether `node` asks for a filter: the attribute, or `filter` in its style or style sheet
/// (CSS filter functions such as `blur()` need no `<filter>` element).
fn uses_filter(node: roxmltree::Node) -> bool {
    let mentions = |text: &str| text.to_ascii_lowercase().contains("filter");
    node.attributes().any(|attribute| {
        attribute.name().eq_ignore_ascii_case("filter")
            || (attribute.name() == "style" && mentions(attribute.value()))
    }) || (node.tag_name().name() == "style"
        && node
            .children()
            .filter_map(|child| child.text())
            .any(mentions))
}

/// Whether drawing `node` needs a layer of its own.
fn is_layer(node: roxmltree::Node) -> bool {
    node.attributes().any(|attribute| {
        let name = attribute.name();
        matches!(
            name,
            "opacity" | "clip-path" | "mix-blend-mode" | "isolation"
        ) || (name == "style" && {
            let style = attribute.value().to_ascii_lowercase();
            style.contains("opacity")
                || style.contains("clip-path")
                || style.contains("mix-blend")
                || style.contains("isolation")
        })
    })
}

/// Draws an SVG icon in this process: what the child of [`DRAW_ICON_COMMAND`] runs. gxserver
/// itself never calls this; a crash or a stall here costs only the child.
pub(crate) fn draw_svg(bytes: &[u8]) -> Result<Vec<u8>, String> {
    guard_svg(bytes)?;
    let text =
        std::str::from_utf8(bytes).map_err(|_| "The SVG icon is not UTF-8 text.".to_string())?;
    let options = resvg::usvg::Options {
        resources_dir: None,
        // No fonts: text in an icon is not drawn (the guide says no words).
        fontdb: Arc::new(resvg::usvg::fontdb::Database::new()),
        image_href_resolver: resvg::usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        style_sheet: None,
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_str(text, &options)
        .map_err(|error| format!("The SVG icon could not be read: {error}."))?;
    let size = tree.size();
    let (width, height) = (size.width(), size.height());
    if (width - height).abs() > width.max(height) * SQUARE_TOLERANCE {
        return Err(format!(
            "The icon is {width}x{height}; it must be square (a viewBox like \"0 0 512 512\")."
        ));
    }
    let mut pixmap = resvg::tiny_skia::Pixmap::new(ICON_SIDE, ICON_SIDE)
        .ok_or_else(|| "The icon could not be drawn.".to_string())?;
    let scale = ICON_SIDE as f32 / width.max(height);
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    if pixmap.pixels().iter().all(|pixel| pixel.alpha() == 0) {
        return Err(
            "The icon draws nothing: only shapes, paths and gradients are drawn, no text or images."
                .to_string(),
        );
    }
    pixmap
        .encode_png()
        .map_err(|error| format!("The icon could not be written: {error}."))
}

/// `gxserver __draw-ghostex-app-icon`: draws the SVG on stdin. Called from `main` before anything
/// else starts. Its own limits back up the parent's deadline: a CPU time limit, and on Linux an
/// address space limit (macOS does not enforce one).
pub fn run_draw_icon_child() -> i32 {
    use std::io::{Read, Write};
    limit_this_process();
    let mut bytes = Vec::new();
    if std::io::stdin()
        .take(SVG_ICON_MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return 1;
    }
    // A panic in resvg (a blur of 1e9 did) ends this child, never gxserver.
    let (code, out) = match std::panic::catch_unwind(|| draw_svg(&bytes)) {
        Ok(Ok(png)) => (0, png),
        Ok(Err(reason)) => (REFUSED_EXIT, reason.into_bytes()),
        Err(_) => (1, Vec::new()),
    };
    let mut stdout = std::io::stdout().lock();
    if stdout
        .write_all(&out)
        .and_then(|()| stdout.flush())
        .is_err()
    {
        return 1;
    }
    code
}

#[cfg(unix)]
fn limit_this_process() {
    let set = |resource, soft: u64, hard: u64| {
        let limit = libc::rlimit {
            rlim_cur: soft as libc::rlim_t,
            rlim_max: hard as libc::rlim_t,
        };
        // SAFETY: setrlimit only reads `limit`, and only lowers this child's own limits.
        unsafe { libc::setrlimit(resource, &limit) };
    };
    let cpu = DRAW_DEADLINE.as_secs() + 2;
    set(libc::RLIMIT_CPU, cpu, cpu + 1);
    #[cfg(target_os = "linux")]
    set(libc::RLIMIT_AS, 1 << 30, 1 << 30);
    // SAFETY: nice only changes this process's own priority.
    unsafe { libc::nice(10) };
}

#[cfg(not(unix))]
fn limit_this_process() {}

/// One drawing at a time, so parallel publishes cannot take every core.
static DRAWING: Mutex<()> = Mutex::new(());

/// Draws `bytes` in a child process, killed if it runs past [`DRAW_DEADLINE`].
fn draw_in_child(bytes: &[u8]) -> Result<Vec<u8>, String> {
    use std::{io::Read, process::Stdio};
    let busy = Instant::now() + DRAW_QUEUE_WAIT;
    let _turn = loop {
        match DRAWING.try_lock() {
            Ok(guard) => break guard,
            Err(std::sync::TryLockError::Poisoned(poisoned)) => break poisoned.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) if Instant::now() < busy => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(std::sync::TryLockError::WouldBlock) => {
                return Err(
                    "Ghostex is busy drawing other icons; try again in a moment.".to_string(),
                )
            }
        }
    };
    let cannot = |error: std::io::Error| format!("The icon could not be drawn: {error}.");
    let program = std::env::current_exe().map_err(cannot)?;
    let mut child = crate::platform::process::background_command(program)
        .arg(DRAW_ICON_COMMAND)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(cannot)?;
    let input = bytes.to_vec();
    let mut stdin = child.stdin.take();
    let writer = std::thread::spawn(move || {
        use std::io::Write;
        if let Some(stdin) = stdin.as_mut() {
            let _ = stdin.write_all(&input);
        }
    });
    let mut stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        if let Some(stdout) = stdout.as_mut() {
            let _ = stdout.take(DRAW_OUTPUT_MAX_BYTES).read_to_end(&mut out);
        }
        out
    });
    let deadline = Instant::now() + DRAW_DEADLINE;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let _ = writer.join();
    let out = reader.join().unwrap_or_default();
    match status {
        None => Err(format!(
            "The icon took more than {} seconds to draw; draw it with fewer, simpler shapes.",
            DRAW_DEADLINE.as_secs()
        )),
        Some(status) if status.success() && out.starts_with(b"\x89PNG\r\n\x1a\n") => Ok(out),
        Some(status) if status.code() == Some(REFUSED_EXIT) => {
            Err(String::from_utf8_lossy(&out).chars().take(300).collect())
        }
        Some(_) => {
            Err("The icon could not be drawn; draw it with plain shapes and paths.".to_string())
        }
    }
}

fn png_png(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.len() > PNG_ICON_MAX_BYTES {
        return Err(format!(
            "The icon is {} KiB; a PNG icon may be at most {} KiB.",
            bytes.len() / 1024,
            PNG_ICON_MAX_BYTES / 1024
        ));
    }
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(PNG_ICON_MAX_SIDE);
    limits.max_image_height = Some(PNG_ICON_MAX_SIDE);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode().map_err(|_| {
        format!("The icon is not a PNG that can be read (at most {PNG_ICON_MAX_SIDE} px a side).")
    })?;
    let (width, height) = (decoded.width(), decoded.height());
    if width != height || width < PNG_ICON_MIN_SIDE {
        return Err(format!(
            "The icon is {width}x{height}; a PNG icon must be square and at least {PNG_ICON_MIN_SIDE} px."
        ));
    }
    let resized = image::imageops::resize(
        &decoded.to_rgba8(),
        ICON_SIDE,
        ICON_SIDE,
        image::imageops::FilterType::Lanczos3,
    );
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(resized)
        .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
        .map_err(|error| format!("The icon could not be written: {error}."))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512"><rect width="512" height="512" rx="118" fill="#f4b24c"/></svg>"##;

    fn is_png(bytes: &[u8]) -> bool {
        bytes.starts_with(b"\x89PNG\r\n\x1a\n")
    }

    #[test]
    fn draws_a_plain_square_svg_into_a_png() {
        let png = draw_svg(PLAIN).unwrap();
        assert!(is_png(&png));
        let image = image::load_from_memory(&png).unwrap();
        assert_eq!((image.width(), image.height()), (ICON_SIDE, ICON_SIDE));
    }

    #[test]
    fn what_slipped_past_the_old_check_comes_out_as_a_plain_png_or_is_refused() {
        // Each of these got through the old deny-list (finding 6). Now what is kept is a PNG
        // gxserver drew, whatever the SVG held, so nothing of the SVG is ever served.
        for svg in [
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1"/onload="alert(1)"><rect width="1" height="1"/></svg>"##,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><a href="java&#115;cript:alert(1)"><rect width="8" height="8"/></a></svg>"##,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><style>rect{fill:url("//example.com/x")}</style><rect width="8" height="8" fill="red"/></svg>"##,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><filter id="f"><feImage href="file:///etc/hosts"/></filter><rect width="8" height="8" fill="red" filter="url(#f)"/></svg>"##,
            r##"<!DOCTYPE svg SYSTEM "http://example.com/x.dtd"><svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><rect width="8" height="8"/></svg>"##,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><image href="file:///etc/hosts" width="8" height="8"/><rect width="8" height="8"/></svg>"##,
        ] {
            if let Ok(png) = draw_svg(svg.as_bytes()) {
                assert!(is_png(&png), "{svg}");
            }
        }
    }

    #[test]
    fn refuses_odd_shapes_empty_drawings_and_big_files() {
        let wide = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 50"><rect width="100" height="50"/></svg>"#;
        assert!(draw_svg(wide).is_err());
        let text_only = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><text y="8">Hi</text></svg>"#;
        assert!(draw_svg(text_only).is_err());
        let image_only = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><image href="file:///etc/hosts" width="8" height="8"/></svg>"#;
        assert!(draw_svg(image_only).is_err());
        assert!(draw_svg(b"not svg").is_err());
        let big = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 1 1\"><!--{}--></svg>",
            "x".repeat(SVG_ICON_MAX_BYTES)
        );
        assert!(draw_svg(big.as_bytes()).is_err());
    }

    /// The review's hostile icons (recheck of 970a63ba9): each crashed gxserver, stalled it for
    /// minutes or grew its memory without limit, and must now be refused before resvg sees it.
    pub(super) fn hostile_icons() -> Vec<(&'static str, String)> {
        const OPEN: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 64 64">"#;
        let nested = |depth: usize, open: &str, close: &str| {
            format!(
                "{OPEN}{}<rect width=\"64\" height=\"64\"/>{}</svg>",
                open.repeat(depth),
                close.repeat(depth)
            )
        };
        let blurs = |count: usize| {
            format!(
                "{OPEN}<filter id=\"f\">{}</filter><rect width=\"64\" height=\"64\" filter=\"url(#f)\"/></svg>",
                "<feGaussianBlur stdDeviation=\"20\"/>".repeat(count)
            )
        };
        // Each level copies the one below ten times: 10^5 rects from 1 KB.
        let mut uses = format!("{OPEN}<defs><rect id=\"u0\" width=\"1\" height=\"1\"/>");
        for level in 1..=5 {
            uses.push_str(&format!("<g id=\"u{level}\">"));
            for _ in 0..10 {
                uses.push_str(&format!("<use href=\"#u{}\"/>", level - 1));
            }
            uses.push_str("</g>");
        }
        uses.push_str("</defs><use href=\"#u5\"/></svg>");
        vec![
            ("1000 nested <g>", nested(1000, "<g>", "</g>")),
            (
                "feMorphology",
                format!("{OPEN}<filter id=\"m\"><feMorphology radius=\"4\"/></filter><rect width=\"64\" height=\"64\" filter=\"url(#m)\"/></svg>"),
            ),
            ("1500 blurs", blurs(1500)),
            ("100 blurs", blurs(100)),
            ("200 nested opacity groups", nested(200, "<g opacity=\"0.99\">", "</g>")),
            ("use tree of 10^5", uses),
            (
                "CSS blur",
                format!("{OPEN}<rect width=\"64\" height=\"64\" style=\"filter:blur(1e9)\"/></svg>"),
            ),
            (
                "turbulence",
                format!("{OPEN}<filter id=\"t\"><feTurbulence numOctaves=\"1000000\"/></filter><rect width=\"64\" height=\"64\" filter=\"url(#t)\"/></svg>"),
            ),
            (
                "mask",
                format!("{OPEN}<mask id=\"k\"><rect width=\"64\" height=\"64\" fill=\"white\"/></mask><rect width=\"64\" height=\"64\" mask=\"url(#k)\"/></svg>"),
            ),
            (
                "pattern",
                format!("{OPEN}<pattern id=\"p\" width=\"0.001\" height=\"0.001\"><rect width=\"1\" height=\"1\"/></pattern><rect width=\"64\" height=\"64\" fill=\"url(#p)\"/></svg>"),
            ),
            (
                "entity tree",
                format!("<!DOCTYPE svg [<!ENTITY a \"<rect width='1' height='1'/>\"><!ENTITY b \"&a;&a;&a;&a;&a;&a;&a;&a;\">]>{OPEN}&b;</svg>"),
            ),
            (
                "many opacity groups",
                format!("{OPEN}{}</svg>", "<g opacity=\"0.5\"><rect width=\"64\" height=\"64\"/></g>".repeat(500)),
            ),
        ]
    }

    #[test]
    fn refuses_the_reviews_hostile_icons_before_drawing() {
        for (what, svg) in hostile_icons() {
            assert!(
                svg.len() <= SVG_ICON_MAX_BYTES,
                "{what} is {} bytes",
                svg.len()
            );
            let started = std::time::Instant::now();
            assert!(
                guard_svg(svg.as_bytes()).is_err(),
                "{what} passed the guard"
            );
            assert!(draw_svg(svg.as_bytes()).is_err(), "{what} was drawn");
            assert!(
                started.elapsed() < Duration::from_secs(1),
                "{what} took {:?}",
                started.elapsed()
            );
        }
    }

    #[test]
    fn an_ordinary_icon_passes_the_guard() {
        let icon = br##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 512 512"><defs><linearGradient id="g"><stop offset="0" stop-color="#f4b24c"/><stop offset="1" stop-color="#e0662b"/></linearGradient><clipPath id="c"><rect width="512" height="512" rx="118"/></clipPath><circle id="dot" r="20" fill="#fff"/></defs><g clip-path="url(#c)"><rect width="512" height="512" fill="url(#g)"/><g opacity="0.8"><use xlink:href="#dot" x="200" y="256"/><use href="#dot" x="312" y="256"/></g></g></svg>"##;
        guard_svg(icon).unwrap();
        assert!(is_png(&draw_svg(icon).unwrap()));
        let deep_enough = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 8 8\">{}<rect width=\"8\" height=\"8\"/>{}</svg>",
            "<g>".repeat(SVG_MAX_DEPTH - 2),
            "</g>".repeat(SVG_MAX_DEPTH - 2)
        );
        guard_svg(deep_enough.as_bytes()).unwrap();
        let too_deep = deep_enough
            .replacen("<g>", "<g><g>", 1)
            .replacen("</g>", "</g></g>", 1);
        assert!(guard_svg(too_deep.as_bytes()).is_err());
    }

    #[test]
    fn a_use_may_not_copy_a_use() {
        let chain = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><rect id="a" width="8" height="8"/><g id="b"><use href="#a"/></g><use href="#b"/></svg>"##;
        assert!(guard_svg(chain).is_err());
    }

    fn png_of(width: u32, height: u32) -> Vec<u8> {
        let mut out = Vec::new();
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            width,
            height,
            image::Rgba([10, 20, 30, 255]),
        ))
        .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
        out
    }

    #[test]
    fn decodes_pngs_whole_and_checks_their_size() {
        let png = icon_png(IconKind::Png, &png_of(256, 256)).unwrap();
        assert_eq!(image::load_from_memory(&png).unwrap().width(), ICON_SIDE);
        assert!(icon_png(IconKind::Png, &png_of(128, 128)).is_err());
        assert!(icon_png(IconKind::Png, &png_of(300, 256)).is_err());
        // A header claiming 60000 px followed by junk (the review's case).
        let mut lying = png_of(256, 256)[..33].to_vec();
        lying[16..20].copy_from_slice(&60000u32.to_be_bytes());
        lying[20..24].copy_from_slice(&60000u32.to_be_bytes());
        lying.extend_from_slice(&[0u8; 64]);
        assert!(icon_png(IconKind::Png, &lying).is_err());
        let mut truncated = png_of(256, 256);
        truncated.truncate(truncated.len() / 2);
        assert!(icon_png(IconKind::Png, &truncated).is_err());
    }
}
