//! Pins that every label a block draws is the transcript's body-text size (`Theme::text_size`)
//! and that the stat values stay in proportion to it, at the default size and at a larger one.

use crate::{render, Item, Scene, Theme, Visual, DEFAULT_TEXT_SIZE};

const BLOCK: &str = r##"{"title":"Sizes","rows":[
  {"title":"Section","text":"A paragraph of body text that is long enough to wrap onto a second line at a narrow chat width."},
  {"stats":[{"label":"Named so since","value":"22 Oct","note":"Course Pages' start"},
            {"label":"Model","value":"Opus 5.5","note":"reviews: Sonnet","tone":"bad"}]},
  {"table":{"columns":["Step","It does","Count"],"rows":[["1. Plan","Outline",3],["2. Brief","One brief per CLO",4]]}},
  {"chart":{"mark":"line","data":{"values":[
     {"day":"Mon","n":3,"kind":"a"},{"day":"Tue","n":5,"kind":"a"},{"day":"Mon","n":2,"kind":"b"},{"day":"Tue","n":6,"kind":"b"}]},
     "encoding":{"x":{"field":"day","type":"ordinal","title":"Day"},"y":{"field":"n","type":"quantitative","title":"Count"},"color":{"field":"kind","type":"nominal"}}}},
  {"chart":{"mark":"bar","data":{"values":[{"c":"Alpha","n":3},{"c":"Beta","n":5},{"c":"Gamma","n":4}]},
     "encoding":{"x":{"field":"c","type":"nominal"},"y":{"field":"n","type":"quantitative"}}}},
  {"chart":{"mark":"arc","data":{"values":[{"c":"Alpha","n":3},{"c":"Beta","n":5}]},
     "encoding":{"theta":{"field":"n","type":"quantitative"},"color":{"field":"c","type":"nominal"}}}}
]}"##;

fn scene(text_size: f32, width: f32) -> Scene {
    let theme = Theme {
        text_size,
        ..Theme::dark()
    };
    match render(BLOCK, width, &theme).expect("the sample block draws") {
        Visual::Drawing(scene) => scene,
        Visual::Page(_) => panic!("a drawing, not a page"),
    }
}

fn text_sizes(scene: &Scene) -> Vec<(String, f32)> {
    scene
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Text { text, size, .. } => Some((text.clone(), *size)),
            _ => None,
        })
        .collect()
}

#[test]
fn the_default_is_the_transcripts_prose_size() {
    assert_eq!(Theme::dark().text_size, DEFAULT_TEXT_SIZE);
    assert_eq!(Theme::light().text_size, 14.0);
}

#[test]
fn every_label_is_the_body_text_size_and_only_stat_values_are_larger() {
    for (text_size, width) in [(14.0, 640.0), (14.0, 300.0), (18.0, 640.0)] {
        let sizes = text_sizes(&scene(text_size, width));
        assert!(sizes.len() > 30, "the sample draws plenty of text");
        let values = ["22 Oct", "Opus 5.5"];
        for (text, size) in &sizes {
            if values.contains(&text.as_str()) {
                assert!(
                    (*size - text_size * 26.0 / 14.0).abs() < 0.01,
                    "stat value {text:?} is in proportion at {text_size}: {size}"
                );
            } else {
                assert_eq!(*size, text_size, "{text:?} at width {width}");
            }
        }
    }
}

#[test]
fn a_larger_text_size_grows_the_drawing_and_nothing_overlaps_in_a_table() {
    let small = scene(14.0, 640.0);
    let large = scene(20.0, 640.0);
    assert!(large.height > small.height);
    // The table's rows are one line of text plus padding, so the text always fits its row.
    for scene in [&small, &large] {
        let size = if std::ptr::eq(scene, &small) {
            14.0
        } else {
            20.0
        };
        let cells: Vec<f32> = scene
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Text { text, y, .. } if text == "One brief per CLO" => Some(*y),
                _ => None,
            })
            .collect();
        assert_eq!(cells.len(), 1);
        let rules: Vec<f32> = scene
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Path { points, .. } if points.len() == 2 && points[0][1] == points[1][1] => {
                    Some(points[0][1])
                }
                _ => None,
            })
            .collect();
        let above = rules
            .iter()
            .copied()
            .filter(|y| *y < cells[0])
            .fold(f32::MIN, f32::max);
        let below = rules
            .iter()
            .copied()
            .filter(|y| *y > cells[0])
            .fold(f32::MAX, f32::min);
        assert!(
            cells[0] - size * 0.85 >= above,
            "the glyphs clear the rule above"
        );
        assert!(
            cells[0] + size * 0.3 <= below,
            "the glyphs clear the rule below"
        );
    }
}

#[test]
fn the_theme_json_carries_the_text_size() {
    let theme = Theme::from_json(&serde_json::json!({ "textSize": 16 }));
    assert_eq!(theme.text_size, 16.0);
    assert_eq!(Theme::from_json(&serde_json::json!({})).text_size, 14.0);
    assert_eq!(
        Theme::from_json(&serde_json::json!({ "textSize": -3 })).text_size,
        14.0
    );
}
