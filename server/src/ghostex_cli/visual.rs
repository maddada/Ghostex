//! `ghostex visual`: draw a ```visual block to a PNG so an agent can look at it before replying
//! (`check`), and install the `$ghostex-visuals` skill (`install-skill`).
//!
//! CDXC:SessionChat 2026-10-06 SEE-ALSO:
//! The block is laid out by packages/gx-visual, the same crate every chat client draws with (apps/desktop/src/app/native_chat/visual.rs, apps/mobile/app/src/chat/native/transcript/VisualBlock.tsx), so the check shows what the reader will see. skills/ghostex-visuals/SKILL.md tells agents to run it.

use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    io::Read,
    path::PathBuf,
    sync::Arc,
};

use ghostex_gx_visual::{render, scene_to_svg, Theme, Visual, MAX_SOURCE_BYTES};

use crate::ghostex_cli::rpc::{CliError, CliResult};
use crate::ghostex_cli::{skills, usage, visual_page_check};

const DEFAULT_WIDTH: f32 = 640.0;
/// The families the PNG's text is set in; the chat draws with its own font, so these only need to
/// be ones every machine has.
const FONT_FAMILY: &str = "Segoe UI, Helvetica Neue, Helvetica, Arial, DejaVu Sans, sans-serif";
/// Margin around the drawing in the PNG, in scene units.
const MARGIN: f32 = 16.0;
const PIXEL_RATIO: f32 = 2.0;

pub fn visual_command(args: &[String]) -> CliResult<()> {
    let subcommand = args.first().map(String::as_str).unwrap_or("help");
    let rest = args.get(1..).unwrap_or_default();
    match subcommand {
        "help" | "-h" | "--help" => {
            println!("{}", usage::visuals_usage());
            Ok(())
        }
        "check" => check_command(rest),
        "install-skill" => skills::install_visuals_skill_command(rest),
        other => Err(CliError::Other(format!(
            "Unknown visual command: {other}\n\n{}",
            usage::visuals_usage()
        ))),
    }
}

struct CheckOptions {
    file: String,
    /// `None` when `--width` was not given: a block is drawn at [`DEFAULT_WIDTH`], a page is
    /// checked at a phone's width and a computer's.
    width: Option<f32>,
    light: bool,
    out: Option<PathBuf>,
}

fn parse_check(args: &[String]) -> CliResult<Option<CheckOptions>> {
    let mut file: Option<String> = None;
    let mut width = None;
    let mut light = false;
    let mut out = None;
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        let value = |index: usize| {
            args.get(index + 1)
                .cloned()
                .ok_or_else(|| CliError::Other(format!("{arg} needs a value.")))
        };
        match arg {
            "-h" | "--help" => return Ok(None),
            "--light" => light = true,
            "--dark" => light = false,
            "--width" => {
                let text = value(index)?;
                width = text
                    .parse::<f32>()
                    .ok()
                    .filter(|width| width.is_finite() && *width > 0.0)
                    .ok_or_else(|| {
                        CliError::Other(format!("--width takes a number, not {text:?}."))
                    })
                    .map(Some)?;
                index += 1;
            }
            "--out" => {
                out = Some(PathBuf::from(value(index)?));
                index += 1;
            }
            flag if flag.starts_with("--") => {
                return Err(CliError::Other(format!("Unknown option {flag}.")));
            }
            path => {
                if file.replace(path.to_string()).is_some() {
                    return Err(CliError::Other(
                        "visual check takes one file (or - for stdin).".to_string(),
                    ));
                }
            }
        }
        index += 1;
    }
    let file = file.ok_or_else(|| {
        CliError::Other(
            "visual check needs the block's JSON or a page: ghostex visual check <file.json|-|page.html> [--width N] [--light] [--out file.png]"
                .to_string(),
        )
    })?;
    Ok(Some(CheckOptions {
        file,
        width,
        light,
        out,
    }))
}

fn read_source(file: &str) -> CliResult<String> {
    let mut bytes = Vec::new();
    let limit = MAX_SOURCE_BYTES as u64 + 1;
    if file == "-" {
        std::io::stdin()
            .take(limit)
            .read_to_end(&mut bytes)
            .map_err(|error| CliError::Other(format!("Could not read stdin: {error}")))?;
    } else {
        fs::File::open(file)
            .and_then(|handle| handle.take(limit).read_to_end(&mut bytes))
            .map_err(|error| CliError::Other(format!("Could not read {file}: {error}")))?;
    }
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err(CliError::Other(format!(
            "The block is over {} KiB; inline fewer rows.",
            MAX_SOURCE_BYTES / 1024
        )));
    }
    let text = String::from_utf8(bytes)
        .map_err(|_| CliError::Other("The block is not UTF-8 text.".to_string()))?;
    Ok(strip_fence(&text))
}

/// The block's JSON, accepting the whole fenced block as an agent would paste it into a reply.
fn strip_fence(text: &str) -> String {
    let trimmed = text.trim();
    if !trimmed.starts_with("```") && !trimmed.starts_with("~~~") {
        return trimmed.to_string();
    }
    let mut lines = trimmed.lines().collect::<Vec<_>>();
    lines.remove(0);
    if lines.last().is_some_and(|line| {
        line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~")
    }) {
        lines.pop();
    }
    lines.join("\n")
}

fn fontdb() -> Arc<resvg::usvg::fontdb::Database> {
    let mut db = resvg::usvg::fontdb::Database::new();
    db.load_system_fonts();
    Arc::new(db)
}

/// The SVG painted over the theme's background with a margin, at twice its size.
fn rasterize(svg: &str, width: f32, height: f32, theme: &Theme) -> CliResult<Vec<u8>> {
    let options = resvg::usvg::Options {
        fontdb: fontdb(),
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_str(svg, &options)
        .map_err(|error| CliError::Other(format!("The drawing could not be read back: {error}")))?;
    let device_width = ((width + MARGIN * 2.0) * PIXEL_RATIO).ceil().max(1.0) as u32;
    let device_height = ((height + MARGIN * 2.0) * PIXEL_RATIO).ceil().max(1.0) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(device_width, device_height)
        .ok_or_else(|| CliError::Other("The drawing is too large to paint.".to_string()))?;
    let background = theme.background;
    pixmap.fill(resvg::tiny_skia::Color::from_rgba8(
        background.r,
        background.g,
        background.b,
        255,
    ));
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(PIXEL_RATIO, PIXEL_RATIO)
            .pre_translate(MARGIN, MARGIN),
        &mut pixmap.as_mut(),
    );
    pixmap
        .encode_png()
        .map_err(|error| CliError::Other(format!("The PNG could not be written: {error}")))
}

fn default_out(source: &str, options: &CheckOptions) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    options.width.map(f32::to_bits).hash(&mut hasher);
    options.light.hash(&mut hasher);
    std::env::temp_dir()
        .join("ghostex-visuals")
        .join(format!("check-{:016x}.png", hasher.finish()))
}

/// The form a ```questions block describes, when the source is one (an object with `questions`).
/// A top-level list is read as a questions block too, so a list of questions gets the error the
/// chat shows for it rather than a visual's.
fn question_form(source: &str) -> Option<ghostex_gx_visual::questions::QuestionForm> {
    let value: serde_json::Value = serde_json::from_str(source).ok()?;
    (value.is_array() || value.get("questions").is_some() || value.get("withdraw").is_some())
        .then(|| ghostex_gx_visual::questions::parse_form(source))
}

/// What the chat will show for a questions block, or the problem that stops it.
fn report_question_form(form: &ghostex_gx_visual::questions::QuestionForm) -> CliResult<()> {
    if !form.error.is_empty() {
        return Err(CliError::Other(format!(
            "The questions block can't be shown: {}",
            form.error
        )));
    }
    if !form.withdraw.is_empty() {
        println!(
            "Withdraws {}: they no longer count as unanswered.",
            form.withdraw.join(", ")
        );
    }
    if form.is_withdrawal() {
        return Ok(());
    }
    println!(
        "A question form, \"{}\", with {} question{}:",
        form.title,
        form.questions.len(),
        if form.questions.len() == 1 { "" } else { "s" }
    );
    for question in &form.questions {
        let kind = if question.text {
            "a text answer".to_string()
        } else {
            format!(
                "{} {} choice{} plus \"{}\"",
                question.choices.len(),
                if question.multiple {
                    "multi-pick"
                } else {
                    "single-pick"
                },
                if question.choices.len() == 1 { "" } else { "s" },
                ghostex_gx_visual::questions::OTHER_CHOICE
            )
        };
        let tag = if question.tag.is_empty() {
            String::new()
        } else {
            format!("{}: ", question.tag)
        };
        println!(
            "- {tag}{} (answer key \"{}\"): {kind}",
            question.title, question.key
        );
    }
    if ghostex_gx_visual::questions::takes_notes(form) {
        println!(
            "- then a notes box for anything else (answer key \"{}\", only when filled in)",
            ghostex_gx_visual::questions::notes_key(form)
        );
    }
    println!(
        "A question the reader leaves blank arrives as \"{}\" (skipped: move on, and don't ask it again unless it blocks the work).",
        ghostex_gx_visual::questions::SKIPPED
    );
    Ok(())
}

fn check_command(args: &[String]) -> CliResult<()> {
    let Some(options) = parse_check(args)? else {
        println!("{}", usage::visuals_usage());
        return Ok(());
    };
    if visual_page_check::is_page_file(&options.file) {
        return visual_page_check::check_page_command(
            &options.file,
            options.width.map(|width| width.round().max(1.0) as u32),
            options.light,
            options.out,
        );
    }
    let source = read_source(&options.file)?;
    let theme = if options.light {
        Theme::light()
    } else {
        Theme::dark()
    };
    if let Some(form) = question_form(&source) {
        return report_question_form(&form);
    }
    let visual = render(&source, options.width.unwrap_or(DEFAULT_WIDTH), &theme)
        .map_err(|message| CliError::Other(format!("The block can't be drawn: {message}")))?;
    let scene = match visual {
        Visual::Page(page) => {
            println!(
                "This block is a page card: \"{}\", which opens {}",
                page.title, page.url
            );
            return Ok(());
        }
        Visual::Drawing(scene) => scene,
    };
    let svg = scene_to_svg(&scene, FONT_FAMILY);
    let png = rasterize(&svg, scene.width, scene.height, &theme)?;
    let out = options
        .out
        .clone()
        .unwrap_or_else(|| default_out(&source, &options));
    if let Some(parent) = out.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|error| {
            CliError::Other(format!("Could not create {}: {error}", parent.display()))
        })?;
    }
    fs::write(&out, png)
        .map_err(|error| CliError::Other(format!("Could not write {}: {error}", out.display())))?;
    println!(
        "Drew {} at {}×{} ({} hover regions, {} theme): {}",
        scene
            .title
            .as_deref()
            .map_or_else(|| "the block".to_string(), |title| format!("\"{title}\"")),
        scene.width.round(),
        scene.height.round(),
        scene.regions.len(),
        if options.light { "light" } else { "dark" },
        out.display()
    );
    Ok(())
}
