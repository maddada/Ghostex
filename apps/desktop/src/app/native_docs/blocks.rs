//! Rendered blocks in the live Markdown editor: ```mermaid diagrams (mermaid-rs-renderer to SVG,
//! then resvg), `$$…$$` / `$…$` math (RaTeX), images, and code colours (gpui-component's
//! tree-sitter highlighter). The editor asks for these while it paints, so every slow render runs
//! in the background and lands in a shared cache; the window repaints when it arrives.
//!
//! Adapted from the Docs prototype (`docs/2026-09-24/docs-gpui-editor-research/demo/src/blocks.rs`).

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    ops::Range,
    rc::Rc,
    sync::{Arc, OnceLock},
};

use gpui::{App, AppContext as _, Entity, HighlightStyle, Hsla, RenderImage};
use image::{Frame, RgbaImage};
use zorite_editor::EditorState;

/// The em the math rasters are typeset at; inline formulas scale from it to the text size.
pub(crate) const MATH_EM: f32 = 20.0;
const RASTER_DPR: f32 = 2.0;
/// Diagrams and SVG images are scaled down to fit the 800px text column.
const MAX_DIAGRAM_WIDTH: f32 = 760.0;

pub(crate) type Rendered = (Arc<RenderImage>, f32, f32);

#[derive(Default)]
pub(crate) struct BlockCache {
    mermaid: HashMap<(String, bool), Option<Rendered>>,
    math: HashMap<(String, u32), Option<Rendered>>,
    /// Images by Docs routing path.
    images: HashMap<String, Option<Arc<RenderImage>>>,
    pending: HashSet<String>,
    code: HashMap<(String, String, bool), Vec<(Range<usize>, HighlightStyle)>>,
}

pub(crate) type SharedCache = Rc<RefCell<BlockCache>>;

/// The Docs routing path an image source names, relative to the document at `doc_path`.
/// Remote URLs and paths that climb out of the Docs roots are not loaded.
pub(crate) fn resolve_image_path(doc_path: &str, src: &str) -> Option<String> {
    let src = src.trim().trim_start_matches("./");
    if src.is_empty() || src.contains("://") || src.starts_with('/') || src.starts_with("data:") {
        return None;
    }
    let mut parts: Vec<&str> = doc_path.split('/').collect();
    parts.pop();
    for segment in src.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// `![alt](src)` sources in a document, for loading ahead of paint.
fn image_sources(text: &str) -> Vec<String> {
    let mut sources = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("![") {
        rest = &rest[start + 2..];
        let Some(close) = rest.find("](") else { break };
        let after = &rest[close + 2..];
        let Some(end) = after.find(')') else { break };
        let target = after[..end].split_whitespace().next().unwrap_or_default();
        if !target.is_empty() {
            sources.push(target.to_string());
        }
        rest = &after[end..];
    }
    sources
}

/// Installs every provider on a fresh editor for the document at `doc_path`.
pub(crate) fn install(
    editor: &Entity<EditorState>,
    cache: &SharedCache,
    doc_path: String,
    light: bool,
    cx: &mut App,
) {
    let mermaid = cache.clone();
    let math = cache.clone();
    let images = cache.clone();
    let code = cache.clone();
    let math_color = math_color_key(light);
    editor.update(cx, |editor, _| {
        editor.set_block_mermaid_provider(move |source| {
            mermaid
                .borrow()
                .mermaid
                .get(&(source.to_string(), light))
                .cloned()
                .flatten()
        });
        editor.set_block_math_provider(move |latex| {
            math.borrow()
                .math
                .get(&(latex.to_string(), math_color))
                .cloned()
                .flatten()
        });
        editor.set_block_math_em(MATH_EM);
        editor.set_block_image_provider(move |src| {
            let path = resolve_image_path(&doc_path, src)?;
            images.borrow().images.get(&path).cloned().flatten()
        });
        editor.set_code_highlighter(move |lang, text| highlight(&code, lang, text, light));
        editor.set_code_languages(
            [
                "text",
                "rust",
                "typescript",
                "javascript",
                "tsx",
                "json",
                "bash",
                "python",
                "go",
                "toml",
                "yaml",
                "css",
                "html",
                "sql",
                "zig",
                "markdown",
            ]
            .into_iter()
            .map(Into::into)
            .collect(),
        );
    });
}

fn math_color_key(light: bool) -> u32 {
    if light { 0x27272a } else { 0xd4d4d4 }
}

/// Starts background renders for every diagram, formula and image in the document that is not
/// cached yet. `scope` reads images through the Docs resource origin (local or remote project).
pub(crate) fn prerender(
    editor: &Entity<EditorState>,
    cache: &SharedCache,
    doc_path: &str,
    light: bool,
    scope: Option<crate::cef::ManageDocsResourceScope>,
    cx: &mut App,
) {
    let text = editor.read(cx).text().to_string();
    enum Job {
        Mermaid(String),
        Math(String),
        Image(String),
    }
    let mut jobs = Vec::new();
    let math_color = math_color_key(light);
    {
        let mut state = cache.borrow_mut();
        for source in zorite_editor::mermaid_sources(&text) {
            let source = source.to_string();
            if !state.mermaid.contains_key(&(source.clone(), light))
                && state.pending.insert(format!("m:{light}:{source}"))
            {
                jobs.push(Job::Mermaid(source));
            }
        }
        for latex in zorite_editor::math_sources(&text)
            .into_iter()
            .chain(zorite_editor::inline_math_sources(&text))
        {
            let latex = latex.to_string();
            if !state.math.contains_key(&(latex.clone(), math_color))
                && state.pending.insert(format!("x:{math_color}:{latex}"))
            {
                jobs.push(Job::Math(latex));
            }
        }
        if scope.is_some() {
            for src in image_sources(&text) {
                if let Some(path) = resolve_image_path(doc_path, &src)
                    && !state.images.contains_key(&path)
                    && state.pending.insert(format!("i:{path}"))
                {
                    jobs.push(Job::Image(path));
                }
            }
        }
    }
    for job in jobs {
        let cache = cache.clone();
        let editor = editor.downgrade();
        let scope = scope.clone();
        let text_color: Hsla = gpui::rgb(math_color).into();
        let task = cx.background_spawn(async move {
            match job {
                Job::Mermaid(source) => {
                    let rendered = render_mermaid(&source, light);
                    (
                        format!("m:{light}:{source}"),
                        Landed::Mermaid(source, rendered),
                    )
                }
                Job::Math(latex) => {
                    let rendered =
                        ratex_gpui::render::render_latex(&latex, MATH_EM, RASTER_DPR, text_color)
                            .map(|done| (done.image, done.width, done.height));
                    (
                        format!("x:{math_color}:{latex}"),
                        Landed::Math(latex, rendered),
                    )
                }
                Job::Image(path) => {
                    let decoded = scope
                        .as_ref()
                        .and_then(|scope| crate::cef::read_manage_docs_resource(scope, &path))
                        .and_then(|bytes| decode_image(&path, &bytes));
                    (format!("i:{path}"), Landed::Image(path, decoded))
                }
            }
        });
        cx.spawn(async move |cx| {
            let (pending, landed) = task.await;
            {
                let mut state = cache.borrow_mut();
                state.pending.remove(&pending);
                match landed {
                    Landed::Mermaid(source, rendered) => {
                        state.mermaid.insert((source, light), rendered);
                    }
                    Landed::Math(latex, rendered) => {
                        state.math.insert((latex, math_color), rendered);
                    }
                    Landed::Image(path, decoded) => {
                        state.images.insert(path, decoded);
                    }
                }
            }
            // A notify alone keeps the editor's row layout from before the bitmap existed, so the
            // block would stay raw until something else repainted; refresh the whole window.
            editor.update(cx, |_, cx| cx.notify()).ok();
            cx.update(|cx| cx.refresh_windows());
        })
        .detach();
    }
}

enum Landed {
    Mermaid(String, Option<Rendered>),
    Math(String, Option<Rendered>),
    Image(String, Option<Arc<RenderImage>>),
}

fn fontdb() -> Arc<resvg::usvg::fontdb::Database> {
    static DB: OnceLock<Arc<resvg::usvg::fontdb::Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = resvg::usvg::fontdb::Database::new();
        db.load_system_fonts();
        Arc::new(db)
    })
    .clone()
}

fn render_mermaid(source: &str, light: bool) -> Option<Rendered> {
    let mut theme = if light {
        mermaid_rs_renderer::Theme::modern()
    } else {
        mermaid_rs_renderer::Theme::dark()
    };
    theme.background = "transparent".into();
    theme.font_family = "Inter, Helvetica Neue, Arial, sans-serif".into();
    let options = mermaid_rs_renderer::RenderOptions {
        theme,
        ..Default::default()
    };
    let svg = mermaid_rs_renderer::render_with_options(source, options).ok()?;
    rasterize_svg(&svg, MAX_DIAGRAM_WIDTH)
}

/// SVG to straight-alpha BGRA at 2x for crisp display, scaled down to fit `max_width` logical px.
fn rasterize_svg(svg: &str, max_width: f32) -> Option<Rendered> {
    let options = resvg::usvg::Options {
        fontdb: fontdb(),
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_str(svg, &options).ok()?;
    let natural = tree.size();
    let fit = (max_width / natural.width()).min(1.0);
    let (logical_w, logical_h) = (natural.width() * fit, natural.height() * fit);
    let scale = fit * RASTER_DPR;
    let width = (natural.width() * scale).ceil().max(1.0) as u32;
    let height = (natural.height() * scale).ceil().max(1.0) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let mut bgra = Vec::with_capacity((width * height * 4) as usize);
    for pixel in pixmap.pixels() {
        let color = pixel.demultiply();
        bgra.extend_from_slice(&[color.blue(), color.green(), color.red(), color.alpha()]);
    }
    let buffer = RgbaImage::from_raw(width, height, bgra)?;
    Some((
        Arc::new(RenderImage::new(vec![Frame::new(buffer)])),
        logical_w,
        logical_h,
    ))
}

fn decode_image(path: &str, bytes: &[u8]) -> Option<Arc<RenderImage>> {
    if path.to_ascii_lowercase().ends_with(".svg") {
        let svg = std::str::from_utf8(bytes).ok()?;
        return rasterize_svg(svg, MAX_DIAGRAM_WIDTH).map(|(image, _, _)| image);
    }
    let mut rgba = image::load_from_memory(bytes).ok()?.into_rgba8();
    for pixel in rgba.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    Some(Arc::new(RenderImage::new(vec![Frame::new(rgba)])))
}

fn highlight(
    cache: &SharedCache,
    lang: &str,
    text: &str,
    light: bool,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let key = (lang.to_string(), text.to_string(), light);
    if let Some(hit) = cache.borrow().code.get(&key) {
        return hit.clone();
    }
    let lang_name = match lang {
        "ts" => "typescript",
        "js" => "javascript",
        "sh" | "shell" | "zsh" => "bash",
        "py" => "python",
        "rs" => "rust",
        "yml" => "yaml",
        "md" => "markdown",
        other => other,
    };
    let mut highlighter = gpui_component::highlighter::SyntaxHighlighter::new(lang_name);
    let rope = gpui_component::Rope::from(text);
    highlighter.update(None, &rope, None);
    let theme = crate::app::native_chat::markdown_style::highlight_theme(light);
    let styles = highlighter.styles(&(0..text.len()), &theme);
    let mut state = cache.borrow_mut();
    if state.code.len() > 512 {
        state.code.clear();
    }
    state.code.insert(key, styles.clone());
    styles
}
