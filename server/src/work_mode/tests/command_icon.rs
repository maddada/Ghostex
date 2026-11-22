//! Which SVG icons a command source may send, what of them is kept, and why the rest are
//! dropped.

use super::*;

fn svg(body: &str) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="48">{body}</svg>"#
    )
}

const SQUARE: &str = r#"<path d="M2 2h20v20H2z"/>"#;

#[test]
fn an_icon_is_rebuilt_from_the_allowlist() {
    for (name, body, kept) in [
        (
            "currentColor and the root's size dropped",
            r#"<path d="M2 2h20v20H2z" fill="currentColor"/>"#,
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M2 2h20v20H2z" fill="currentColor"></path></svg>"#,
        ),
        (
            "a local gradient",
            r##"<defs><linearGradient id="g"><stop offset="0" stop-color="#f00"/></linearGradient></defs><rect width="20" height="20" fill="url(#g)"/>"##,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><defs><linearGradient id="g"><stop offset="0" stop-color="#f00"></stop></linearGradient></defs><rect width="20" height="20" fill="url(#g)"></rect></svg>"##,
        ),
        (
            "editor metadata, text, comments and unknown attributes dropped",
            r#"<!-- made in an editor --><metadata>m</metadata><text>A</text><g data-name="x" class="c"><circle cx="12" cy="12" r="8" fill="rgb(10, 20, 30)"/></g>"#,
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><g><circle cx="12" cy="12" r="8" fill="rgb(10, 20, 30)"></circle></g></svg>"#,
        ),
        (
            "a title kept as text",
            r#"<title>Lific &amp; co</title><path d="M2 2h20v20H2z" fill="red"/>"#,
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><title>Lific &amp; co</title><path d="M2 2h20v20H2z" fill="red"></path></svg>"#,
        ),
        (
            "a quote in a value stays inside the attribute",
            r#"<path d="M2 2h20v20H2z&quot; onload=&quot;x" fill="red"/>"#,
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M2 2h20v20H2z&quot; onload=&quot;x" fill="red"></path></svg>"#,
        ),
        (
            "markup in a title stays text",
            r#"<title>&lt;script&gt;alert(1)&lt;/script&gt;</title><path d="M2 2h20v20H2z"/>"#,
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><title>&lt;script&gt;alert(1)&lt;/script&gt;</title><path d="M2 2h20v20H2z"></path></svg>"#,
        ),
        (
            "a rejected element inside a dropped one is dropped with it",
            r#"<metadata><script>alert(1)</script></metadata><path d="M2 2h20v20H2z"/>"#,
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M2 2h20v20H2z"></path></svg>"#,
        ),
    ] {
        assert_eq!(sanitize_icon(&svg(body)).as_deref(), Ok(kept), "{name}");
    }
}

#[test]
fn an_icon_that_could_reach_outside_itself_is_dropped() {
    let cases: Vec<(&str, String, &str)> = vec![
        ("a script", svg(&format!("<script>alert(1)</script>{SQUARE}")), "it contains a <script> element"),
        ("a foreign object", svg("<foreignObject><div/></foreignObject>"), "it contains a <foreignObject> element"),
        ("an image", svg(r#"<image href="https://x/y.png"/>"#), "it contains a <image> element"),
        ("a use", svg(r##"<use href="#a"/>"##), "it contains a <use> element"),
        ("a style element", svg("<style>path{}</style>"), "it contains a <style> element"),
        ("a filter", svg("<filter/>"), "it contains a <filter> element"),
        ("a mask", svg("<mask/>"), "it contains a <mask> element"),
        ("an animation", svg(r#"<animateTransform attributeName="transform"/>"#), "it contains a <animateTransform> element"),
        ("a style attribute", svg(r#"<path d="M2 2h20v20H2z" style="fill:red"/>"#), "it has a style attribute"),
        ("an event handler", svg(r#"<path d="M2 2h20v20H2z" onclick="x()"/>"#), "it has a onclick attribute"),
        (
            "an xlink href",
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 24 24"><path xlink:href="https://x" d="M2 2h20v20H2z"/></svg>"#.to_string(),
            "it has a href attribute",
        ),
        (
            "an outside paint",
            svg(r#"<path d="M2 2h20v20H2z" fill="url(https://x/p.svg#g)"/>"#),
            "its fill is \"url(https://x/p.svg#g)\", which an icon can't use",
        ),
        (
            "a url in a geometry value",
            svg(r#"<path d="M2 2h20v20H2z" transform="url(#t)"/>"#),
            "its transform is \"url(#t)\", which an icon can't use",
        ),
        (
            "a paint that is not a color",
            svg(r#"<path d="M2 2h20v20H2z" fill="expression(alert(1))"/>"#),
            "its fill is \"expression(alert(1))\", which an icon can't use",
        ),
        (
            "a doctype",
            format!(r#"<!DOCTYPE svg [<!ENTITY a "aaaa">]>{}"#, svg(SQUARE)),
            "it declares a DOCTYPE",
        ),
        ("too large", svg(&SQUARE.repeat(800)), "it is larger than 16 KB"),
        ("too many elements", svg(&"<g/>".repeat(501)), "it has more than 500 elements"),
        ("nested too deep", svg(&format!("{}{SQUARE}{}", "<g>".repeat(17), "</g>".repeat(17))), "it nests elements deeper than 16"),
        ("no viewBox", r#"<svg xmlns="http://www.w3.org/2000/svg"><path d="M2 2h20v20H2z"/></svg>"#.to_string(), "it has no usable viewBox"),
        ("an empty viewBox", r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 0 24"><path d="M2 2h20v20H2z"/></svg>"#.to_string(), "it has no usable viewBox"),
        ("a viewBox with more than numbers", r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24 junk"><path d="M2 2h20v20H2z"/></svg>"#.to_string(), "it has no usable viewBox"),
        (
            "an outside clip path",
            svg(r#"<path d="M2 2h20v20H2z" clip-path="url(https://x#a)"/>"#),
            "its clip-path is \"url(https://x#a)\", which an icon can't use",
        ),
        (
            "an id that isn't a name",
            svg(r#"<path id="a b" d="M2 2h20v20H2z"/>"#),
            "its id is \"a b\", which an icon can't use",
        ),
        (
            "a long refused value quoted short",
            svg(&format!(r#"<path d="M2 2h20v20H2z" fill="expression({})"/>"#, "a".repeat(5000))),
            "its fill is \"expression(aaaaaaaaaaaaaaaaaaaaaaaaaaaaa…\", which an icon can't use",
        ),
        (
            "a long refused element name quoted short",
            svg(&format!("<animate{}/>", "a".repeat(5000))),
            "it contains a <animateaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa…> element",
        ),
        (
            "a long refused attribute name quoted short",
            svg(&format!(r#"<path d="M2 2h20v20H2z" on{}="x()"/>"#, "a".repeat(5000))),
            "it has a onaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa… attribute",
        ),
        ("not SVG", r#"<html viewBox="0 0 24 24"/>"#.to_string(), "its root element isn't an SVG <svg>"),
        ("no namespace", r#"<svg viewBox="0 0 24 24"><path d="M2 2h20v20H2z"/></svg>"#.to_string(), "its root element isn't an SVG <svg>"),
        ("nothing drawn", svg("<g><path d=\"\"/></g>"), "it draws nothing"),
    ];
    for (name, icon, why) in cases {
        let answer = sanitize_icon(&icon);
        assert_eq!(answer, Err(why.to_string()), "{name}");
    }
    assert!(sanitize_icon("not xml <")
        .unwrap_err()
        .starts_with("it isn't valid XML"));
    let long_tag = sanitize_icon(&svg(&format!("<{}></g>", "a".repeat(5000)))).unwrap_err();
    assert!(
        long_tag.starts_with("it isn't valid XML (expected 'aaaa"),
        "{long_tag}"
    );
    assert!(long_tag.ends_with("aaa…)"), "{long_tag}");
    assert!(long_tag.chars().count() < 150, "{long_tag}");
}
