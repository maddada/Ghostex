//! `ghostex visual check <page.html>`: opens a page the way `ghostex show` would publish it in a
//! headless Chrome at a phone's width, a large phone's or tablet's, a foldable's and a computer's,
//! and again with large text, saves a screenshot of each and says what does not fit, so an agent
//! sees its page on a phone before the user does. `ghostex show` runs the narrow half (no
//! screenshots) after publishing and prints what it found.
//!
//! CDXC:SessionChat 2026-10-28 WHY:
//! The page kit (server/src/visual_pages/fit.rs) fits a too-wide page to the phone, but a page that needs fitting still reads worse than one written for a phone: a stacked comparison loses its side-by-side, a scrolled mockup hides its right half. The check reports what the kit had to fit (`window.__ghostexFit`) and anything still past the screen's edge, so the agent fixes its CSS before sending. Chrome is driven over the DevTools protocol because a headless Chrome window cannot be narrower than 500px, and the phone's width only comes from device emulation.
//! CDXC:SessionChat 2026-11-15 DECISION:
//! User, with a screenshot of a page on their Android foldable (two option cards side by side where they did not fit, mockups cut at their card's edge, stat boxes drawn over each other, the answered pill over a card): "Please also make sure inline htmls don't look like this where stuff is over flowing and overlapping." That page had passed the check at 390 and 1280px, but the foldable's screen is about 750 to 900 CSS px with large text, a width neither run saw. So the check now opens every page at 360, 600, 900 and 1280px and again at 360 and 900px with text at 130% (Android's font size setting scales text, not boxes), and inspects the page itself rather than only what the kit fitted: a box whose own content spills past its edge or is cut off by `overflow:hidden` without an ellipsis, content a sideways scroller cannot scroll back to, text drawn over other text, and fixed boxes over text. (2026-11-15: the answered pill shows always, the user's "Always show", so it is checked only with the page scrolled to its end, where it must cover no text or control.) Each problem names its CSS selector and the width it showed at.
//! SEE-ALSO: server/src/visual_pages/fit.rs (`window.__ghostexFit`), server/src/visual_pages/kit.rs (the safe layout defaults), server/src/ghostex_cli/visual_guide.md (tells agents to run the check).

use std::{
    io::{Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde_json::{json, Value};

use crate::ghostex_cli::rpc::{CliError, CliResult};
use crate::visual_pages::prepare_page;

/// The narrowest phone width pages are checked at: small Android phones, the bottom of the 360–430px
/// range phones have.
pub(crate) const PHONE_WIDTH: u32 = 360;
/// A large phone held sideways, a small tablet, or the Ghostex window narrowed on a computer.
pub(crate) const SMALL_TABLET_WIDTH: u32 = 600;
/// A foldable phone opened, or a tablet: wide enough for two columns that may not fit.
pub(crate) const FOLDABLE_WIDTH: u32 = 900;
pub(crate) const DESKTOP_WIDTH: u32 = 1280;
/// Android's large font size setting (and iOS's larger text) scales text, not boxes; 130% is a
/// common large setting.
pub(crate) const LARGE_TEXT_SCALE: f64 = 1.3;
/// What `ghostex visual check page.html` opens a page at: each width at the normal text size, then
/// the phone and the foldable with large text.
pub(crate) const DEFAULT_CHECKS: [(u32, f64); 6] = [
    (PHONE_WIDTH, 1.0),
    (SMALL_TABLET_WIDTH, 1.0),
    (FOLDABLE_WIDTH, 1.0),
    (DESKTOP_WIDTH, 1.0),
    (PHONE_WIDTH, LARGE_TEXT_SCALE),
    (FOLDABLE_WIDTH, LARGE_TEXT_SCALE),
];
/// What `ghostex show` checks after publishing: the narrow screens, where pages break.
pub(crate) const SHOW_CHECKS: [(u32, f64); 3] = [
    (PHONE_WIDTH, 1.0),
    (SMALL_TABLET_WIDTH, 1.0),
    (FOLDABLE_WIDTH, LARGE_TEXT_SCALE),
];
/// Widths at or below this are emulated as a phone or tablet (meta viewport, touch, a dense screen).
const MOBILE_MAX_WIDTH: u32 = 1024;
/// The tallest single screenshot, in CSS pixels: a longer page is saved in parts of this height
/// (`page-360.png`, `page-360-part2.png`, …), each under Chrome's largest picture at a phone's
/// double-density screen.
const SCREENSHOT_PART_HEIGHT: f64 = 6000.0;
/// The most of a page screenshotted, in CSS pixels; anything longer is cut there.
const SCREENSHOT_MAX_HEIGHT: f64 = 48000.0;
const CHROME_START_TIMEOUT: Duration = Duration::from_secs(20);
const CDP_TIMEOUT: Duration = Duration::from_secs(20);

/// Whether `ghostex visual check` was given a page rather than a block's JSON.
pub(crate) fn is_page_file(file: &str) -> bool {
    let lower = file.to_ascii_lowercase();
    lower.ends_with(".html") || lower.ends_with(".htm")
}

pub(crate) struct PageCheck {
    pub(crate) width: u32,
    /// How much larger than the page's own sizes text is drawn (1.0 as written).
    pub(crate) text_scale: f64,
    pub(crate) light: bool,
    pub(crate) screenshot: Option<PathBuf>,
}

/// What one width showed: the kit's fit report, what the page itself got wrong, and its size.
pub(crate) struct PageCheckResult {
    pub(crate) width: u32,
    pub(crate) text_scale: f64,
    pub(crate) report: Value,
    /// The page's screenshot, top to bottom: one file, or several parts for a long page.
    pub(crate) screenshots: Vec<PathBuf>,
}

/// "Phone, 360px wide", "Foldable or tablet, 900px wide, text at 130%".
pub(crate) fn screen_label(width: u32, text_scale: f64) -> String {
    let kind = if width <= 480 {
        "Phone"
    } else if width <= 700 {
        "Large phone or small tablet"
    } else if width <= MOBILE_MAX_WIDTH {
        "Foldable or tablet"
    } else {
        "Computer"
    };
    let text = if is_large_text(text_scale) {
        format!(", text at {:.0}%", text_scale * 100.0)
    } else {
        String::new()
    };
    format!("{kind}, {width}px wide{text}")
}

fn is_large_text(text_scale: f64) -> bool {
    (text_scale - 1.0).abs() > 0.001
}

/// `ghostex visual check <page.html> [--width N] [--text-scale P] [--light] [--out file.png]`.
/// Ends with an error when anything overflows or overlaps, so a script can rely on it.
pub(crate) fn check_page_command(
    file: &str,
    width: Option<u32>,
    text_scale: Option<f64>,
    light: bool,
    out: Option<PathBuf>,
) -> CliResult<()> {
    let path = absolute(file);
    let runs: Vec<(u32, f64)> = match (width, text_scale) {
        (Some(width), scale) => vec![(width, scale.unwrap_or(1.0))],
        (None, Some(scale)) => [
            PHONE_WIDTH,
            SMALL_TABLET_WIDTH,
            FOLDABLE_WIDTH,
            DESKTOP_WIDTH,
        ]
        .into_iter()
        .map(|width| (width, scale))
        .collect(),
        (None, None) => DEFAULT_CHECKS.to_vec(),
    };
    let several = runs.len() > 1;
    let checks = runs
        .iter()
        .map(|&(width, text_scale)| PageCheck {
            width,
            text_scale,
            light,
            screenshot: Some(screenshot_path(
                &path,
                out.as_deref(),
                width,
                text_scale,
                several,
            )),
        })
        .collect::<Vec<_>>();
    let results = check_page(&path, &checks)?;
    let mut broken = 0;
    for result in &results {
        let shot = if result.screenshots.is_empty() {
            String::new()
        } else {
            format!(
                ": {}",
                result
                    .screenshots
                    .iter()
                    .map(|shot| shot.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        println!(
            "{} ({} theme){shot}",
            screen_label(result.width, result.text_scale),
            if light { "light" } else { "dark" }
        );
        let problems = problem_findings(&result.report);
        let findings = page_findings(result.width, &result.report);
        let notes = kit_notes(&result.report);
        if problems.is_empty() && findings.is_empty() {
            println!("  Fits: nothing overflows or overlaps, and the kit had nothing to fix.");
        }
        if !problems.is_empty() {
            broken += 1;
        }
        for finding in problems.iter().chain(findings.iter()).chain(notes.iter()) {
            println!("  - {finding}");
        }
    }
    // The Pick buttons do not depend on the width, so they are said once.
    let mut repeated = 0;
    if let Some(result) = results.first() {
        let picks = pick_findings(&result.report);
        if !picks.is_empty() {
            println!("Pick buttons:");
            for finding in picks {
                println!("  - {finding}");
            }
        }
        let lists = repeated_findings(&result.report);
        repeated = lists.len();
        if !lists.is_empty() {
            println!("Repeated lists:");
            for finding in lists {
                println!("  - {finding}");
            }
        }
    }
    println!("Look at each screenshot before sending the page.");
    if repeated > 0 {
        return Err(CliError::Other(format!(
            "{repeated} question{} {} the items listed above {}; put the picks on the rows and check again.",
            if repeated == 1 { "" } else { "s" },
            if repeated == 1 { "repeats" } else { "repeat" },
            if repeated == 1 { "it" } else { "them" },
        )));
    }
    if broken > 0 {
        return Err(CliError::Other(format!(
            "The page overflows or overlaps at {broken} of {} sizes; fix what is listed above and check again.",
            results.len()
        )));
    }
    Ok(())
}

/// What the check `ghostex show` runs after publishing found: the phone fit and the questions left
/// without Pick buttons.
#[derive(Default)]
pub(crate) struct ShowFindings {
    pub(crate) phone: Vec<String>,
    pub(crate) picks: Vec<String>,
}

/// The narrow-screen check `ghostex show` runs after publishing: what to fix, each finding led by
/// the screen it showed on, or nothing when the page fits or no Chrome is installed to look with.
pub(crate) fn findings_for_show(path: &Path) -> ShowFindings {
    let checks = SHOW_CHECKS
        .iter()
        .map(|&(width, text_scale)| PageCheck {
            width,
            text_scale,
            light: false,
            screenshot: None,
        })
        .collect::<Vec<_>>();
    match check_page(path, &checks) {
        Ok(results) => ShowFindings {
            phone: results
                .iter()
                .flat_map(|result| {
                    let label = screen_label(result.width, result.text_scale);
                    problem_findings(&result.report)
                        .into_iter()
                        .chain(page_findings(result.width, &result.report))
                        .map(move |finding| format!("{label}: {finding}"))
                })
                .collect(),
            picks: results
                .first()
                .map(|result| {
                    let mut picks = pick_findings(&result.report);
                    picks.extend(repeated_findings(&result.report));
                    picks
                })
                .unwrap_or_default(),
        },
        Err(_) => ShowFindings::default(),
    }
}

/// Prepares the page as `ghostex show` would and opens it once per width.
pub(crate) fn check_page(path: &Path, checks: &[PageCheck]) -> CliResult<Vec<PageCheckResult>> {
    let source = std::fs::read_to_string(path)
        .map_err(|error| CliError::Other(format!("Could not read {}: {error}", path.display())))?;
    let base_dir = path.parent().unwrap_or(Path::new("/"));
    let prepared = prepare_page(&source, base_dir).map_err(CliError::Other)?;
    let chrome = chrome_binary().ok_or_else(|| {
        CliError::Other(
            "Checking a page needs Google Chrome, Chromium, Edge or Brave; none was found. Set GHOSTEX_CHROME to the browser's program to use another one."
                .to_string(),
        )
    })?;
    let scratch = tempfile::Builder::new()
        .prefix("ghostex-page-check-")
        .tempdir()
        .map_err(|error| CliError::Other(format!("Could not make a scratch folder: {error}")))?;
    let page = scratch.path().join("page.html");
    std::fs::write(&page, prepared.html)
        .map_err(|error| CliError::Other(format!("Could not write the page to check: {error}")))?;
    let profile = scratch.path().join("profile");
    let browser = Browser::start(&chrome, &profile)?;
    let mut cdp = browser.connect()?;
    let mut results = Vec::new();
    for check in checks {
        results.push(check_width(&mut cdp, &page, check)?);
    }
    Ok(results)
}

fn check_width(cdp: &mut Cdp, page: &Path, check: &PageCheck) -> CliResult<PageCheckResult> {
    let mobile = check.width <= MOBILE_MAX_WIDTH;
    cdp.call(
        "Emulation.setDeviceMetricsOverride",
        json!({
            "width": check.width,
            "height": if mobile { 844 } else { 900 },
            "deviceScaleFactor": if mobile { 2 } else { 1 },
            "mobile": mobile,
        }),
    )?;
    cdp.call(
        "Emulation.setTouchEmulationEnabled",
        json!({ "enabled": mobile }),
    )?;
    let theme = if check.light { "light" } else { "dark" };
    let url = format!("{}#gx-theme={theme}", file_url(page));
    cdp.call("Page.navigate", json!({ "url": url }))?;
    let deadline = Instant::now() + CDP_TIMEOUT;
    loop {
        let answer = cdp.call(
            "Runtime.evaluate",
            json!({ "expression": PROBE, "awaitPromise": true, "returnByValue": true }),
        )?;
        if answer
            .pointer("/result/value/page")
            .and_then(Value::as_bool)
            == Some(true)
        {
            break;
        }
        if Instant::now() >= deadline {
            return Err(CliError::Other(
                "The page did not finish loading in Chrome.".to_string(),
            ));
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    let answer = cdp.call(
        "Runtime.evaluate",
        json!({
            "expression": format!("({INSPECT})({})", check.text_scale),
            "awaitPromise": true,
            "returnByValue": true,
        }),
    )?;
    if let Some(error) = answer.pointer("/exceptionDetails") {
        return Err(CliError::Other(format!(
            "The check could not inspect the page: {}",
            error
                .pointer("/exception/description")
                .and_then(Value::as_str)
                .unwrap_or("an error in the page")
        )));
    }
    let report = answer
        .pointer("/result/value")
        .cloned()
        .unwrap_or(Value::Null);
    let mut screenshots = Vec::new();
    if let Some(out) = &check.screenshot {
        let height = report
            .get("height")
            .and_then(Value::as_f64)
            .unwrap_or(900.0)
            .clamp(1.0, SCREENSHOT_MAX_HEIGHT);
        if let Some(parent) = out.parent().filter(|parent| !parent.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|error| {
                CliError::Other(format!("Could not create {}: {error}", parent.display()))
            })?;
        }
        let mut top = 0.0;
        while top < height {
            let part = (height - top).min(SCREENSHOT_PART_HEIGHT);
            let shot = cdp.call(
                "Page.captureScreenshot",
                json!({
                    "format": "png",
                    "captureBeyondViewport": true,
                    "clip": { "x": 0, "y": top, "width": check.width, "height": part, "scale": 1 },
                }),
            )?;
            let data = shot.get("data").and_then(Value::as_str).unwrap_or_default();
            let png = BASE64_STANDARD.decode(data).map_err(|error| {
                CliError::Other(format!("Chrome's screenshot could not be read: {error}"))
            })?;
            let file = part_path(out, screenshots.len() + 1);
            std::fs::write(&file, png).map_err(|error| {
                CliError::Other(format!("Could not write {}: {error}", file.display()))
            })?;
            screenshots.push(file);
            top += part;
        }
    }
    Ok(PageCheckResult {
        width: check.width,
        text_scale: check.text_scale,
        report,
        screenshots,
    })
}

/// Waits for the page to load and settle, then reads the kit's fit report and the page's size.
/// `page` is false while Chrome still shows the blank tab it started with.
const PROBE: &str = r#"(function(){
if(location.protocol!=="file:")return Promise.resolve({page:false});
return new Promise(function(done){
  function settle(){setTimeout(function(){requestAnimationFrame(function(){requestAnimationFrame(function(){
    var d=document.documentElement,fit=window.__ghostexFit||null,kit=window.__ghostexKit||null;
    done({page:true,fit:fit?JSON.parse(JSON.stringify(fit)):null,kit:kit?JSON.parse(JSON.stringify(kit)):null,docWidth:d.scrollWidth,viewport:d.clientWidth,height:Math.max(d.scrollHeight,document.body?document.body.scrollHeight:0)});
  });});},300);}
  if(document.readyState==="complete")settle();else addEventListener("load",settle,{once:true});
});
})()"#;

/// Run once the page has loaded (`PROBE`), as `(INSPECT)(textScale)`: draws the text larger when
/// asked (every element's font size and pixel line height times the scale, the way Android's font
/// size setting does, then a `load` event so the kit fits the page again), waits for the page to
/// settle, then answers the kit's reports, the page's size and `problems`, what the page itself
/// gets wrong:
///
/// - `spill`: a box whose own child or text reaches past its edge (the deepest box: a box whose
///   overflow comes only from a grandchild is left to the child that holds it);
/// - `clipped`: the same in a box that hides its overflow without an ellipsis, so content is cut;
/// - `unreachable`: content placed left of where a sideways scroller starts, which no scrolling
///   reaches (a centred row in an `overflow-x:auto` box);
/// - `overlap`: text drawn over other text, a fixed box included; the answered pill floats over
///   the page on purpose, so it counts only when it covers text or a control with the page
///   scrolled to its end, where the kit gives it room.
///
/// Each names the element by a CSS selector unique in the page.
const INSPECT: &str = r###"function(scale){
return new Promise(function(done){
var LIMIT=8000,SHOWN=12,SKIP=/^(SCRIPT|STYLE|TEMPLATE|NOSCRIPT|BR|WBR|HEAD|META|LINK|TITLE|OPTION|TEXTAREA|INPUT|SELECT)$/;
function svgPart(e){return typeof SVGElement!=="undefined"&&e instanceof SVGElement&&!(e instanceof SVGSVGElement);}
if(Math.abs(scale-1)>0.001&&document.body){
  var sized=[];
  [document.documentElement,document.body].concat(Array.prototype.slice.call(document.body.querySelectorAll("*"))).forEach(function(e){
    if(svgPart(e)||SKIP.test(e.tagName)&&e.tagName!=="TEXTAREA"&&e.tagName!=="INPUT"&&e.tagName!=="SELECT")return;
    var cs=getComputedStyle(e);sized.push([e,parseFloat(cs.fontSize),cs.lineHeight]);
  });
  sized.forEach(function(v){
    if(v[1])v[0].style.setProperty("font-size",(v[1]*scale)+"px","important");
    if(/px$/.test(v[2]))v[0].style.setProperty("line-height",(parseFloat(v[2])*scale)+"px","important");
  });
  dispatchEvent(new Event("load"));
}
setTimeout(function(){requestAnimationFrame(function(){requestAnimationFrame(function(){
  var d=document.documentElement,fit=window.__ghostexFit||null,kit=window.__ghostexKit||null;
  done({page:true,fit:fit?JSON.parse(JSON.stringify(fit)):null,kit:kit?JSON.parse(JSON.stringify(kit)):null,docWidth:d.scrollWidth,viewport:d.clientWidth,height:Math.max(d.scrollHeight,document.body?document.body.scrollHeight:0),problems:problems()});
});});},400);
function visible(e){return e.checkVisibility?e.checkVisibility({opacityProperty:true,visibilityProperty:true}):true;}
function classes(e){var c=e.getAttribute&&e.getAttribute("class");return c?c.trim().split(/\s+/).filter(function(x){return x&&!/^gx-(fit|more|has-)/.test(x);}):[];}
function esc(t){return window.CSS&&CSS.escape?CSS.escape(t):t;}
function part(e){
  var s=e.tagName.toLowerCase();if(e.id)return s+"#"+esc(e.id);
  var c=classes(e).slice(0,2);if(c.length)s+="."+c.map(esc).join(".");
  ["data-choice","data-tab","tag"].forEach(function(a){var v=e.getAttribute(a);if(v!==null&&v.length<40)s+="["+a+"=\""+v.replace(/["\\]/g,"\\$&")+"\"]";});
  return s;
}
function selector(e){
  var parts=[],n=e;
  while(n&&n.nodeType===1&&n!==document.documentElement&&n!==document.body){
    var p=part(n),parent=n.parentElement;
    if(parent){var twins=0;for(var c=parent.firstElementChild;c;c=c.nextElementSibling){try{if(c.matches(p))twins++;}catch(_){twins=2;}}if(twins>1)p+=":nth-child("+(Array.prototype.indexOf.call(parent.children,n)+1)+")";}
    parts.unshift(p);
    if(n.id)break;
    try{if(document.querySelectorAll(parts.join(" > ")).length===1&&parts.length>1)break;}catch(_){}
    n=parent;
  }
  return parts.join(" > ");
}
function said(e){var t=(e.textContent||"").replace(/\s+/g," ").trim();return t.length>40?t.slice(0,39)+"…":t;}
function textSaid(t){var s=t.data.replace(/\s+/g," ").trim();return s.length>40?s.slice(0,39)+"…":s;}
function textRects(t){var r=document.createRange();r.selectNodeContents(t);return Array.prototype.filter.call(r.getClientRects(),function(x){return x.width>=1&&x.height>=1;});}
/* The child (or text) of e reaching furthest past e's inside edges. */
function worst(e,left,right,leftOnly){
  var best=null,most=1;
  for(var n=e.firstChild;n;n=n.nextSibling){
    var rects,what;
    if(n.nodeType===1){
      if(SKIP.test(n.tagName)||svgPart(n))continue;
      var cs=getComputedStyle(n);if(cs.position==="absolute"||cs.position==="fixed"||cs.display==="none")continue;
      var r=n.getBoundingClientRect();if(!r.width||!r.height)continue;rects=[r];what=n;
    }else if(n.nodeType===3&&n.data.trim()){rects=textRects(n);what=n;}else continue;
    rects.forEach(function(r){
      var past=Math.max(leftOnly?0:r.right-right,left-r.left);
      if(past>most){most=past;best=what;}
    });
  }
  return best&&{node:best,past:Math.round(most)};
}
function name(node){if(node.nodeType!==3)return selector(node);var t=textSaid(node);return /"/.test(t)?"its text "+t:"its text \""+t+"\"";}
function problems(){
  var out={spill:[],clipped:[],unreachable:[],overlap:[]};if(!document.body)return out;
  var all=document.body.querySelectorAll("*"),n=Math.min(all.length,LIMIT);
  for(var i=0;i<n;i++){
    var e=all[i];if(SKIP.test(e.tagName)||svgPart(e)||e.tagName==="svg")continue;
    if(e.scrollWidth<=e.clientWidth+1)continue;
    var cs=getComputedStyle(e);if(/^(inline|contents|none)$/.test(cs.display)||!visible(e))continue;
    var r=e.getBoundingClientRect(),left=r.left+parseFloat(cs.borderLeftWidth),right=Math.min(r.right-parseFloat(cs.borderRightWidth),left+e.clientWidth);
    var ox=cs.overflowX,scroller=ox==="auto"||ox==="scroll",found;
    if(scroller){
      if(e.scrollLeft<=0&&(found=worst(e,left,right,true))&&out.unreachable.length<SHOWN)out.unreachable.push({element:selector(e),text:said(e),culprit:name(found.node),past:found.past});
      continue;
    }
    var clips=ox==="hidden"||ox==="clip";if(clips&&cs.textOverflow==="ellipsis")continue;
    found=worst(e,left,right,false);if(!found)continue;
    var list=clips?out.clipped:out.spill;
    if(list.length<SHOWN)list.push({element:selector(e),text:said(e),culprit:name(found.node),past:found.past,width:Math.round(right-left)});
  }
  /* Text over text. Each text's line boxes, cut to what its scrolling or clipping boxes show. */
  var clipsOf=new Map(),NONE={l:-1e9,t:-1e9,r:1e9,b:1e9};
  function clipOf(e){
    if(!e||e===document.body||e===document.documentElement)return NONE;
    if(clipsOf.has(e))return clipsOf.get(e);
    var cs=getComputedStyle(e),c=cs.position==="fixed"?NONE:clipOf(e.parentElement);
    if(cs.overflowX!=="visible"||cs.overflowY!=="visible"){var r=e.getBoundingClientRect();c={l:Math.max(c.l,r.left),t:Math.max(c.t,r.top),r:Math.min(c.r,r.right),b:Math.min(c.b,r.bottom)};}
    clipsOf.set(e,c);return c;
  }
  function fixedOf(e){for(;e&&e!==document.body;e=e.parentElement){if(getComputedStyle(e).position==="fixed")return e;}return null;}
  var boxes=[],walker=document.createTreeWalker(document.body,NodeFilter.SHOW_TEXT),t;
  while((t=walker.nextNode())&&boxes.length<6000){
    if(!t.data.trim())continue;var p=t.parentElement;
    if(!p||p.closest("script,style,textarea,svg,noscript,template,select,.gx-tally,.gx-rowbar")||!visible(p))continue;
    var c=clipOf(p),fixed=fixedOf(p);
    textRects(t).forEach(function(r){
      var b={l:Math.max(r.left,c.l),t:Math.max(r.top,c.t),r:Math.min(r.right,c.r),b:Math.min(r.bottom,c.b)};
      if(b.r-b.l>=1&&b.b-b.t>=1)boxes.push({node:t,el:p,fixed:fixed,l:b.l,t:b.t,r:b.r,b:b.b});
    });
  }
  boxes.sort(function(a,b){return a.t-b.t;});
  var seen={};
  for(var a=0;a<boxes.length&&out.overlap.length<SHOWN;a++){
    var x=boxes[a];
    for(var k=a+1;k<boxes.length&&boxes[k].t<x.b;k++){
      var y=boxes[k];if(y.node===x.node)continue;
      var w=Math.min(x.r,y.r)-Math.max(x.l,y.l),h=Math.min(x.b,y.b)-Math.max(x.t,y.t);
      if(w<=2||h<=0)continue;
      var smallH=Math.min(x.b-x.t,y.b-y.t),smallA=Math.min((x.r-x.l)*(x.b-x.t),(y.r-y.l)*(y.b-y.t));
      if(h<0.4*smallH||w*h<0.2*smallA)continue;
      var top=y.fixed&&!x.fixed?y:x,under=top===y?x:y,key=selector(top.el)+"|"+selector(under.el);
      if(seen[key])continue;seen[key]=1;
      out.overlap.push({element:selector(top.el),text:textSaid(top.node),fixed:!!top.fixed,other:selector(under.el),otherText:textSaid(under.node)});
      if(out.overlap.length>=SHOWN)break;
    }
  }
  /* The answered pill, and the bar that takes its place while a list's rows are on screen, float over the page on purpose, wherever it is scrolled; what must hold is that at the page's end, in their own room, they cover no text or control. */
  Array.prototype.forEach.call(document.querySelectorAll(".gx-tally,.gx-rowbar"),function(pill){
  if(pill&&visible(pill)){
    var se=document.scrollingElement||document.documentElement,was=se.scrollTop;se.scrollTop=se.scrollHeight;
    var q=pill.getBoundingClientRect(),hit=null;
    var covers=function(r){return Math.min(q.right,r.right)-Math.max(q.left,r.left)>2&&Math.min(q.bottom,r.bottom)-Math.max(q.top,r.top)>2;};
    Array.prototype.some.call(document.querySelectorAll("button,a[href],input,select,textarea,label,[role=button]"),function(e){
      if(pill.contains(e)||!visible(e))return false;var r=e.getBoundingClientRect();if(r.width&&r.height&&covers(r)){hit=e;return true;}return false;
    });
    if(!hit){var w2=document.createTreeWalker(document.body,NodeFilter.SHOW_TEXT),t2;
      while(!hit&&(t2=w2.nextNode())){var p2=t2.parentElement;if(!t2.data.trim()||!p2||pill.contains(p2)||p2.closest("script,style,noscript,template")||!visible(p2))continue;if(textRects(t2).some(covers))hit=p2;}}
    se.scrollTop=was;
    if(hit&&out.overlap.length<SHOWN)out.overlap.push({element:selector(pill),text:pill.textContent.trim(),fixed:true,other:selector(hit),otherText:hit.textContent.trim().slice(0,60),end:true});
  }
  });
  return out;
}
});
}"###;

/// What to tell the agent about one width, from the probe's answer.
pub(crate) fn page_findings(width: u32, report: &Value) -> Vec<String> {
    let mut findings = Vec::new();
    let Some(fit) = report.get("fit").filter(|fit| !fit.is_null()) else {
        if report
            .get("docWidth")
            .and_then(Value::as_f64)
            .is_some_and(|wide| wide > f64::from(width) + 1.0)
        {
            findings.push(format!(
                "The page is wider than a {width}px screen. Phones zoom the whole page out to fit it, so everything turns small and narrow."
            ));
        }
        return findings;
    };
    let entries = |key: &str| -> Vec<String> {
        fit.get(key)
            .and_then(Value::as_array)
            .map(|items| items.iter().map(describe).collect())
            .unwrap_or_default()
    };
    let overflow = fit
        .get("overflow")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if !overflow.is_empty() {
        let list = overflow
            .iter()
            .map(|item| {
                let right = item.get("right").and_then(Value::as_f64).unwrap_or(0.0);
                format!("{} (ends at {right:.0}px)", describe(item))
            })
            .collect::<Vec<_>>();
        findings.push(format!(
            "Reaches past the {width}px screen: {}. Make it fit the column: let it wrap or stack, give it max-width:100%, or put a fixed-width mockup in a .gx-frame (or <gx-wide>) so it scrolls sideways in its own box.",
            join(&list)
        ));
    }
    let stacked = entries("stacked");
    if !stacked.is_empty() {
        findings.push(format!(
            "The kit stacked side-by-side columns that did not fit: {}. Write a stacking rule yourself (.gx-grid, or @media (max-width:600px) {{ grid-template-columns: 1fr }}) so the page reads the same without the kit's help.",
            join(&stacked)
        ));
    }
    let nowrap = entries("nowrap");
    if !nowrap.is_empty() {
        findings.push(format!(
            "The kit kept pills from breaking mid-word: {}. Give pills, chips and badges white-space:nowrap and flex:none.",
            join(&nowrap)
        ));
    }
    let scroll = entries("scroll");
    if !scroll.is_empty() {
        findings.push(format!(
            "Scrolls sideways inside its own box (with a fade at the edge): {}. Fine for a fixed-width mockup of a real screen; anything else should wrap or stack to the column instead.",
            join(&scroll)
        ));
    }
    findings
}

/// What the kit adjusted that needs no fix (`ghostex visual check` says it; `ghostex show` does
/// not, since the page reads well as it is).
pub(crate) fn kit_notes(report: &Value) -> Vec<String> {
    let widened = report
        .pointer("/fit/widened")
        .and_then(Value::as_array)
        .map(|items| items.iter().map(describe).collect::<Vec<_>>())
        .unwrap_or_default();
    if widened.is_empty() {
        return Vec::new();
    }
    vec![format!(
        "The kit gave columns more room so what they hold fits (a mockup, or a value's longest word), which puts fewer side by side: {}. Fine as it is; to choose the width yourself, set --gx-option-min, --gx-grid-min or --gx-stat-min on the row (for a mockup: its width plus the card's padding).",
        join(&widened)
    )]
}

/// What the page itself gets wrong at one size (`problems` from `INSPECT`): one finding per kind of
/// problem, saying how to fix it, then each place it happens by its CSS selector (on its own
/// indented line). Empty when the page fits.
pub(crate) fn problem_findings(report: &Value) -> Vec<String> {
    let Some(problems) = report.get("problems").filter(|value| value.is_object()) else {
        return Vec::new();
    };
    let list = |key: &str| {
        problems
            .get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let text = |item: &Value, key: &str| {
        item.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let px = |item: &Value, key: &str| item.get(key).and_then(Value::as_f64).unwrap_or(0.0);
    let said = |item: &Value, key: &str| match text(item, key) {
        said if said.is_empty() => String::new(),
        said if said.contains('"') => format!(" ({said})"),
        said => format!(" (\"{said}\")"),
    };
    let group = |head: &str, lines: Vec<String>| -> Option<String> {
        (!lines.is_empty()).then(|| {
            let mut finding = head.to_string();
            for line in lines {
                finding.push_str("\n    ");
                finding.push_str(&line);
            }
            finding
        })
    };
    let mut findings = Vec::new();
    findings.extend(group(
        "Overflows its box, over whatever is beside it. Let it shrink and wrap (min-width:0 on grid and flex children, overflow-wrap:anywhere on long words, no white-space:nowrap on long values), give it max-width:100%, or put a fixed-width mockup in a .gx-frame so it scrolls in its own box:",
        list("spill")
            .iter()
            .map(|item| {
                format!(
                    "`{}`{}, {:.0}px wide: {} reaches {:.0}px past its edge",
                    text(item, "element"),
                    said(item, "text"),
                    px(item, "width"),
                    text(item, "culprit"),
                    px(item, "past"),
                )
            })
            .collect(),
    ));
    findings.extend(group(
        "Cut off by overflow:hidden. Let the content wrap or fit the box, or end the cut with text-overflow:ellipsis:",
        list("clipped")
            .iter()
            .map(|item| {
                format!(
                    "`{}`{} hides {:.0}px of {}",
                    text(item, "element"),
                    said(item, "text"),
                    px(item, "past"),
                    text(item, "culprit"),
                )
            })
            .collect(),
    ));
    findings.extend(group(
        "Can't be scrolled to: a box scrolls sideways, but part of its content starts left of where scrolling begins. Use justify-content:safe center (or flex-start) in a box that scrolls:",
        list("unreachable")
            .iter()
            .map(|item| {
                format!(
                    "`{}`: {} starts {:.0}px left of it",
                    text(item, "element"),
                    text(item, "culprit"),
                    px(item, "past"),
                )
            })
            .collect(),
    ));
    let overlaps = list("overlap");
    let (pill_at_end, overlaps): (Vec<_>, Vec<_>) = overlaps
        .iter()
        .partition(|item| item.get("end").and_then(Value::as_bool) == Some(true));
    let (fixed, text_over): (Vec<_>, Vec<_>) = overlaps
        .into_iter()
        .partition(|item| item.get("fixed").and_then(Value::as_bool) == Some(true));
    let over = |items: Vec<&Value>| {
        items
            .into_iter()
            .map(|item| {
                format!(
                    "`{}`{} over `{}`{}",
                    text(item, "element"),
                    said(item, "text"),
                    text(item, "other"),
                    said(item, "otherText"),
                )
            })
            .collect::<Vec<_>>()
    };
    findings.extend(group(
        "Text drawn over other text. Let each text wrap inside its own box (min-width:0, overflow-wrap:anywhere) or give the boxes room to stack:",
        over(text_over),
    ));
    findings.extend(group(
        "A box fixed to the screen covers text. Give it room of its own, or move it off the content:",
        over(fixed),
    ));
    findings.extend(group(
        "Scrolled to the end, the answered pill covers content. The kit leaves room for it after the page's last box, so something reaches into that room (a fixed height on html or body, or a box placed at the bottom):",
        over(pill_at_end),
    ));
    findings
}

/// The questions that sit under mockups although none of the mockups got a Pick button, from the
/// kit's report (`window.__ghostexKit.untied`, visual_pages/kit.rs `autoTie`): the author ties them
/// before sending, since the reader would have to scroll down to the question to pick.
///
/// CDXC:SessionChat 2026-10-30 DECISION:
/// User: "PLEASE for inline htmls we must show the pick button on the top right so i can pick one easily instead of having to pick below". The kit ties untied variants itself where it can; when it cannot, `ghostex show` and `ghostex visual check` say so plainly (a warning, never a refusal), naming the question.
pub(crate) fn pick_findings(report: &Value) -> Vec<String> {
    report
        .pointer("/kit/untied")
        .and_then(Value::as_array)
        .map(|questions| {
            questions
                .iter()
                .map(|question| {
                    let text = |key: &str| {
                        question
                            .get(key)
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|text| !text.is_empty())
                    };
                    let title = text("title");
                    let tag = text("tag");
                    let key = text("question").unwrap_or("a question");
                    let name = match (tag, title) {
                        (Some(tag), Some(title)) => format!("{tag} \"{title}\""),
                        (None, Some(title)) => format!("\"{title}\""),
                        (Some(tag), None) => tag.to_string(),
                        (None, None) => format!("\"{key}\""),
                    };
                    let asked = tag.unwrap_or(key);
                    format!(
                        "{name} sits under mockups but none of them got a Pick button, so the reader has to scroll down to the question to choose. Put data-question=\"{asked}\" and data-choice=\"<the choice's letter or label>\" on each variant's section (the element holding its heading and mockup), or start each variant's heading with its choice's letter (\"A. Tabs\") or exactly its label."
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The questions that list again, as their choices, the items the page shows above them, from the
/// kit's report (`window.__ghostexKit.repeated`, visual_pages/kit.rs `repeatedLists`): the reader
/// would read a row, scroll down to find it in the question, pick, and scroll back. `ghostex visual
/// check` fails on them; `ghostex show` warns.
///
/// CDXC:SessionChat 2026-11-20 DECISION:
/// User: "I want ALWAYS to add the pick buttons there on the same row instead of having to scroll down to pick from there, that makes no sense at all." So the check refuses a page whose question repeats its rows, and says how to put the picks on them (`data-pick`, `<gx-pick>`; the kit's `bindRows`).
pub(crate) fn repeated_findings(report: &Value) -> Vec<String> {
    report
        .pointer("/kit/repeated")
        .and_then(Value::as_array)
        .map(|questions| {
            questions
                .iter()
                .map(|question| {
                    let text = |key: &str| {
                        question
                            .get(key)
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|text| !text.is_empty())
                    };
                    let tag = text("tag");
                    let asked = tag.or(text("question")).unwrap_or("Q1");
                    let name = match (tag, text("title")) {
                        (Some(tag), Some(title)) => format!("{tag} \"{title}\""),
                        (_, Some(title)) => format!("\"{title}\""),
                        (Some(tag), None) => tag.to_string(),
                        (None, None) => "A question".to_string(),
                    };
                    let items: Vec<&str> = question
                        .get("items")
                        .and_then(Value::as_array)
                        .map(|items| items.iter().filter_map(Value::as_str).collect())
                        .unwrap_or_default();
                    let first = items.first().copied().unwrap_or("FB-1");
                    format!(
                        "{name} repeats {} items listed above it ({}), so the reader scrolls between a row and its pick. Put the picks on the rows: data-question=\"{asked}\" on the list and data-pick=\"{first}\" on each row (or write each item as <gx-pick value=\"{first}\"> with Markdown inside), and drop the question's <gx-choice> lines: the kit makes them from the rows.",
                        items.len(),
                        items.join(", ")
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn describe(item: &Value) -> String {
    let element = item
        .get("element")
        .and_then(Value::as_str)
        .unwrap_or("an element");
    match item
        .get("text")
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
    {
        Some(text) => format!("{element} \"{text}\""),
        None => element.to_string(),
    }
}

/// At most six items, then how many more.
fn join(items: &[String]) -> String {
    const SHOWN: usize = 6;
    let mut text = items
        .iter()
        .take(SHOWN)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if items.len() > SHOWN {
        text.push_str(&format!(" and {} more", items.len() - SHOWN));
    }
    text
}

/// Where a size's screenshot goes: `--out` as given for one size, with `-<width>` (and
/// `-text<percent>` for large text) before the extension for several, else beside the system's
/// temporary files.
pub(crate) fn screenshot_path(
    page: &Path,
    out: Option<&Path>,
    width: u32,
    text_scale: f64,
    several: bool,
) -> PathBuf {
    let size = if is_large_text(text_scale) {
        format!("{width}-text{:.0}", text_scale * 100.0)
    } else {
        width.to_string()
    };
    match out {
        Some(out) if !several => out.to_path_buf(),
        Some(out) => {
            let stem = out
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_else(|| "page".to_string());
            out.with_file_name(format!("{stem}-{size}.png"))
        }
        None => {
            let stem = page
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_else(|| "page".to_string());
            std::env::temp_dir()
                .join("ghostex-visuals")
                .join(format!("{stem}-{size}.png"))
        }
    }
}

/// The file for part `n` (from 1) of a long screenshot: the path itself for the first part,
/// `-part<n>` before the extension for the rest.
fn part_path(out: &Path, n: usize) -> PathBuf {
    if n == 1 {
        return out.to_path_buf();
    }
    let stem = out
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "page".to_string());
    out.with_file_name(format!("{stem}-part{n}.png"))
}

fn absolute(file: &str) -> PathBuf {
    let path = PathBuf::from(file);
    if path.is_absolute() {
        return path;
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(&path))
        .unwrap_or(path)
}

fn file_url(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let encoded = text
        .split('/')
        .map(|part| {
            part.bytes()
                .map(|byte| match byte {
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b':' => {
                        (byte as char).to_string()
                    }
                    _ => format!("%{byte:02X}"),
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("/");
    if encoded.starts_with('/') {
        format!("file://{encoded}")
    } else {
        format!("file:///{encoded}")
    }
}

/// A Chromium browser to check with: `GHOSTEX_CHROME` when set, else the usual install places.
fn chrome_binary() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("GHOSTEX_CHROME")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
    {
        return Some(path);
    }
    chrome_candidates().into_iter().find(|path| path.is_file())
}

fn chrome_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if cfg!(target_os = "macos") {
        let apps = [
            "Google Chrome.app/Contents/MacOS/Google Chrome",
            "Chromium.app/Contents/MacOS/Chromium",
            "Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
            "Brave Browser.app/Contents/MacOS/Brave Browser",
        ];
        let mut roots = vec![PathBuf::from("/Applications")];
        if let Some(home) = std::env::var_os("HOME") {
            roots.push(PathBuf::from(home).join("Applications"));
        }
        for root in &roots {
            for app in apps {
                candidates.push(root.join(app));
            }
        }
    } else if cfg!(windows) {
        let programs = [
            "Google/Chrome/Application/chrome.exe",
            "Microsoft/Edge/Application/msedge.exe",
            "BraveSoftware/Brave-Browser/Application/brave.exe",
            "Chromium/Application/chrome.exe",
        ];
        for root in ["PROGRAMFILES", "PROGRAMFILES(X86)", "LOCALAPPDATA"] {
            if let Some(root) = std::env::var_os(root) {
                for program in programs {
                    candidates.push(PathBuf::from(&root).join(program));
                }
            }
        }
    } else {
        let names = [
            "google-chrome",
            "google-chrome-stable",
            "chromium",
            "chromium-browser",
            "microsoft-edge",
            "brave-browser",
        ];
        if let Some(path) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&path) {
                for name in names {
                    candidates.push(dir.join(name));
                }
            }
        }
    }
    candidates
}

/// A headless Chrome with its own throwaway profile, stopped when dropped.
struct Browser {
    child: Child,
    port: u16,
}

impl Browser {
    fn start(chrome: &Path, profile: &Path) -> CliResult<Browser> {
        let child = Command::new(chrome)
            .args([
                "--headless=new",
                "--remote-debugging-port=0",
                "--no-first-run",
                "--no-default-browser-check",
                "--disable-extensions",
                "--hide-scrollbars",
                "--mute-audio",
                "--allow-file-access-from-files",
            ])
            .arg(format!("--user-data-dir={}", profile.display()))
            .arg("about:blank")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| {
                CliError::Other(format!("Could not start {}: {error}", chrome.display()))
            })?;
        let mut browser = Browser { child, port: 0 };
        let deadline = Instant::now() + CHROME_START_TIMEOUT;
        let active = profile.join("DevToolsActivePort");
        loop {
            if let Some(port) = std::fs::read_to_string(&active)
                .ok()
                .and_then(|text| text.lines().next()?.trim().parse::<u16>().ok())
            {
                browser.port = port;
                return Ok(browser);
            }
            if Instant::now() >= deadline || browser.child.try_wait().ok().flatten().is_some() {
                return Err(CliError::Other(format!(
                    "{} did not start a headless browser to check the page with.",
                    chrome.display()
                )));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// The DevTools socket of the browser's page tab (a new one when it shows none).
    fn connect(&self) -> CliResult<Cdp> {
        let deadline = Instant::now() + CHROME_START_TIMEOUT;
        loop {
            let pages = http(self.port, "GET", "/json/list").unwrap_or(Value::Null);
            let socket = pages.as_array().and_then(|pages| {
                pages
                    .iter()
                    .find(|page| page.get("type").and_then(Value::as_str) == Some("page"))
                    .and_then(|page| page.get("webSocketDebuggerUrl").and_then(Value::as_str))
                    .map(str::to_string)
            });
            let socket = socket.or_else(|| {
                http(self.port, "PUT", "/json/new?about:blank")
                    .ok()
                    .and_then(|page| {
                        page.get("webSocketDebuggerUrl")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
            });
            if let Some(socket) = socket {
                return Cdp::connect(self.port, &socket);
            }
            if Instant::now() >= deadline {
                return Err(CliError::Other(
                    "Chrome opened no page to check with.".to_string(),
                ));
            }
            std::thread::sleep(Duration::from_millis(150));
        }
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// One small HTTP request to the browser's DevTools endpoint, answered with JSON (HTTP/1.1: Chrome
/// answers an HTTP/1.0 request with nothing).
fn http(port: u16, method: &str, path: &str) -> Result<Value, String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).map_err(|error| error.to_string())?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: 0\r\n\r\n"
    )
    .map_err(|error| error.to_string())?;
    // Chrome keeps the connection open after answering, so the body is read to its length.
    let mut answer = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    loop {
        let read = stream.read(&mut chunk).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        answer.extend_from_slice(&chunk[..read]);
        let text = String::from_utf8_lossy(&answer);
        if let Some((head, body)) = text.split_once("\r\n\r\n") {
            let length = head.lines().find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.trim()
                    .eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())?
            });
            if length.is_some_and(|length| body.len() >= length) {
                break;
            }
        }
    }
    let text = String::from_utf8_lossy(&answer);
    let body = text.split_once("\r\n\r\n").map_or("", |(_, body)| body);
    serde_json::from_str(body).map_err(|error| error.to_string())
}

/// The page tab's DevTools socket: calls in order, events dropped.
struct Cdp {
    socket: tungstenite::WebSocket<TcpStream>,
    next_id: u64,
}

impl Cdp {
    fn connect(port: u16, url: &str) -> CliResult<Cdp> {
        let stream = TcpStream::connect(("127.0.0.1", port))
            .map_err(|error| CliError::Other(format!("Could not reach Chrome: {error}")))?;
        let (socket, _) = tungstenite::client::client(url, stream)
            .map_err(|error| CliError::Other(format!("Could not reach Chrome: {error}")))?;
        Ok(Cdp { socket, next_id: 1 })
    }

    fn call(&mut self, method: &str, params: Value) -> CliResult<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let payload = json!({ "id": id, "method": method, "params": params }).to_string();
        self.socket
            .send(tungstenite::Message::Text(payload.into()))
            .map_err(|error| CliError::Other(format!("Chrome stopped answering: {error}")))?;
        let deadline = Instant::now() + CDP_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(CliError::Other(format!(
                    "Chrome did not answer {method} in time."
                )));
            }
            let _ = self.socket.get_mut().set_read_timeout(Some(remaining));
            let message = self
                .socket
                .read()
                .map_err(|error| CliError::Other(format!("Chrome stopped answering: {error}")))?;
            let tungstenite::Message::Text(text) = message else {
                continue;
            };
            let Ok(parsed) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            if parsed.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = parsed.get("error") {
                let message = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown error");
                return Err(CliError::Other(format!(
                    "Chrome refused {method}: {message}"
                )));
            }
            return Ok(parsed.get("result").cloned().unwrap_or(Value::Null));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_files_are_pages() {
        assert!(is_page_file("mockup.html"));
        assert!(is_page_file("/a/B.HTM"));
        assert!(!is_page_file("chart.json"));
        assert!(!is_page_file("-"));
    }

    #[test]
    fn a_page_that_fits_has_no_findings() {
        let report = json!({"page":true,"docWidth":390,"fit":{"width":390,"stacked":[],"nowrap":[],"scroll":[],"overflow":[]}});
        assert!(page_findings(390, &report).is_empty());
    }

    #[test]
    fn what_the_kit_fitted_is_reported_with_how_to_fix_it() {
        let report = json!({"page":true,"fit":{
            "stacked":[{"element":"div.grid","text":"Today Proposed"}],
            "nowrap":[{"element":"span.pill","text":"Workers 2"}],
            "scroll":[{"element":"div.card","text":"Editing links.rs"}],
            "overflow":[]}});
        let findings = page_findings(390, &report);
        assert_eq!(findings.len(), 3);
        assert!(findings[0].contains("div.grid \"Today Proposed\""));
        assert!(findings[0].contains("stacking rule"));
        assert!(findings[1].contains("span.pill \"Workers 2\""));
        assert!(findings[1].contains("white-space:nowrap"));
        assert!(findings[2].contains("div.card"));
    }

    #[test]
    fn content_past_the_screen_comes_first_with_where_it_ends() {
        let report = json!({"page":true,"fit":{"stacked":[{"element":"div.grid"}],"nowrap":[],"scroll":[],
            "overflow":[{"element":"div.phone","text":"","right":612}]}});
        let findings = page_findings(390, &report);
        assert!(findings[0].starts_with("Reaches past the 390px screen: div.phone (ends at 612px)"));
        assert!(findings[1].contains("div.grid"));
    }

    #[test]
    fn a_page_without_the_kit_is_judged_by_its_width() {
        let wide = json!({"page":true,"fit":null,"docWidth":694});
        assert_eq!(page_findings(390, &wide).len(), 1);
        let fits = json!({"page":true,"fit":null,"docWidth":390});
        assert!(page_findings(390, &fits).is_empty());
    }

    #[test]
    fn long_lists_are_cut_short() {
        let items = (0..9).map(|n| format!("e{n}")).collect::<Vec<_>>();
        assert_eq!(join(&items), "e0, e1, e2, e3, e4, e5 and 3 more");
    }

    #[test]
    fn screenshots_get_the_width_when_several_are_taken() {
        let page = Path::new("/w/mock.html");
        assert_eq!(
            screenshot_path(page, Some(Path::new("/o/shot.png")), 390, 1.0, false),
            PathBuf::from("/o/shot.png")
        );
        assert_eq!(
            screenshot_path(page, Some(Path::new("/o/shot.png")), 390, 1.0, true),
            PathBuf::from("/o/shot-390.png")
        );
        assert!(
            screenshot_path(page, None, 1280, 1.0, true).ends_with("ghostex-visuals/mock-1280.png")
        );
        assert!(screenshot_path(page, None, 900, 1.3, true)
            .ends_with("ghostex-visuals/mock-900-text130.png"));
    }

    /// The kit's phone fit (visual_pages/fit.rs) in a real browser: a fixed two-column grid, pills
    /// that wrap and a fixed-width mockup. Runs only where a Chrome is installed.
    #[test]
    fn the_kit_fits_a_wide_page_to_a_phone_and_leaves_the_computer_alone() {
        if chrome_binary().is_none() {
            eprintln!("skipped: no Chrome to check pages with");
            return;
        }
        let dir = tempfile::tempdir().expect("a scratch folder");
        let page = dir.path().join("wide.html");
        std::fs::write(
            &page,
            r#"<!doctype html><html><head><meta name="ghostex-page" content="v2"><title>Wide</title>
<style>body{padding:20px}.two{display:grid;grid-template-columns:1fr 1fr;gap:14px}
.row{display:flex;gap:8px;white-space:nowrap}.pill{display:inline-flex;padding:0 10px;border:1px solid;border-radius:13px}
.phone{width:600px;height:40px}</style></head><body><gx-page>
<header><h1>Wide</h1></header>
<div class="two"><div class="row">A row that is long enough to overflow a phone column</div><div class="row">Another long row of words here</div></div>
<div class="bar" style="display:flex;gap:6px"><span class="pill">Workers 2</span><span class="pill">SPX-123 +2</span><span class="pill">Resources 3</span><span class="pill">Tasks 6/11</span><span class="pill">Workers 1</span><span class="pill">3 suggested</span></div>
<div class="phone">fixed-width mockup</div>
</gx-page></body></html>"#,
        )
        .expect("the page");
        let checks = [PHONE_WIDTH, DESKTOP_WIDTH].map(|width| PageCheck {
            width,
            text_scale: 1.0,
            light: false,
            screenshot: None,
        });
        let results = check_page(&page, &checks).expect("the check");
        let fit = |index: usize, key: &str| {
            results[index].report["fit"][key]
                .as_array()
                .cloned()
                .unwrap_or_default()
        };
        let phone = &results[0].report;
        assert!(phone["docWidth"].as_f64().unwrap() <= f64::from(PHONE_WIDTH) + 1.0);
        assert!(fit(0, "overflow").is_empty(), "{phone}");
        assert_eq!(fit(0, "stacked").len(), 1, "{phone}");
        assert!(fit(0, "nowrap").len() >= 2, "{phone}");
        assert!(
            fit(0, "scroll")
                .iter()
                .any(|item| item["element"] == "div.phone"),
            "{phone}"
        );
        let computer = &results[1].report;
        assert!(
            fit(1, "stacked").is_empty() && fit(1, "nowrap").is_empty(),
            "{computer}"
        );
        assert!(fit(1, "overflow").is_empty(), "{computer}");
    }

    /// Opens a page the way `ghostex show` would publish it in a headless Chrome at the computer's
    /// width, then evaluates the expression there. `None` where no Chrome is installed.
    fn eval_in_page(body: &str, expression: &str) -> Option<Value> {
        let chrome = chrome_binary().or_else(|| {
            eprintln!("skipped: no Chrome to check pages with");
            None
        })?;
        let dir = tempfile::tempdir().expect("a scratch folder");
        let html = format!(
            "<!doctype html><html><head><meta name=\"ghostex-page\" content=\"v2\"><title>Pick</title></head><body>{body}</body></html>"
        );
        let prepared = prepare_page(&html, dir.path()).expect("a prepared page");
        let page = dir.path().join("page.html");
        std::fs::write(&page, prepared.html).expect("the page");
        let browser = Browser::start(&chrome, &dir.path().join("profile")).expect("a browser");
        let mut cdp = browser.connect().expect("a connection");
        cdp.call(
            "Emulation.setDeviceMetricsOverride",
            json!({ "width": DESKTOP_WIDTH, "height": 900, "deviceScaleFactor": 1, "mobile": false }),
        )
        .expect("a window size");
        cdp.call("Page.navigate", json!({ "url": file_url(&page) }))
            .expect("the page opens");
        let deadline = Instant::now() + CDP_TIMEOUT;
        loop {
            let answer = cdp
                .call(
                    "Runtime.evaluate",
                    json!({ "expression": PROBE, "awaitPromise": true, "returnByValue": true }),
                )
                .expect("the probe");
            if answer
                .pointer("/result/value/page")
                .and_then(Value::as_bool)
                == Some(true)
            {
                break;
            }
            assert!(Instant::now() < deadline, "the page did not load");
            std::thread::sleep(Duration::from_millis(150));
        }
        let answer = cdp
            .call(
                "Runtime.evaluate",
                json!({ "expression": expression, "awaitPromise": true, "returnByValue": true }),
            )
            .expect("the expression");
        let text = answer
            .pointer("/result/value")
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("no answer: {answer}"));
        Some(serde_json::from_str(text).expect("JSON"))
    }

    /// What the kit tied and what is on the page, as JSON text.
    const PICKS: &str = r#"JSON.stringify({kit:window.__ghostexKit,picks:Array.prototype.map.call(document.querySelectorAll(".gx-pick"),function(p){var h=p.parentElement;return{text:h.textContent.replace(/Pick$/,"").trim(),tag:h.tagName,last:h.lastElementChild===p};})})"#;

    fn question(tag: &str, title: &str, choices: &[&str]) -> String {
        let choices = choices
            .iter()
            .map(|choice| format!("<gx-choice>{choice}</gx-choice>"))
            .collect::<String>();
        format!("<gx-question tag=\"{tag}\" title=\"{title}\">{choices}</gx-question>")
    }

    fn variants(headings: &[&str]) -> String {
        let sections = headings
            .iter()
            .map(|heading| {
                format!("<section><h3>{heading}</h3><div class=\"gx-frame\"><div style=\"width:300px;height:80px\">mockup</div></div></section>")
            })
            .collect::<String>();
        format!("<gx-wide>{sections}</gx-wide>")
    }

    fn pairs(list: &Value) -> Vec<(String, String)> {
        list.as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|item| {
                (
                    item["question"].as_str().unwrap_or_default().to_string(),
                    format!(
                        "{}={}",
                        item["heading"].as_str().unwrap_or_default(),
                        item["choice"].as_str().unwrap_or_default()
                    ),
                )
            })
            .collect()
    }

    /// The matching of untied variants to choices (visual_pages/kit.rs `autoTie`): by letter, by
    /// label, never for an ambiguous or unmatched heading, never for a heading of an earlier
    /// question, and not at all for a question that has tied variants.
    #[test]
    fn untied_variants_get_pick_buttons_only_where_the_match_is_certain() {
        let body = format!(
            "<gx-questions><header><h2>Header heading</h2></header>{q1v}{q1}{q2v}{q2}{q3v}{q3}{q4v}{q4}{q5v}{q5}{q6v}{q6}<h3>C. Stray after the last question</h3>{q7}</gx-questions>",
            q1v = variants(&[
                "A. Whatever the first looks like",
                "B. Something else again"
            ]),
            q1 = question("Q1", "Which layout?", &["Tabs", "One long page"]),
            q2v = variants(&["Compact", "Roomy"]),
            q2 = question("Q2", "How dense?", &["Compact", "Roomy"]),
            q3v = variants(&["A. Red", "A. Green"]),
            q3 = question("Q3", "Which colour?", &["Red", "Green"]),
            q4v = variants(&["Alpha", "Alpha"]),
            q4 = question("Q4", "Which name?", &["Alpha", "Beta"]),
            q5v = variants(&["Unrelated heading", "Another heading"]),
            q5 = question("Q5", "Which other?", &["One", "Two"]),
            q6v = "<section data-question=\"Q6\" data-choice=\"x\"><h3>A. Already tied</h3><div class=\"gx-frame\">m</div></section><section><h3>B. Not tied</h3><div class=\"gx-frame\">m</div></section>",
            q6 = question("Q6", "Which tied?", &["x", "y"]),
            q7 = question("Q7", "Anything with no mockups?", &["Yes", "No"]),
        );
        let Some(page) = eval_in_page(&body, PICKS) else {
            return;
        };
        let kit = &page["kit"];
        assert_eq!(
            pairs(&kit["auto"]),
            vec![
                (
                    "Which layout?".into(),
                    "A. Whatever the first looks like=a".into()
                ),
                ("Which layout?".into(), "B. Something else again=b".into()),
                ("How dense?".into(), "Compact=a".into()),
                ("How dense?".into(), "Roomy=b".into()),
                ("Which colour?".into(), "A. Red=a".into()),
            ],
            "{page}"
        );
        let untied = kit["untied"]
            .as_array()
            .expect("untied")
            .iter()
            .map(|item| item["tag"].as_str().unwrap_or_default().to_string())
            .collect::<Vec<_>>();
        assert_eq!(untied, vec!["Q4", "Q5"], "{page}");
        // Q6 is tied by its author: the kit leaves its other heading alone.
        assert_eq!(kit["tied"], 1, "{page}");
        let picks = page["picks"].as_array().expect("picks");
        let texts = picks
            .iter()
            .map(|pick| pick["text"].as_str().unwrap_or_default())
            .collect::<Vec<_>>();
        assert_eq!(
            texts,
            vec![
                "A. Whatever the first looks like",
                "B. Something else again",
                "Compact",
                "Roomy",
                "A. Red",
                "A. Already tied",
            ],
            "{page}"
        );
        // Each button is the last thing in its heading's row.
        assert!(picks
            .iter()
            .all(|pick| pick["last"] == true && pick["tag"] == "H3"));
        // The messages name the questions.
        let report = json!({ "kit": kit });
        let findings = pick_findings(&report);
        assert_eq!(findings.len(), 2);
        assert!(findings[0].contains("Q4 \"Which name?\""), "{findings:?}");
        assert!(findings[1].contains("data-question=\"Q5\""), "{findings:?}");
    }

    /// A tied heading's Pick button picks its choice and shows it picked in both places; clicking
    /// either one clears both.
    #[test]
    fn an_auto_tied_pick_and_its_choice_pick_and_clear_together() {
        let body = format!(
            "<gx-questions>{}{}</gx-questions>",
            variants(&["A. Tabs", "B. One long page"]),
            question("Q1", "Which layout?", &["Tabs", "One long page"])
        );
        let expression = r#"(function(){
          var picks=document.querySelectorAll(".gx-pick"),radios=Array.prototype.slice.call(document.querySelectorAll(".gx-choices .gx-choice")).concat(Array.prototype.slice.call(document.querySelectorAll(".gx-pickbar .gx-other"))),log=[];
          function state(){return Array.prototype.map.call(picks,function(p){return p.getAttribute("aria-pressed")+":"+p.textContent;}).concat(Array.prototype.map.call(radios,function(r){return r.getAttribute("aria-checked");}));}
          log.push(state());picks[1].click();log.push(state());picks[1].click();log.push(state());
          radios[0].click();log.push(state());picks[1].click();log.push(state());radios[1].click();log.push(state());
          log.push([String(!!document.querySelector("gx-question.gx-q-tied"))]);
          return JSON.stringify(log);})()"#;
        let Some(log) = eval_in_page(&body, expression) else {
            return;
        };
        let log: Vec<Vec<String>> = serde_json::from_value(log).expect("states");
        // Order: pick A, pick B, choice a, choice b, Other.
        assert_eq!(
            log[0],
            ["false:Pick", "false:Pick", "false", "false", "false"]
        );
        assert_eq!(
            log[1],
            [
                "false:Pick",
                "true:\u{2713} Picked",
                "false",
                "true",
                "false"
            ]
        );
        assert_eq!(
            log[2],
            ["false:Pick", "false:Pick", "false", "false", "false"]
        );
        assert_eq!(
            log[3],
            [
                "true:\u{2713} Picked",
                "false:Pick",
                "true",
                "false",
                "false"
            ]
        );
        assert_eq!(
            log[4],
            [
                "false:Pick",
                "true:\u{2713} Picked",
                "false",
                "true",
                "false"
            ]
        );
        assert_eq!(
            log[5],
            ["false:Pick", "false:Pick", "false", "false", "false"]
        );
        // Every choice has its Pick, so the card shows what was picked instead of its list.
        assert_eq!(log[6], ["true"]);
    }

    /// Pages whose variants are tied by their author behave as before: the kit ties nothing more
    /// and says nothing.
    #[test]
    fn variants_tied_by_the_author_are_left_exactly_as_they_were() {
        let body = "<gx-questions><gx-wide><section data-question=\"Q1\" data-choice=\"a\"><h3>A. Tabs</h3><div class=\"gx-frame\">m</div></section><section data-question=\"Q1\" data-choice=\"One long page\"><h3>Second</h3><div class=\"gx-frame\">m</div></section></gx-wide>".to_string()
            + &question("Q1", "Which layout?", &["Tabs", "One long page"])
            + "</gx-questions>";
        let Some(page) = eval_in_page(&body, PICKS) else {
            return;
        };
        assert_eq!(page["kit"]["tied"], 2, "{page}");
        assert!(page["kit"]["auto"].as_array().unwrap().is_empty(), "{page}");
        assert!(
            page["kit"]["untied"].as_array().unwrap().is_empty(),
            "{page}"
        );
        assert_eq!(page["picks"].as_array().unwrap().len(), 2, "{page}");
    }

    #[test]
    fn a_question_without_a_pick_is_named_and_a_tied_page_is_quiet() {
        let report = json!({"page":true,"kit":{"tied":0,"auto":[],"untied":[
            {"question":"Which layout?","tag":"Q1","title":"Which layout?"},
            {"question":"Plain","tag":"","title":""}]}});
        let findings = pick_findings(&report);
        assert_eq!(findings.len(), 2);
        assert!(findings[0].starts_with("Q1 \"Which layout?\" sits under mockups"));
        assert!(findings[0].contains("data-question=\"Q1\""));
        assert!(findings[1].starts_with("\"Plain\" sits under mockups"));
        assert!(
            pick_findings(&json!({"page":true,"kit":{"tied":1,"auto":[],"untied":[]}})).is_empty()
        );
        assert!(pick_findings(&json!({"page":true,"kit":null})).is_empty());
    }

    #[test]
    fn file_urls_escape_spaces() {
        assert_eq!(
            file_url(Path::new("/tmp/a b/page.html")),
            "file:///tmp/a%20b/page.html"
        );
    }
    /// What the page sends when its Send button is pressed, as JSON text (`ghostex.submit` caught).
    const SENT: &str = r#"(function(){var sent=null;ghostex.submit=function(out){sent=out;return Promise.resolve({});};
      document.querySelector(".gx-send-button").click();return JSON.stringify({sent:sent,keys:sent?Object.keys(sent):[],notes:document.querySelectorAll(".gx-notes").length});})()"#;

    /// Every question page ends with the kit's "Anything else?" box just above the send bar; its
    /// text arrives last under "Additional notes", and an empty box sends nothing extra
    /// (CDXC:SessionChat 2026-11-04 DECISION on KIT_SCRIPT).
    #[test]
    fn a_question_page_ends_with_an_anything_else_box_sent_as_additional_notes() {
        let body = format!(
            "<gx-questions>{}</gx-questions>",
            question("Q1", "Which layout?", &["Tabs", "One long page"])
        );
        let expression = format!(
            r#"(function(){{var card=document.querySelector(".gx-notes"),next=card&&card.nextElementSibling;
              var empty=JSON.parse({SENT});
              return {{order:next&&next.className,title:card&&card.querySelector(".gx-q-title").textContent,empty:empty}};}})()"#
        );
        let Some(empty) = eval_in_page(&body, &format!("JSON.stringify({expression})")) else {
            return;
        };
        assert_eq!(empty["order"], "gx-send");
        assert_eq!(empty["title"], "Anything else?");
        assert_eq!(empty["empty"]["notes"], 1);
        assert_eq!(
            empty["empty"]["sent"],
            json!({ "Which layout?": "skipped" })
        );

        let expression = format!(
            r#"(function(){{document.querySelectorAll(".gx-choice")[0].click();var t=document.querySelector(".gx-notes textarea");t.value="  Ship macOS first  ";t.dispatchEvent(new Event("input",{{bubbles:true}}));return {SENT};}})()"#
        );
        let Some(written) = eval_in_page(&body, &expression) else {
            return;
        };
        assert_eq!(
            written["sent"],
            json!({ "Which layout?": "Tabs", "Additional notes": "Ship macOS first" })
        );
        assert_eq!(
            written["keys"],
            json!(["Which layout?", "Additional notes"])
        );
    }

    /// A question that already answers under "Additional notes" moves the box's key on, the way the
    /// chat's forms number it; a page whose last question is its own "Anything else?" text question
    /// gets no second box.
    #[test]
    fn the_anything_else_box_keeps_clear_of_the_pages_own_questions() {
        let taken = format!(
            "<gx-questions>{}</gx-questions>",
            question("Q1", "Additional notes", &["Yes", "No"])
        );
        let expression = format!(
            r#"(function(){{var t=document.querySelector(".gx-notes textarea");t.value="more";t.dispatchEvent(new Event("input",{{bubbles:true}}));return {SENT};}})()"#
        );
        let Some(numbered) = eval_in_page(&taken, &expression) else {
            return;
        };
        assert_eq!(
            numbered["sent"],
            json!({ "Additional notes": "skipped", "Additional notes (2)": "more" })
        );

        let own = format!(
            "<gx-questions>{}<gx-question title=\"Anything else?\" type=\"text\"></gx-question></gx-questions>",
            question("Q1", "Which layout?", &["Tabs", "One long page"])
        );
        let Some(sent) = eval_in_page(&own, SENT) else {
            return;
        };
        assert_eq!(sent["notes"], 0);
        assert_eq!(
            sent["sent"],
            json!({ "Which layout?": "skipped", "Anything else?": "skipped" })
        );
    }

    /// An image pasted into the box goes through the page's `/image` route like a note's, is written
    /// in as `[Image #1](path)` so the answers carry it, and shows as a thumbnail that removes it.
    #[test]
    fn an_image_pasted_into_the_anything_else_box_shows_as_a_thumbnail_and_is_sent() {
        let body = format!(
            "<gx-questions>{}</gx-questions>",
            question("Q1", "Which layout?", &["Tabs", "One long page"])
        );
        let expression = format!(
            r#"new Promise(function(done){{
              window.fetch=function(url){{return Promise.resolve(new Response(JSON.stringify({{ok:true,path:"/Users/me/pasted shot.png"}}),{{status:200}}));}};
              var c=document.createElement("canvas");c.width=40;c.height=30;var g=c.getContext("2d");g.fillStyle="red";g.fillRect(0,0,40,30);
              c.toBlob(function(blob){{var t=document.querySelector(".gx-notes textarea"),dt=new DataTransfer();t.value="See this";t.focus();t.setSelectionRange(8,8);
                dt.items.add(new File([blob],"shot.png",{{type:"image/png"}}));
                t.dispatchEvent(new ClipboardEvent("paste",{{clipboardData:dt,bubbles:true,cancelable:true}}));
                (function wait(n){{var thumb=document.querySelector(".gx-notes .gx-thumb img");
                  if(!thumb&&n<50)return setTimeout(function(){{wait(n+1);}},50);
                  var text=t.value,sent=JSON.parse({SENT});
                  done(JSON.stringify({{text:text,thumb:!!thumb&&/^blob:/.test(thumb.src),pills:document.querySelectorAll(".gx-notes .gx-pill").length,sent:sent.sent}}));}})(0);
              }},"image/png");}})"#
        );
        let Some(pasted) = eval_in_page(&body, &expression) else {
            return;
        };
        assert_eq!(
            pasted["text"],
            "See this [Image #1](/Users/me/pasted shot.png) "
        );
        assert_eq!(pasted["thumb"], true);
        assert_eq!(pasted["pills"], 0);
        assert_eq!(
            pasted["sent"]["Additional notes"],
            "See this [Image #1](/Users/me/pasted shot.png)"
        );
    }
}
