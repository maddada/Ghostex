//! Reading a block's JSON into the document it describes: a page reference, or a drawing made of
//! stacked leaves (charts, stat tiles, tables, text, columns).

use serde_json::{Map, Value};

use crate::scene::PageRef;
use crate::stats::{self, Tile};
use crate::table::{self, TableSpec};
use crate::vega::{self, ChartSpec};
use crate::MAX_SOURCE_BYTES;

/// Leaves inside `rows`/`columns` may nest this deep.
const MAX_DEPTH: usize = 8;

pub(crate) enum Doc {
    Page(PageRef),
    Drawing {
        title: Option<String>,
        rows: Vec<Leaf>,
    },
}

pub(crate) struct Leaf {
    pub title: Option<String>,
    pub body: Body,
}

pub(crate) enum Body {
    Chart(Box<ChartSpec>),
    Stats(Vec<Tile>),
    Table(TableSpec),
    Text(String),
    Columns(Vec<Leaf>),
    Rows(Vec<Leaf>),
}

/// The block's JSON, after the size cap.
pub(crate) fn parse_json(source: &str) -> Result<Value, String> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(format!(
            "This visual is too large ({} KiB); the limit is {} KiB.",
            source.len().div_ceil(1024),
            MAX_SOURCE_BYTES / 1024
        ));
    }
    let trimmed = source.trim_start_matches('\u{feff}').trim();
    if trimmed.is_empty() {
        return Err("The block is empty; put a JSON object in it.".to_string());
    }
    serde_json::from_str(trimmed).map_err(|e| format!("The block isn't valid JSON: {e}."))
}

pub(crate) fn parse_doc(value: &Value) -> Result<Doc, String> {
    let Value::Object(obj) = value else {
        return Err("The block must be a JSON object.".to_string());
    };
    if let Some(page) = obj.get("page") {
        return parse_page(page).map(Doc::Page);
    }
    if obj.contains_key("rows") && !obj.contains_key("mark") {
        for key in obj.keys() {
            if !matches!(key.as_str(), "rows" | "title" | "description" | "$schema") {
                return Err(format!(
                    "Unsupported key \"{key}\" next to \"rows\"; put each part inside \"rows\"."
                ));
            }
        }
        return Ok(Doc::Drawing {
            title: title_text(obj.get("title")),
            rows: parse_leaves(obj.get("rows"), "rows", 1)?,
        });
    }
    let mut leaf = parse_leaf(obj, 0)?;
    let title = leaf.title.take();
    Ok(Doc::Drawing {
        title,
        rows: vec![leaf],
    })
}

/// The page reference a parsed block holds, if it is one.
pub(crate) fn page_of(value: &Value) -> Option<PageRef> {
    value.get("page").and_then(|page| parse_page(page).ok())
}

fn parse_page(page: &Value) -> Result<PageRef, String> {
    let Value::Object(page) = page else {
        return Err(
            "\"page\" must be an object with \"title\" and \"url\" (http:// or https://)."
                .to_string(),
        );
    };
    let url = page
        .get("url")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) || url.len() <= 8 {
        return Err(
            "The page reference needs a \"url\" starting with http:// or https://.".to_string(),
        );
    }
    let title = page
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .unwrap_or("Ghostex page");
    let file = page
        .get("file")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|f| !f.is_empty())
        .map(str::to_string);
    let popup = match page.get("open").and_then(Value::as_str).map(str::trim) {
        None | Some("browser") => false,
        Some("popup") => true,
        Some(other) => {
            return Err(format!(
                "The page's \"open\" is \"popup\" or \"browser\", not {other:?}."
            ))
        }
    };
    // `"questions": ["Q1", "Q2"]`, written by `ghostex show` for a page that asks questions.
    let questions = match page.get("questions") {
        None | Some(Value::Null) => None,
        Some(Value::Array(tags)) => Some(
            tags.iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|tag| !tag.is_empty())
                .map(str::to_string)
                .collect(),
        ),
        Some(_) => {
            return Err(
                "The page's \"questions\" is the list of its question tags, such as [\"Q1\"]."
                    .to_string(),
            )
        }
    };
    // `"awaitsSeen": true`, written by `ghostex show` for a page that asks nothing.
    let awaits_seen = page.get("awaitsSeen").and_then(Value::as_bool) == Some(true);
    Ok(PageRef {
        title: title.to_string(),
        url: url.to_string(),
        file,
        popup,
        questions,
        awaits_seen,
    })
}

/// A title given as a string, an array of lines, or Vega-Lite's `{"text": …}`.
pub(crate) fn title_text(value: Option<&Value>) -> Option<String> {
    let text = match value? {
        Value::String(s) => s.trim().to_string(),
        Value::Array(lines) => lines
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(" "),
        Value::Object(obj) => return title_text(obj.get("text")),
        Value::Number(n) => n.to_string(),
        _ => return None,
    };
    (!text.is_empty()).then_some(text)
}

fn parse_leaves(value: Option<&Value>, key: &str, depth: usize) -> Result<Vec<Leaf>, String> {
    let Some(Value::Array(items)) = value else {
        return Err(format!("\"{key}\" must be an array of parts."));
    };
    if depth > MAX_DEPTH {
        return Err(format!(
            "The block nests \"rows\" and \"columns\" more than {MAX_DEPTH} levels deep."
        ));
    }
    if items.is_empty() {
        return Err(format!("\"{key}\" is empty; add at least one part."));
    }
    items
        .iter()
        .map(|item| match item {
            Value::Object(obj) => parse_leaf(obj, depth),
            _ => Err(format!("Each entry in \"{key}\" must be an object.")),
        })
        .collect()
}

fn parse_leaf(obj: &Map<String, Value>, depth: usize) -> Result<Leaf, String> {
    let title = title_text(obj.get("title"));
    let body = if obj.contains_key("mark") {
        Body::Chart(Box::new(vega::parse_chart(obj)?))
    } else if let Some(chart) = obj.get("chart") {
        let Value::Object(chart) = chart else {
            return Err("\"chart\" must be a Vega-Lite object with a \"mark\".".to_string());
        };
        let spec = vega::parse_chart(chart)?;
        let title = title.or_else(|| spec.title.clone());
        return Ok(Leaf {
            title,
            body: Body::Chart(Box::new(spec)),
        });
    } else if let Some(tiles) = obj.get("stats") {
        Body::Stats(stats::parse(tiles)?)
    } else if let Some(table) = obj.get("table") {
        Body::Table(table::parse(table)?)
    } else if let Some(text) = obj.get("text") {
        match text {
            Value::String(s) => Body::Text(s.clone()),
            Value::Number(n) => Body::Text(n.to_string()),
            _ => return Err("\"text\" must be a string.".to_string()),
        }
    } else if obj.contains_key("columns") {
        Body::Columns(parse_leaves(obj.get("columns"), "columns", depth + 1)?)
    } else if obj.contains_key("rows") {
        Body::Rows(parse_leaves(obj.get("rows"), "rows", depth + 1)?)
    } else {
        if let Some(error) = vega::unsupported_feature(obj) {
            return Err(error);
        }
        return Err(unknown_leaf(obj));
    };
    Ok(Leaf { title, body })
}

fn unknown_leaf(obj: &Map<String, Value>) -> String {
    let keys: Vec<String> = obj
        .keys()
        .filter(|k| k.as_str() != "title")
        .take(6)
        .map(|k| format!("\"{k}\""))
        .collect();
    if keys.is_empty() {
        return "A part has nothing to draw; give it \"chart\", \"stats\", \"table\", \"text\", \"columns\" or \"rows\"."
            .to_string();
    }
    format!(
        "Unknown part with {} {}; use \"chart\", \"stats\", \"table\", \"text\", \"columns\" or \"rows\".",
        if keys.len() == 1 { "key" } else { "keys" },
        keys.join(", ")
    )
}
