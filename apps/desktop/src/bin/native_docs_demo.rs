//! Standalone preview of the native Files view's Markdown document: the same live editor
//! (zorite-editor), Docs palette, rendered blocks (math, Mermaid, images, code colours), gutter and
//! overlays the app draws, in a window of its own, so the look can be compared with the React
//! Files page without launching Ghostex.
//!
//!     GHOSTEX_NATIVE_DOCS_DEMO_FILE=<absolute path>       (default: native_docs_demo/parity.md; a CSV, JSON or code file opens the views in native_docs_demo/data.rs, a picture the zoomable picture in native_docs_demo/picture.rs)
//!     GHOSTEX_NATIVE_DOCS_DEMO_THEME=dark|light           (default dark)
//!     GHOSTEX_NATIVE_DOCS_DEMO_MODE=live|source           (default live)
//!     GHOSTEX_NATIVE_DOCS_DEMO_SIZE=<w>x<h>               (logical px, default 1100x900)
//!     GHOSTEX_NATIVE_DOCS_DEMO_ZOOM=<percent>             (the Files text zoom, 70 to 200, default 100)
//!     GHOSTEX_NATIVE_DOCS_DEMO_SCROLL=<px>                (scroll the document down by this much)
//!     GHOSTEX_NATIVE_DOCS_DEMO_CARET=<line>               (1-based line to put the caret on)
//!     GHOSTEX_NATIVE_DOCS_DEMO_KEYS="<keystroke> ..."     (pressed in order through the keymap once loaded; the Markdown and code editors answer alt-up / alt-down like Files)
//!     GHOSTEX_NATIVE_DOCS_DEMO_BASE=<absolute path>       (a HEAD version for the git stripe)
//!     GHOSTEX_NATIVE_MODAL_DEMO_BACKGROUND=1              (open without taking focus)

#![allow(dead_code, unused_imports)]

#[path = "../assets.rs"]
mod assets;
#[path = "../ui_fonts.rs"]
mod ui_fonts;

/// The app modules the Docs files reach through `crate::app::…`, with stand-ins for the pieces
/// that belong to the rest of the app. They live in files under `native_docs_demo/` because a
/// `#[path]` inside an inline module resolves through folders named after the modules, which
/// do not exist, and macOS will not walk `..` out of a missing folder.
#[path = "native_docs_demo/app.rs"]
mod app;
#[path = "native_docs_demo/data.rs"]
mod data;
#[path = "native_docs_demo/picture.rs"]
mod picture;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use app::helpers::manage_docs_resources::{ManageDocsResourceRoot, ManageDocsResourceScope};
use app::native_docs::{
    blocks, editor_style, gutter, markdown_body, palette::DocsPalette, table_tools,
};
use gpui::{
    App, AppContext as _, Bounds, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, ScrollHandle, SharedString, Styled as _, Window, WindowBounds,
    WindowOptions, div, point, px, size,
};
use gpui_component::Root;
use zorite_editor::{EditorEvent, EditorState};

/// The settings above, read from the environment or from `NAME=value` launch arguments (a
/// launcher such as SpaceO passes arguments but no environment).
fn env(name: &str) -> String {
    let prefix = format!("{name}=");
    std::env::args()
        .skip(1)
        .find_map(|argument| argument.strip_prefix(&prefix).map(str::to_string))
        .or_else(|| std::env::var(name).ok())
        .unwrap_or_default()
        .trim()
        .to_string()
}

struct DocsDemo {
    live: Entity<EditorState>,
    scroll: ScrollHandle,
    palette: DocsPalette,
    source: bool,
    base: Option<String>,
    changes: Option<(Vec<gutter::LineChange>, Vec<usize>)>,
    cache: blocks::SharedCache,
    doc_path: String,
    scope: ManageDocsResourceScope,
    scrolled: bool,
    _subscription: gpui::Subscription,
}

impl DocsDemo {
    fn refresh(&mut self, cx: &mut Context<Self>) {
        let text = self.live.read(cx).text().to_string();
        self.changes = self.base.as_deref().map(|base| gutter::diff(base, &text));
        blocks::prerender(
            &self.live,
            &self.cache,
            &self.doc_path,
            self.palette.light,
            Some(self.scope.clone()),
            cx,
        );
        cx.notify();
    }
}

impl Render for DocsDemo {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.scrolled {
            let scroll_by: f32 = env("GHOSTEX_NATIVE_DOCS_DEMO_SCROLL")
                .parse()
                .unwrap_or(0.0);
            if scroll_by > 0.0 && self.scroll.bounds().size.height > px(0.0) {
                self.scroll.set_offset(point(px(0.0), px(-scroll_by)));
                self.scrolled = true;
            }
            window.request_animation_frame();
        }
        let p = &self.palette;
        let body = markdown_body::render_markdown_body(
            markdown_body::DocsMarkdownBody {
                id: SharedString::from("native-docs-demo-scroll"),
                live: &self.live,
                scroll: &self.scroll,
                source: self.source,
                line_numbers: true,
                constrain: false,
                sliding: false,
                changes: self.changes.as_ref(),
                table_actions: &table_tools::TableActionHost {
                    copy: std::rc::Rc::new(|text, cx| {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text))
                    }),
                    open: Some(std::rc::Rc::new(|_, _| {})),
                },
            },
            p,
            window,
            cx,
        );
        let ruler = markdown_body::render_overview_ruler(
            &self.live,
            &self.scroll,
            self.changes.as_ref(),
            p,
            cx,
        );
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(p.page)
            .font_family(p.font.clone())
            .text_color(p.text)
            // The Files view's own Option+Up / Option+Down handler (`native_docs_key_down`), on the
            // ancestor of the focused editor as in the app.
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                if let Some(down) = app::native_docs::move_lines::move_lines_key(event) {
                    this.live.update(cx, |editor, cx| {
                        app::native_docs::move_lines::move_lines_in_live(editor, down, cx)
                    });
                    cx.stop_propagation();
                }
            }))
            .child(
                div()
                    .flex_none()
                    .h(px(35.0))
                    .pl(px(13.0))
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(p.border)
                    .bg(p.chrome)
                    .text_size(px(12.0))
                    .child(SharedString::from(self.doc_path.clone())),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(body)
                    .children(ruler)
                    .child(markdown_body::render_body_scrollbar(&self.scroll)),
            )
    }
}

fn main() {
    let light = env("GHOSTEX_NATIVE_DOCS_DEMO_THEME") == "light";
    if let Ok(zoom) = env("GHOSTEX_NATIVE_DOCS_DEMO_ZOOM").parse::<f64>() {
        let settings = serde_json::Map::from_iter([(
            ui_fonts::FILES_ZOOM_SETTING.to_string(),
            serde_json::Value::from(zoom),
        )]);
        ui_fonts::sync_files_zoom(&settings);
    }
    let source = env("GHOSTEX_NATIVE_DOCS_DEMO_MODE") == "source";
    let demo_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/bin/native_docs_demo");
    let file = match env("GHOSTEX_NATIVE_DOCS_DEMO_FILE") {
        path if !path.is_empty() => PathBuf::from(path),
        _ => demo_dir.join("parity.md"),
    };
    let (width, height) = env("GHOSTEX_NATIVE_DOCS_DEMO_SIZE")
        .split_once('x')
        .and_then(|(w, h)| Some((w.parse::<f32>().ok()?, h.parse::<f32>().ok()?)))
        .unwrap_or((1100.0, 900.0));
    // A picture opens in the Files view's zoomable picture (picture.rs).
    if matches!(
        file.extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "svg")
    ) {
        picture::run(&file, light, width, height);
        return;
    }
    let text = std::fs::read_to_string(&file).expect("read the demo document");
    // Anything but Markdown: the CSV table, the JSON tree or the code editor (data.rs).
    if !matches!(
        file.extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("md" | "markdown" | "mdx")
    ) {
        data::run(&file, text, light, width, height);
        return;
    }
    let base = match env("GHOSTEX_NATIVE_DOCS_DEMO_BASE") {
        path if !path.is_empty() => std::fs::read_to_string(path).ok(),
        _ => None,
    };
    // Images resolve inside the document's folder, as a Files document's do inside its project.
    let root = file.parent().map(Path::to_path_buf).unwrap_or_default();
    let doc_path = file
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let caret_line: Option<usize> = env("GHOSTEX_NATIVE_DOCS_DEMO_CARET").parse().ok();
    gpui_platform::application()
        .with_assets(assets::GhostexAssets)
        .run(move |cx: &mut App| {
            gpui_component::init(cx);
            ui_fonts::register(cx);
            register_mono(cx);
            zorite_editor::bind_keys(cx);
            let window_size = size(px(width), px(height));
            let bounds = cx
                .primary_display()
                .map(|display| Bounds::centered_at(display.bounds().center(), window_size))
                .unwrap_or_else(|| Bounds::new(point(px(120.0), px(80.0)), window_size));
            let background = env("GHOSTEX_NATIVE_MODAL_DEMO_BACKGROUND") == "1";
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                focus: !background,
                show: true,
                ..Default::default()
            };
            let window = cx
                .open_window(options, move |window, cx| {
                    window.set_window_title("Native Docs demo");
                    let palette = DocsPalette::resolve(
                        false,
                        light,
                        ui_fonts::UI_FONT.to_string(),
                        gpui::rgb(if light { 0xffffff } else { 0x0e0e0e }).into(),
                    );
                    let style = editor_style::syntax_style(&palette);
                    let live = cx.new(|cx| {
                        let mut editor = EditorState::new(window, cx).with_text(text.clone());
                        editor.set_tab_indent(2);
                        editor.set_labels(editor_style::labels(), cx);
                        if !source {
                            editor.set_markdown_style(style, cx);
                        }
                        if let Some(line) = caret_line {
                            let offset = text
                                .split_inclusive('\n')
                                .take(line.saturating_sub(1))
                                .map(str::len)
                                .sum::<usize>();
                            editor.set_cursor(offset, cx);
                        }
                        editor
                    });
                    let cache = blocks::SharedCache::default();
                    let expand: app::native_docs::mermaid_widget::MermaidExpand =
                        std::rc::Rc::new(|source, _| {
                            eprintln!("expand diagram: {} bytes", source.len())
                        });
                    blocks::install(
                        &live,
                        &cache,
                        doc_path.clone(),
                        light,
                        editor_style::mermaid_colors(&palette),
                        expand,
                        cx,
                    );
                    let root = root.clone();
                    let scope = ManageDocsResourceScope::new(
                        Arc::new(move || {
                            Some(vec![ManageDocsResourceRoot {
                                allowed_relative_roots: vec![String::new()],
                                mount_segment: String::new(),
                                path: root.clone(),
                            }])
                        }),
                        Arc::new(|_| None),
                    );
                    let view = cx.new(|cx: &mut Context<DocsDemo>| {
                        let subscription =
                            cx.subscribe(&live, |this, _, event: &EditorEvent, cx| {
                                if matches!(event, EditorEvent::Changed) {
                                    this.refresh(cx);
                                }
                            });
                        let mut demo = DocsDemo {
                            live: live.clone(),
                            scroll: ScrollHandle::new(),
                            palette,
                            source,
                            base: base.clone(),
                            changes: None,
                            cache,
                            doc_path: doc_path.clone(),
                            scope,
                            scrolled: false,
                            _subscription: subscription,
                        };
                        demo.refresh(cx);
                        demo
                    });
                    if !background {
                        window.activate_window();
                    }
                    // A caret line puts the caret there with the editor focused (its table tools,
                    // raw source on the caret line).
                    if caret_line.is_some() {
                        live.update(cx, |editor, cx| editor.focus(window, cx));
                    }
                    cx.new(|cx| Root::new(view, window, cx))
                })
                .expect("open the demo window");
            // Keystrokes through the keymap, as a keyboard would send them (posted window messages
            // cannot carry Ctrl or Shift).
            let keys = env("GHOSTEX_NATIVE_DOCS_DEMO_KEYS");
            if !keys.is_empty() {
                let handle: gpui::AnyWindowHandle = window.into();
                let os = cx.platform_info().os;
                cx.spawn(async move |cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_secs(3))
                        .await;
                    for key in keys.split_whitespace() {
                        let Ok(keystroke) = gpui::Keystroke::parse_with_os(key, os) else {
                            continue;
                        };
                        let _ = cx.update_window(handle, |_, window, cx| {
                            window.dispatch_keystroke(keystroke, cx);
                        });
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(120))
                            .await;
                    }
                })
                .detach();
            }
            cx.activate(!background);
        });
}

fn register_mono(cx: &App) {
    let fonts = vec![
        include_bytes!(
            "../../../../.dependencies/ghostty/src/font/res/JetBrainsMonoNerdFont-Regular.ttf"
        )
        .as_slice()
        .into(),
        include_bytes!(
            "../../../../.dependencies/ghostty/src/font/res/JetBrainsMonoNerdFont-Bold.ttf"
        )
        .as_slice()
        .into(),
        include_bytes!(
            "../../../../.dependencies/ghostty/src/font/res/JetBrainsMonoNerdFont-Italic.ttf"
        )
        .as_slice()
        .into(),
    ];
    let _ = cx.text_system().add_fonts(fonts);
}
