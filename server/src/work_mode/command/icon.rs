//! A command source's icon: the SVG text its `info` sends, rebuilt from an allowlist before any
//! client sees it.
//!
//! CDXC:WorkMode 2026-10-31 WHY:
//! Clients draw the icon, and an SVG can carry scripts, external fetches, entity bombs and CSS.
//! So the text is parsed (no DTD), every element and attribute outside a small drawing
//! allowlist is dropped with everything inside it, anything that could reach outside the icon
//! fails it where it would be drawn, and what is left is written out again rather than passed
//! through. `currentColor` stays allowed so an icon can follow the client's light or dark text
//! color: an SVG loaded as an image (a data URL) inherits nothing, so a client draws the icon
//! inline, or puts its own text color in place of `currentColor` before it encodes the icon.

/// The icon text a command may send.
const MAX_ICON_BYTES: usize = 16 * 1024;
const MAX_ELEMENTS: usize = 500;
const MAX_DEPTH: usize = 16;

const SVG_NAMESPACE: &str = "http://www.w3.org/2000/svg";

/// How much of a refused attribute value an error quotes: Settings shows it.
const MAX_SHOWN_VALUE_CHARS: usize = 40;

/// Elements kept (with their allowed attributes); any other SVG element is dropped with
/// everything inside it.
const ELEMENTS: [&str; 15] = [
    "svg",
    "g",
    "path",
    "circle",
    "ellipse",
    "rect",
    "line",
    "polyline",
    "polygon",
    "defs",
    "linearGradient",
    "radialGradient",
    "stop",
    "clipPath",
    "title",
];

/// Elements that fail the icon where it would draw them: they run code, load something or
/// restyle the page around the icon. Inside an element that is dropped, one is dropped with it,
/// since nothing of it is written.
const REJECTED_ELEMENTS: [&str; 7] = [
    "script",
    "foreignObject",
    "image",
    "use",
    "style",
    "filter",
    "mask",
];

const ATTRIBUTES: [&str; 35] = [
    // Geometry.
    "d",
    "cx",
    "cy",
    "r",
    "rx",
    "ry",
    "x",
    "y",
    "x1",
    "y1",
    "x2",
    "y2",
    "width",
    "height",
    "points",
    "transform",
    "viewBox",
    // Paint.
    "fill",
    "stroke",
    "stroke-width",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-miterlimit",
    "fill-rule",
    "clip-rule",
    "opacity",
    "fill-opacity",
    "stroke-opacity",
    "stop-color",
    "stop-opacity",
    "offset",
    "gradientUnits",
    "gradientTransform",
    // References inside the icon.
    "id",
    "clip-path",
];

/// The icon rebuilt from the allowlist, or why it was dropped (Settings shows it).
pub(crate) fn sanitize_icon(text: &str) -> Result<String, String> {
    if text.len() > MAX_ICON_BYTES {
        return Err("it is larger than 16 KB".to_string());
    }
    let document = roxmltree::Document::parse(text).map_err(|error| match error {
        roxmltree::Error::DtdDetected => "it declares a DOCTYPE".to_string(),
        error => format!("it isn't valid XML ({error})"),
    })?;
    let root = document.root_element();
    if root.tag_name().name() != "svg" || root.tag_name().namespace() != Some(SVG_NAMESPACE) {
        return Err("its root element isn't an SVG <svg>".to_string());
    }
    let view_box = root
        .attribute("viewBox")
        .filter(|view_box| valid_view_box(view_box))
        .ok_or("it has no usable viewBox")?;
    let elements = document
        .descendants()
        .filter(roxmltree::Node::is_element)
        .count();
    if elements > MAX_ELEMENTS {
        return Err(format!("it has more than {MAX_ELEMENTS} elements"));
    }

    let mut svg = format!(
        "<svg xmlns=\"{SVG_NAMESPACE}\" viewBox=\"{}\"",
        escape(view_box)
    );
    write_attributes(root, &mut svg, &["viewBox", "width", "height"])?;
    svg.push('>');
    for child in root.children() {
        write_node(child, 1, &mut svg)?;
    }
    svg.push_str("</svg>");

    let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default())
        .map_err(|error| format!("it doesn't draw as an SVG ({error})"))?;
    let bounds = tree.root().abs_bounding_box();
    if !tree.root().has_children() || (bounds.width() <= 0.0 && bounds.height() <= 0.0) {
        return Err("it draws nothing".to_string());
    }
    Ok(svg)
}

fn write_node(node: roxmltree::Node, depth: usize, out: &mut String) -> Result<(), String> {
    if !node.is_element() {
        return Ok(());
    }
    if depth > MAX_DEPTH {
        return Err(format!("it nests elements deeper than {MAX_DEPTH}"));
    }
    let name = node.tag_name().name();
    if REJECTED_ELEMENTS.contains(&name) || name.starts_with("animate") {
        return Err(format!("it contains a <{name}> element"));
    }
    // Editor metadata in other namespaces, text, markers and the like are not drawn.
    if node.tag_name().namespace() != Some(SVG_NAMESPACE)
        || name == "svg"
        || !ELEMENTS.contains(&name)
    {
        return Ok(());
    }
    out.push('<');
    out.push_str(name);
    write_attributes(node, out, &[])?;
    out.push('>');
    if name == "title" {
        out.push_str(&escape(node.text().unwrap_or_default()));
    } else {
        for child in node.children() {
            write_node(child, depth + 1, out)?;
        }
    }
    out.push_str("</");
    out.push_str(name);
    out.push('>');
    Ok(())
}

/// The element's allowed attributes, each checked; `skip` names attributes the caller wrote
/// itself or drops.
fn write_attributes(node: roxmltree::Node, out: &mut String, skip: &[&str]) -> Result<(), String> {
    for attribute in node.attributes() {
        let name = attribute.name();
        let value = attribute.value();
        if name.eq_ignore_ascii_case("style")
            || name.eq_ignore_ascii_case("href")
            || name.to_ascii_lowercase().starts_with("on")
        {
            return Err(format!("it has a {name} attribute"));
        }
        if attribute.namespace().is_some() || skip.contains(&name) || !ATTRIBUTES.contains(&name) {
            continue;
        }
        let allowed = match name {
            "fill" | "stroke" | "stop-color" => valid_paint(value),
            "clip-path" => local_reference(value),
            "id" => valid_id(value),
            _ => !value.to_ascii_lowercase().contains("url("),
        };
        if !allowed {
            return Err(format!(
                "its {name} is \"{}\", which an icon can't use",
                crate::work_mode::cut(value, MAX_SHOWN_VALUE_CHARS)
            ));
        }
        out.push_str(&format!(" {name}=\"{}\"", escape(value)));
    }
    Ok(())
}

/// Four finite numbers, the last two positive, and nothing else.
fn valid_view_box(view_box: &str) -> bool {
    let Ok(numbers) = view_box
        .split(|c: char| c.is_ascii_whitespace() || c == ',')
        .filter(|part| !part.is_empty())
        .map(str::parse::<f64>)
        .collect::<Result<Vec<f64>, _>>()
    else {
        return false;
    };
    numbers.len() == 4
        && numbers.iter().all(|number| number.is_finite())
        && numbers[2] > 0.0
        && numbers[3] > 0.0
}

/// A color, `none`, `currentColor`, or a gradient in the icon (`url(#id)`).
fn valid_paint(value: &str) -> bool {
    let value = value.trim();
    if value == "none" || value == "currentColor" || local_reference(value) {
        return true;
    }
    if let Some(hex) = value.strip_prefix('#') {
        return matches!(hex.len(), 3 | 4 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit());
    }
    let lower = value.to_ascii_lowercase();
    for function in ["rgb(", "rgba(", "hsl(", "hsla("] {
        if let Some(arguments) = lower
            .strip_prefix(function)
            .and_then(|rest| rest.strip_suffix(')'))
        {
            return arguments
                .chars()
                .all(|c| c.is_ascii_digit() || " .,%".contains(c));
        }
    }
    // A named color (`red`, `transparent`).
    !value.is_empty() && value.len() <= 20 && value.chars().all(|c| c.is_ascii_alphabetic())
}

/// `url(#id)`: something defined inside the icon itself.
fn local_reference(value: &str) -> bool {
    value
        .trim()
        .strip_prefix("url(#")
        .and_then(|rest| rest.strip_suffix(')'))
        .is_some_and(valid_id)
}

fn valid_id(value: &str) -> bool {
    value.len() <= 64
        && value
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
