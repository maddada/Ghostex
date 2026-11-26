//! Where `ghostex_click`'s mouse press would land, worked out through DevTools' DOM domain, which
//! the page's own script cannot see or change.
//!
//! CDXC:Browser 2026-11-26 DECISION:
//! Coordinator, on the user's behalf (re-check of #762, N2): the checks that decide whether a real press may go to an element ran in the page's own script world, so a hostile page could answer them falsely (patching `elementFromPoint`, replacing `HTMLIFrameElement`, hiding a frame in a closed shadow root) or time a swap to the moment between the check and the press, and land one trusted click in a framed site (a like, follow, sign-in or pay button). The press is now decided here instead: the element is found with `DOM.querySelector`, its centre comes from `DOM.getContentQuads`, and the node really at that point comes from `DOM.getNodeForLocation`, which hit-tests the page's layout through every shadow root, open or closed. The press goes only when that node is in the page itself (the main frame), is not a frame, object or embed, and is the element or inside it (its whole subtree, shadow roots included, from `DOM.describeNode`). After the mouse moves there the same check runs again, after a short random wait, as one DevTools call, and the press follows at once; no page function runs between the move and the press, so the page cannot tell when to swap. The check runs once more while the button is down: when something else came under the pointer in that last gap, the button is released at the far corner of the view. A frame that took the press then gets a mousedown but no click (its document never sees the release on what it pressed); a press that landed on the page's own element gives the page a trusted click on the nearest parent the two points share (often `body` or `html`), which is the page's own and runs its own handlers, such as one closing a menu. A page could still win by sliding a frame in for the press and out again before that check; that takes two swaps each timed to within a DevTools round trip. Anything else gets the script click, as before. The page script still scrolls the element into view, names a file field and reports which element took the press, but none of that decides the press.
//! Coordinator, on the user's behalf (re-check 4 of #762, S1): the press point is in the view's coordinates and `DOM.getNodeForLocation` takes the document's, so on a scrolled page the checks looked above where the press landed (a frame there got a trusted click, and buttons below the fold lost their real press). Each check now reads the view's scroll offset fresh (`Page.getLayoutMetrics`' `cssVisualViewport`) and adds it to the point; the press is refused when the page scrolled since the point was worked out, or is pinch-zoomed.
//! Coordinator, on the user's behalf (re-check 5 of #762, S2): the hit test takes whole pixels while the press lands at the exact point, so on a 2x screen a view scrolled by half a pixel let a frame just below the tested point take the press. A view scrolled by part of a pixel is refused, and the point script rounds the scroll after `scrollIntoView` so honest pages keep their real click. After a press on a page is refused because the page scrolled or something came under the pointer, that page gets the script click for a minute (`SCRIPT_CLICKS_AFTER_REFUSAL`, mcp_tools.rs), since such an attack needs many tries and an honest page almost never trips it.
//! Coordinator, on the user's behalf (re-check 6 of #762, R1 to R3): Chromium keeps scroll offsets on whole device pixels, so at 125% and 150% a whole CSS offset is not always reachable; the point script rounds to an offset whole in both (a multiple of the smallest step that is), and steps back towards the start when the page's end clamps it. Only a refusal that may be a race (a frame, nothing, or a scroll under the pointer, `may_be_a_race`) starts the minute of script clicks; another element of the page itself (a menu's backdrop, a button drawn again on hover) does not, and a frame outside the element is told from another element by its node name.
//! SEE-ALSO: server/src/server/cef_devtools_scope.rs (the DOM commands a session's tools may send), server/src/ghostex_cli/browser_mcp/page_scripts.rs (`GHOSTEX_CLICK_POINT_SCRIPT`).

use super::*;
use crate::ghostex_cli::rpc::CliResult;
use serde_json::{json, Value};
use std::collections::HashSet;

/// The node names of elements that show another document: a press on one goes to that document.
const FRAME_OWNERS: &[&str] = &[
    "IFRAME",
    "FRAME",
    "OBJECT",
    "EMBED",
    "FENCEDFRAME",
    "PORTAL",
];

/// The element a click names, as DevTools sees it.
pub(super) struct ClickTarget {
    /// The page's main frame: a hit in another frame is not the element.
    main_frame: String,
    /// The element and everything inside it, shadow roots included, frames' documents not.
    subtree: HashSet<i64>,
    /// The frames, objects and embeds in `subtree`: a press on one goes to another document.
    frame_owners: HashSet<i64>,
    /// Whether the element is, holds or names a file upload field.
    pub(super) file_field: bool,
    /// The point the press goes to, in the view's CSS pixels; `None` when the element has no box
    /// inside the view.
    pub(super) point: Option<(i64, i64)>,
    /// The view's size in CSS pixels.
    view: (f64, f64),
    /// Where the view stood in the page when `point` was worked out.
    offset: Option<ViewOffset>,
}

/// Where the view stands in the page: how far it is scrolled, in CSS pixels, and its pinch-zoom
/// scale (`Page.getLayoutMetrics`' `cssVisualViewport`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ViewOffset {
    page_x: f64,
    page_y: f64,
    scale: f64,
}

/// The refusal for a view that scrolled between the checks.
pub(super) const PAGE_SCROLLED: &str = "The page scrolled";
/// The refusal for a view scrolled by part of a pixel.
const PART_PIXEL: &str = "The page is scrolled by part of a pixel";
/// The refusal for a frame, object or embed at the point (or a node whose name could not be read).
const FRAME_COVERS: &str = "A frame covers the element's centre";
/// The refusal for nothing of the page at the point.
const NOTHING_THERE: &str = "Nothing in the page is at the element's centre";
/// The refusal for another element of the page itself at the point.
const ANOTHER_ELEMENT: &str = "Another element covers the element's centre";

/// Whether a refusal may be a page racing the checks, which starts the minute of script clicks
/// (mcp_tools.rs): a frame, nothing, or a scroll came under the pointer. Another element of the page
/// itself does not (a menu's backdrop, a button drawn again on hover): a press on the page's own
/// element gains an attacker nothing.
pub(super) fn may_be_a_race(refusal: &str) -> bool {
    [FRAME_COVERS, NOTHING_THERE, PAGE_SCROLLED, PART_PIXEL].contains(&refusal)
}

/// The view's offset in a `Page.getLayoutMetrics` answer.
fn view_offset(metrics: &Value) -> Option<ViewOffset> {
    let viewport = metrics
        .get("cssVisualViewport")
        .or_else(|| metrics.get("visualViewport"))?;
    Some(ViewOffset {
        page_x: viewport.get("pageX")?.as_f64()?,
        page_y: viewport.get("pageY")?.as_f64()?,
        scale: viewport.get("scale").and_then(Value::as_f64).unwrap_or(1.0),
    })
}

/// The point of the page's document that `point` of the view shows now, which is what
/// `DOM.getNodeForLocation` takes (the mouse events take the view's point): `point` plus how far
/// the view is scrolled. Refused when the view scrolled since the point was worked out (`then`),
/// so the press would land somewhere else, or is pinch-zoomed, or either is unknown.
pub(super) fn document_point(
    point: (i64, i64),
    then: Option<ViewOffset>,
    now: Option<ViewOffset>,
) -> Result<(i64, i64), &'static str> {
    let (Some(then), Some(now)) = (then, now) else {
        return Err("The page's scroll position is unknown");
    };
    if (then.scale - 1.0).abs() > 0.001 || (now.scale - 1.0).abs() > 0.001 {
        return Err("The page is zoomed in");
    }
    if (now.page_x - then.page_x).abs() > 0.5 || (now.page_y - then.page_y).abs() > 0.5 {
        return Err(PAGE_SCROLLED);
    }
    // The hit test takes whole pixels while the press lands at the exact point, so a view
    // scrolled by part of a pixel (a 2x screen) would test a point beside the press.
    let whole = |offset: f64| (offset - offset.round()).abs() < 0.001;
    if !whole(now.page_x) || !whole(now.page_y) {
        return Err(PART_PIXEL);
    }
    Ok((
        (point.0 as f64 + now.page_x).round() as i64,
        (point.1 as f64 + now.page_y).round() as i64,
    ))
}

impl ClickTarget {
    /// The corner of the view farthest from `point`, where a press that went wrong is released so
    /// it does not become a click on what took it.
    pub(super) fn far_corner(&self, point: (i64, i64)) -> (i64, i64) {
        let (width, height) = self.view;
        let far = |at: i64, size: f64| {
            if (at as f64) < size / 2.0 {
                (size - 1.0).max(0.0) as i64
            } else {
                0
            }
        };
        (far(point.0, width), far(point.1, height))
    }
}

/// Why a press at the target's point would not reach it, or `None` when it would. A frame owner
/// outside the element is "another element"; one inside it is known from its subtree.
pub(super) fn hit_refusal(
    target: &ClickTarget,
    hit_backend_node: Option<i64>,
    hit_frame: Option<&str>,
) -> Option<&'static str> {
    let Some(hit) = hit_backend_node else {
        return Some(NOTHING_THERE);
    };
    if target.main_frame.is_empty() || hit_frame != Some(target.main_frame.as_str()) {
        return Some(FRAME_COVERS);
    }
    if target.frame_owners.contains(&hit) {
        return Some(FRAME_COVERS);
    }
    if !target.subtree.contains(&hit) {
        return Some(ANOTHER_ELEMENT);
    }
    None
}

/// The element `selector` names, its subtree and its centre, all from DevTools; `None` when no
/// element matches.
pub(super) fn click_target(
    state: &mut McpState,
    page_id: &str,
    selector: &str,
) -> CliResult<Option<ClickTarget>> {
    let document = client_call(state, page_id, "DOM.getDocument", json!({ "depth": 0 }))?;
    let Some(root) = document.pointer("/root/nodeId").and_then(Value::as_i64) else {
        return Ok(None);
    };
    let found = client_call(
        state,
        page_id,
        "DOM.querySelector",
        json!({ "nodeId": root, "selector": selector }),
    )?;
    let Some(node_id) = found
        .get("nodeId")
        .and_then(Value::as_i64)
        .filter(|id| *id > 0)
    else {
        return Ok(None);
    };
    let frames = client_call(state, page_id, "Page.getFrameTree", json!({}))?;
    let main_frame = frames
        .pointer("/frameTree/frame/id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let described = client_call(
        state,
        page_id,
        "DOM.describeNode",
        json!({ "nodeId": node_id, "depth": -1, "pierce": true }),
    )?;
    let mut found = Subtree::default();
    if let Some(node) = described.get("node") {
        collect_subtree(node, &mut found);
    }
    let (point, view, offset) = element_point(state, page_id, node_id)?;
    Ok(Some(ClickTarget {
        main_frame,
        subtree: found.nodes,
        frame_owners: found.frame_owners,
        file_field: found.file_field,
        point,
        view,
        offset,
    }))
}

/// What `collect_subtree` found under the element.
#[derive(Default)]
struct Subtree {
    nodes: HashSet<i64>,
    frame_owners: HashSet<i64>,
    file_field: bool,
}

/// Adds `node` and what it holds (children, shadow roots, pseudo elements) to `found`, but not
/// the documents of frames inside it, and notes frame owners and a file upload field.
fn collect_subtree(node: &Value, found: &mut Subtree) {
    let name = node
        .get("nodeName")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if let Some(id) = node.get("backendNodeId").and_then(Value::as_i64) {
        found.nodes.insert(id);
        if FRAME_OWNERS.contains(&name.to_ascii_uppercase().as_str()) {
            found.frame_owners.insert(id);
        }
    }
    if name.eq_ignore_ascii_case("INPUT")
        && attribute(node, "type").is_some_and(|kind| kind.eq_ignore_ascii_case("file"))
    {
        found.file_field = true;
    }
    for list in ["children", "shadowRoots", "pseudoElements"] {
        for child in node
            .get(list)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            collect_subtree(child, found);
        }
    }
}

/// The value of attribute `name` of a described node (`attributes` is `[name, value, ...]`).
fn attribute<'a>(node: &'a Value, name: &str) -> Option<&'a str> {
    let attributes = node.get("attributes")?.as_array()?;
    attributes
        .chunks(2)
        .find(|pair| pair.first().and_then(Value::as_str) == Some(name))
        .and_then(|pair| pair.get(1))
        .and_then(Value::as_str)
}

/// A press point (see `ClickTarget::point`), the view's size and where it stood in the page.
type PointAndView = (Option<(i64, i64)>, (f64, f64), Option<ViewOffset>);

/// The centre of the element's first box with an area, inside the view, in whole CSS pixels, the
/// view's size, and where the view stood in the page.
fn element_point(state: &mut McpState, page_id: &str, node_id: i64) -> CliResult<PointAndView> {
    let quads = client_call(
        state,
        page_id,
        "DOM.getContentQuads",
        json!({ "nodeId": node_id }),
    )
    .ok();
    let metrics = client_call(state, page_id, "Page.getLayoutMetrics", json!({}))?;
    let viewport = metrics
        .get("cssLayoutViewport")
        .or_else(|| metrics.get("layoutViewport"));
    let size = |key: &str| {
        viewport
            .and_then(|viewport| viewport.get(key))
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
    };
    let quads: Vec<Vec<f64>> = quads
        .as_ref()
        .and_then(|quads| quads.get("quads"))
        .and_then(Value::as_array)
        .map(|quads| {
            quads
                .iter()
                .filter_map(|quad| {
                    quad.as_array()
                        .map(|points| points.iter().filter_map(Value::as_f64).collect())
                })
                .collect()
        })
        .unwrap_or_default();
    let view = (size("clientWidth"), size("clientHeight"));
    Ok((
        quad_centre(&quads, view.0, view.1),
        view,
        view_offset(&metrics),
    ))
}

/// The centre of the first quad with an area of at least one pixel, when it lies in a view of
/// `width` by `height`.
pub(super) fn quad_centre(quads: &[Vec<f64>], width: f64, height: f64) -> Option<(i64, i64)> {
    quads.iter().find_map(|quad| {
        if quad.len() != 8 {
            return None;
        }
        let xs = [quad[0], quad[2], quad[4], quad[6]];
        let ys = [quad[1], quad[3], quad[5], quad[7]];
        // The shoelace formula.
        let area = (0..4)
            .map(|i| xs[i] * ys[(i + 1) % 4] - xs[(i + 1) % 4] * ys[i])
            .sum::<f64>()
            .abs()
            / 2.0;
        if area < 1.0 {
            return None;
        }
        let x = (xs.iter().sum::<f64>() / 4.0).floor();
        let y = (ys.iter().sum::<f64>() / 4.0).floor();
        (x >= 0.0 && y >= 0.0 && x < width && y < height).then_some((x as i64, y as i64))
    })
}

/// Why a press at `point` (in the view) would not reach `target` now, or `None` when it would. The
/// view's scroll offset is read fresh each time (the page can scroll between checks), then the
/// node at that point of the document; the hit test is the last call, so the press can follow it
/// closely.
pub(super) fn press_refusal(
    state: &mut McpState,
    page_id: &str,
    target: &ClickTarget,
    point: (i64, i64),
) -> CliResult<Option<&'static str>> {
    let now = client_call(state, page_id, "Page.getLayoutMetrics", json!({}))
        .ok()
        .and_then(|metrics| view_offset(&metrics));
    let (x, y) = match document_point(point, target.offset, now) {
        Ok(at) => at,
        Err(why) => return Ok(Some(why)),
    };
    let Ok(hit) = client_call(
        state,
        page_id,
        "DOM.getNodeForLocation",
        json!({
            "x": x,
            "y": y,
            "includeUserAgentShadowDOM": false,
            "ignorePointerEventsNone": false,
        }),
    ) else {
        return Ok(Some(NOTHING_THERE));
    };
    let backend = hit.get("backendNodeId").and_then(Value::as_i64);
    let refusal = hit_refusal(target, backend, hit.get("frameId").and_then(Value::as_str));
    if refusal != Some(ANOTHER_ELEMENT) {
        return Ok(refusal);
    }
    // Outside the element: a frame there is a frame, not "another element" (only on the refusal
    // path, so the press is not delayed). A name that cannot be read counts as a frame.
    let name = client_call(
        state,
        page_id,
        "DOM.describeNode",
        json!({ "backendNodeId": backend, "depth": 0 }),
    )
    .ok()
    .and_then(|described| {
        described
            .pointer("/node/nodeName")
            .and_then(Value::as_str)
            .map(str::to_ascii_uppercase)
    });
    Ok(Some(match name {
        Some(name) if !FRAME_OWNERS.contains(&name.as_str()) => ANOTHER_ELEMENT,
        _ => FRAME_COVERS,
    }))
}

/// A short wait of 40 to 160 ms, different each time, between the mouse move and the last check,
/// so a page cannot time a swap to land after the check and before the press.
pub(super) fn settle_after_move() {
    let jitter = (uuid::Uuid::new_v4().as_u128() % 121) as u64;
    std::thread::sleep(std::time::Duration::from_millis(40 + jitter));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(subtree: &[i64]) -> ClickTarget {
        ClickTarget {
            main_frame: "MAIN".to_string(),
            subtree: subtree.iter().copied().collect(),
            frame_owners: [7].into_iter().collect(),
            file_field: false,
            point: Some((10, 10)),
            view: (800.0, 600.0),
            offset: None,
        }
    }

    #[test]
    fn a_press_goes_only_to_the_element_in_the_page_itself() {
        let button = target(&[5, 6, 7]);
        assert_eq!(hit_refusal(&button, Some(6), Some("MAIN")), None);
        assert_eq!(hit_refusal(&button, Some(5), Some("MAIN")), None);
        assert!(hit_refusal(&button, Some(5), None).is_some());
        // A frame in the element, open or in a closed shadow root: its owner is hit.
        assert!(hit_refusal(&button, Some(7), Some("MAIN")).is_some());
        // A node of a same-site frame's own document.
        assert!(hit_refusal(&button, Some(6), Some("CHILD")).is_some());
        // Another element on top (a frame outside the element too).
        assert!(hit_refusal(&button, Some(9), Some("MAIN")).is_some());
        assert!(hit_refusal(&button, None, None).is_some());
    }

    #[test]
    fn the_hit_test_looks_where_the_press_lands_on_a_scrolled_page() {
        let at = |page_x, page_y, scale| {
            Some(ViewOffset {
                page_x,
                page_y,
                scale,
            })
        };
        // Not scrolled: the same point.
        assert_eq!(
            document_point((100, 100), at(0.0, 0.0, 1.0), at(0.0, 0.0, 1.0)),
            Ok((100, 100))
        );
        // Scrolled 200 down (re-check 4, S1): the press at y 100 of the view hits y 300 of the page.
        assert_eq!(
            document_point((100, 100), at(0.0, 200.0, 1.0), at(0.0, 200.0, 1.0)),
            Ok((100, 300))
        );
        assert_eq!(
            document_point((60, 480), at(30.0, 2700.0, 1.0), at(30.0, 2700.0, 1.0)),
            Ok((90, 3180))
        );
        // Scrolled by half a pixel (re-check 5, S2, a 2x screen): the hit test cannot look exactly
        // where the press lands, so no press.
        assert!(document_point((60, 480), at(0.0, 200.5, 1.0), at(0.0, 200.5, 1.0)).is_err());
        assert!(document_point((60, 480), at(0.25, 200.0, 1.0), at(0.25, 200.0, 1.0)).is_err());
        // Scrolled since the point was worked out, pinch-zoomed, or unknown: no press.
        assert!(document_point((100, 100), at(0.0, 200.0, 1.0), at(0.0, 260.0, 1.0)).is_err());
        assert!(document_point((100, 100), at(0.0, 0.0, 1.5), at(0.0, 0.0, 1.5)).is_err());
        assert!(document_point((100, 100), None, at(0.0, 0.0, 1.0)).is_err());
        assert!(document_point((100, 100), at(0.0, 0.0, 1.0), None).is_err());
        // The offset comes from the visual viewport, in CSS pixels.
        let metrics = json!({ "cssVisualViewport": { "pageX": 0, "pageY": 700, "scale": 1 } });
        assert_eq!(view_offset(&metrics), at(0.0, 700.0, 1.0));
    }

    #[test]
    fn only_a_frame_nothing_or_a_scroll_starts_the_minute() {
        for race in [FRAME_COVERS, NOTHING_THERE, PAGE_SCROLLED, PART_PIXEL] {
            assert!(may_be_a_race(race), "{race}");
        }
        for honest in [
            ANOTHER_ELEMENT,
            "The page is zoomed in",
            "The page's scroll position is unknown",
        ] {
            assert!(!may_be_a_race(honest), "{honest}");
        }
    }

    #[test]
    fn a_press_that_went_wrong_is_released_far_away() {
        let button = target(&[5]);
        assert_eq!(button.far_corner((10, 10)), (799, 599));
        assert_eq!(button.far_corner((700, 500)), (0, 0));
    }

    #[test]
    fn the_subtree_takes_shadow_roots_but_not_frame_documents() {
        let described = json!({
            "backendNodeId": 1, "nodeName": "DIV", "attributes": [],
            "shadowRoots": [{ "backendNodeId": 2, "nodeName": "#document-fragment", "children": [
                { "backendNodeId": 3, "nodeName": "IFRAME", "attributes": ["src", "https://x"],
                  "contentDocument": { "backendNodeId": 4, "nodeName": "#document", "children": [
                      { "backendNodeId": 5, "nodeName": "BUTTON" } ] } },
                { "backendNodeId": 6, "nodeName": "INPUT", "attributes": ["type", "FILE"] }
            ] }],
            "children": [{ "backendNodeId": 7, "nodeName": "SPAN" }]
        });
        let mut found = Subtree::default();
        collect_subtree(&described, &mut found);
        assert_eq!(found.nodes, [1, 2, 3, 6, 7].into_iter().collect());
        assert_eq!(found.frame_owners, [3].into_iter().collect());
        assert!(found.file_field);
    }

    #[test]
    fn the_press_point_is_a_boxs_centre_inside_the_view() {
        let quad = vec![10.0, 20.0, 110.0, 20.0, 110.0, 60.0, 10.0, 60.0];
        assert_eq!(quad_centre(&[quad.clone()], 800.0, 600.0), Some((60, 40)));
        assert_eq!(quad_centre(&[quad], 50.0, 600.0), None);
        let flat = vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        assert_eq!(quad_centre(&[flat], 800.0, 600.0), None);
    }
}
