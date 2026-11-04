//! The kit's phone fit: every published page holds up on a phone (360–430px) whatever its own CSS
//! does, the way the theme makes every page follow light and dark.
//!
//! The style narrows the template's gutters on a small screen, keeps the phone's browser from
//! inflating text, lets pictures shrink to the column (a tap opens them full size in the phone's
//! viewer), and keeps the question card's footer on one row. The script then fits what the page's
//! own CSS left too wide, in this order, and again whenever the window's width changes:
//!
//! 1. A small pill or chip whose text wrapped (`SPX-` over `123`) stays on one line.
//! 2. On a phone, a multi-column grid whose content no longer fits stacks into one column.
//! 3. A block still wider than its column scrolls sideways inside its own box, with a fade on the
//!    edge that has more, so the page itself never scrolls sideways. A block whose own box is wider
//!    than the column (a fixed-width device frame) is wrapped in a scroller, so it keeps its real
//!    size instead of being squashed.
//!
//! What it did is kept in `window.__ghostexFit` (`stacked`, `nowrap`, `scroll`, `overflow`), which
//! `ghostex visual check` reads to tell the agent what to fix in its page.
//!
//! CDXC:SessionChat 2026-10-28 DECISION:
//! User, on an agent's page opened on their Android phone (a column two thirds of the screen wide, mockups running off the right edge, pills breaking mid-word, a narrow question card): "The inline html u sent doesn't look good at phone at all please fix this for customers and all future inline htmls like we did previously for light dark etc". So the kit fits every page to a phone instead of trusting each agent's CSS.
//! CDXC:SessionChat 2026-10-28 WHY:
//! The phone's page sheet is an Android WebView in overview mode (react-native-webview's `scalesPageToFit`, on by default): when anything makes the document wider than the screen it zooms the whole page out to fit, and Chrome's text autosizer then inflates the text, which is what made the column narrow and the pills wrap. So the page must never be wider than the screen: a too-wide block scrolls inside its own box (the user's standing rule for mockups: "horizontal scrolling is fine, a squashed or wrapped screen is broken"), rather than being scaled down, which would turn 13px mockup text into unreadable 7px text; and on a phone the root clips sideways overflow as the last guard.
//! SEE-ALSO: kit.rs (the template's column and the question card), apps/mobile/app/src/chat/native/cards/PageSheet.tsx (the phone's page sheet), server/src/ghostex_cli/visual_page_check.rs (reads `window.__ghostexFit`), skills/ghostex-visuals/SKILL.md (teaches agents to write pages that need none of this).

/// The fit's look; after the kit's style, so its narrow-screen rules win over the kit's defaults.
pub(super) const FIT_STYLE: &str = r#"<style id="ghostex-fit">
html{-webkit-text-size-adjust:100%;text-size-adjust:100%}
:where(gx-page,gx-questions) :where(img,video,canvas){max-width:100%;height:auto}
.gx-send{flex-wrap:nowrap}
.gx-send-status{flex:1 1 0}
.gx-skip-all{flex:none}
.gx-send-button{flex:0 1 auto;min-width:0}
.gx-send-button>span{min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.gx-fit-nowrap{white-space:nowrap!important;flex-shrink:0!important}
.gx-fit-stack{grid-template-columns:minmax(0,1fr)!important}
.gx-fit-grow{min-width:max-content!important}
.gx-fit-scroll{overflow-x:auto!important;overscroll-behavior-x:contain;max-width:100%;min-width:0;box-sizing:border-box}
.gx-frame{overscroll-behavior-x:contain}
.gx-more-right{-webkit-mask-image:linear-gradient(90deg,#000 calc(100% - 36px),transparent);mask-image:linear-gradient(90deg,#000 calc(100% - 36px),transparent)}
.gx-more-left{-webkit-mask-image:linear-gradient(90deg,transparent,#000 36px);mask-image:linear-gradient(90deg,transparent,#000 36px)}
.gx-more-left.gx-more-right{-webkit-mask-image:linear-gradient(90deg,transparent,#000 36px,#000 calc(100% - 36px),transparent);mask-image:linear-gradient(90deg,transparent,#000 36px,#000 calc(100% - 36px),transparent)}
@media (max-width:600px){
:where(:root){--gx-gutter:16px}
html{overflow-x:clip}
body:has(>gx-page,>gx-questions){padding-left:0!important;padding-right:0!important;margin-left:0!important;margin-right:0!important}
gx-page,gx-questions{padding:32px 0 56px;row-gap:18px}
:where(gx-page,gx-questions) :where(h1){font-size:26px}
:where(gx-page,gx-questions) :where(h2){margin-top:18px;font-size:20px}
.gx-lede{font-size:16px}
gx-question{padding:12px 12px 12px}
.gx-choice{gap:10px;padding:10px}
.gx-send{gap:8px;padding:10px 12px}
.gx-skip-all,.gx-send-button{padding:0 10px}
[data-question][data-choice]:not(h1,h2,h3,h4,h5,h6){padding:10px}
:where([data-question][data-choice]:not(h1,h2,h3,h4,h5,h6)){max-width:calc(100% - 24px)}
}
</style>"#;

/// The fit's behaviour (see the module comment). It runs once the kit has built the questions, again
/// after pictures and fonts load, and whenever the window's width changes, each time from a clean
/// page: what it changed the last time is put back first.
pub(super) const FIT_SCRIPT: &str = r##"<script>(function(){
var MARK="data-gx-fit",PHONE=640,CHIP_CHARS=24,lastWidth=-1,queued=0,running=false,watched=new WeakSet();
var report={width:0,stacked:[],nowrap:[],scroll:[],overflow:[]};
window.__ghostexFit=report;
function name(e){var s=e.tagName.toLowerCase(),c=typeof e.className==="string"?e.className.trim().split(/\s+/).filter(function(x){return x&&!/^gx-(fit|more)/.test(x);}).slice(0,2):[];return e.id?s+"#"+e.id:c.length?s+"."+c.join("."):s;}
function said(e){var t=(e.textContent||"").replace(/\s+/g," ").trim();return t.length>40?t.slice(0,39)+"…":t;}
function entry(e){return{element:name(e),text:said(e)};}
/* The kit's own parts already fit; only the page's own markup is fitted. */
function kitOwned(e){return !!(e.closest&&e.closest("gx-question,.gx-send,.gx-theme-switch,#gx-tip,#gx-layout-notice,.gx-seg"));}
function undo(){
  Array.prototype.forEach.call(document.querySelectorAll("["+MARK+"]"),function(e){
    if(e.getAttribute(MARK)==="wrap"){var child=e.firstElementChild;if(child)e.replaceWith(child);else e.remove();return;}
    e.classList.remove("gx-fit-nowrap","gx-fit-stack","gx-fit-scroll","gx-fit-grow","gx-more-left","gx-more-right");e.removeAttribute(MARK);
  });
}
function mark(e,kind){if(!e.hasAttribute(MARK))e.setAttribute(MARK,kind);}
function painted(e){var cs=getComputedStyle(e),bg=cs.backgroundColor;return parseFloat(cs.borderTopWidth)>0||parseFloat(cs.borderTopLeftRadius)>=4||(!!bg&&bg!=="transparent"&&!/,\s*0\)$/.test(bg));}
/* The outermost short inline box or flex item around a text: a pill, a chip, a badge. */
function chipOf(text,anyLook){
  var e=text.parentElement,chip=null,chrome=anyLook;
  while(e&&e!==document.body&&(e.textContent||"").trim().length<=CHIP_CHARS){
    var d=getComputedStyle(e).display,p=e.parentElement,pd=p&&getComputedStyle(p);
    if(/^inline-(flex|block|grid)$/.test(d)||(pd&&/flex$/.test(pd.display)&&pd.flexDirection.indexOf("row")===0)){chip=e;chrome=chrome||painted(e);}
    e=p;
  }
  return chip&&chrome?chip:null;
}
function wraps(text){var r=document.createRange();r.selectNodeContents(text);var rects=r.getClientRects(),top=null;for(var i=0;i<rects.length;i++){if(!rects[i].width)continue;if(top===null)top=rects[i].top;else if(Math.abs(rects[i].top-top)>2)return true;}return false;}
/* A wrapped label counts when it looks like a pill (a border, a fill or round corners); inside a block that scrolls sideways, where there is room, any short label does. */
function keepChipsWhole(root,anyLook){
  var walker=document.createTreeWalker(root,NodeFilter.SHOW_TEXT),text,seen=0,chips=[];
  while((text=walker.nextNode())&&seen<6000){
    var t=text.data.trim();if(!t||t.length>CHIP_CHARS)continue;seen++;
    var parent=text.parentElement;if(!parent||parent.closest("script,style,textarea,svg,pre,code")||kitOwned(parent))continue;
    if(!/\s|-/.test(t)||!wraps(text))continue;
    var chip=chipOf(text,anyLook);if(chip&&chips.indexOf(chip)<0)chips.push(chip);
  }
  chips.forEach(function(chip){chip.classList.add("gx-fit-nowrap");mark(chip,"nowrap");report.nowrap.push(entry(chip));});
}
function columns(e){var v=getComputedStyle(e).gridTemplateColumns;return v&&v!=="none"?v.split(/\s+(?![^(]*\))/).length:0;}
function overflows(e){return e.scrollWidth>e.clientWidth+1;}
/* On a phone, side-by-side columns that no longer fit become one column. The layout width, not `innerWidth`, which a zoomed-out phone page reports as wider. */
function stackGrids(){
  if(document.documentElement.clientWidth>PHONE)return;
  var all=document.body.querySelectorAll("*"),n=Math.min(all.length,8000);
  for(var i=0;i<n;i++){
    var e=all[i];if(/^(GX-PAGE|GX-QUESTIONS|GX-WIDE|SCRIPT|STYLE|SVG)$/i.test(e.tagName)||kitOwned(e))continue;
    var d=getComputedStyle(e).display;if(d!=="grid"&&d!=="inline-grid")continue;
    if(columns(e)<2||e.children.length<2||!overflows(e))continue;
    e.classList.add("gx-fit-stack");mark(e,"stack");report.stacked.push(entry(e));
  }
}
function scrolls(e){var o=getComputedStyle(e).overflowX;return o==="auto"||o==="scroll";}
function clips(e){var o=getComputedStyle(e).overflowX;return o==="hidden"||o==="clip";}
/* How wide a child of the container may be: the template's column, a wide band's width, or the body's. */
function room(container){
  var cs=getComputedStyle(container);
  if(/^GX-(PAGE|QUESTIONS)$/.test(container.tagName)){var tracks=cs.gridTemplateColumns.split(/\s+/);if(tracks.length===3)return parseFloat(tracks[1]);}
  return container.clientWidth-parseFloat(cs.paddingLeft)-parseFloat(cs.paddingRight);
}
/* Inside a block that scrolls sideways, rows keep their real width: short labels stay on one line, and a row grows to hold its items so its own border and fill reach past the screen with them. */
function unsquash(scroller){
  keepChipsWhole(scroller,true);
  Array.prototype.forEach.call(scroller.querySelectorAll("*"),function(e){
    var cs=getComputedStyle(e);if(cs.display!=="flex"||cs.flexDirection.indexOf("row")!==0||cs.flexWrap!=="nowrap"||kitOwned(e))return;
    if(overflows(e)){e.classList.add("gx-fit-grow");mark(e,"grow");}
  });
}
function fade(e){
  if(!watched.has(e)){watched.add(e);e.addEventListener("scroll",function(){shade(e);},{passive:true});}
  shade(e);
}
function shade(e){var max=e.scrollWidth-e.clientWidth;e.classList.toggle("gx-more-left",max>1&&e.scrollLeft>1);e.classList.toggle("gx-more-right",max>1&&e.scrollLeft<max-1);}
function contain(){
  var containers=Array.prototype.slice.call(document.querySelectorAll("gx-page,gx-questions,gx-wide"));
  if(!containers.length&&document.body)containers=[document.body];
  containers.forEach(function(container){
    var width=room(container);
    Array.prototype.slice.call(container.children).forEach(function(c){
      if(/^(SCRIPT|STYLE|TEMPLATE|GX-WIDE|LINK|META)$/.test(c.tagName)||kitOwned(c))return;
      var cs=getComputedStyle(c);if(cs.display==="none"||cs.position==="absolute"||cs.position==="fixed")return;
      if(scrolls(c)){mark(c,"fade");fade(c);return;}
      if(clips(c))return;
      if(c.getBoundingClientRect().width>width+1){
        var wrap=document.createElement("div");wrap.className="gx-fit-scroll";wrap.setAttribute(MARK,"wrap");
        c.before(wrap);wrap.append(c);report.scroll.push(entry(c));unsquash(wrap);fade(wrap);
      }else if(overflows(c)){c.classList.add("gx-fit-scroll");mark(c,"scroll");report.scroll.push(entry(c));unsquash(c);fade(c);}
    });
  });
  Array.prototype.forEach.call(document.querySelectorAll(".gx-frame,pre"),function(e){if(!e.hasAttribute(MARK)){mark(e,"fade");}fade(e);});
}
/* Whatever still reaches past the window's right edge, outside any scroller: what `ghostex visual check` warns about. */
function leftovers(){
  var limit=document.documentElement.clientWidth+1,found=[],all=document.body?document.body.querySelectorAll("*"):[],n=Math.min(all.length,8000);
  for(var i=0;i<n&&found.length<8;i++){
    var e=all[i],r=e.getBoundingClientRect();if(!r.width||(r.right<=limit&&r.left>=-1))continue;
    var p=e.parentElement,inside=false;
    while(p&&p!==document.body){if(scrolls(p)||clips(p)){inside=true;break;}p=p.parentElement;}
    if(inside||found.some(function(f){return f.node.contains(e);}))continue;
    found.push({node:e,element:name(e),text:said(e),right:Math.round(r.right)});
  }
  return found.map(function(f){return{element:f.element,text:f.text,right:f.right};});
}
function fit(){
  if(running||!document.body||document.readyState==="loading")return;running=true;
  try{
    undo();report.stacked=[];report.nowrap=[];report.scroll=[];
    lastWidth=document.documentElement.clientWidth;report.width=lastWidth;
    keepChipsWhole(document.body,false);stackGrids();contain();
    report.overflow=leftovers();
  }finally{running=false;}
}
function later(){if(queued)return;queued=requestAnimationFrame(function(){queued=0;fit();});}
addEventListener("resize",function(){if(document.documentElement.clientWidth!==lastWidth)later();});
addEventListener("load",later);
if(document.fonts&&document.fonts.ready)document.fonts.ready.then(later);
if(document.readyState==="loading")document.addEventListener("DOMContentLoaded",fit);else fit();
})();</script>"##;

#[cfg(test)]
mod tests {
    use super::super::builtin::{current_block, with_current_block, BUILTIN_END, BUILTIN_START};
    use super::super::kit::KIT_SCRIPT;
    use super::*;

    #[test]
    fn the_built_in_block_ends_with_the_fit_after_the_kit() {
        let block = current_block();
        let kit = block.find(KIT_SCRIPT).expect("the kit's script");
        let style = block.find(FIT_STYLE).expect("the fit's style");
        let script = block.find(FIT_SCRIPT).expect("the fit's script");
        assert!(kit < style && style < script);
        assert!(block.ends_with(&format!("{FIT_SCRIPT}{BUILTIN_END}")));
    }

    /// builtin.rs finds each built-in part's end at its first `</style>` or `</script>`.
    #[test]
    fn each_part_closes_only_at_its_end() {
        assert_eq!(FIT_STYLE.matches("</style>").count(), 1);
        assert!(FIT_STYLE.ends_with("</style>"));
        assert_eq!(FIT_SCRIPT.matches("</script>").count(), 1);
        assert!(FIT_SCRIPT.ends_with("</script>"));
        assert!(!FIT_SCRIPT.contains("<!--"));
    }

    #[test]
    fn a_page_published_before_the_fit_gets_it_when_served() {
        let page = format!(
            "<!doctype html><html><head>{BUILTIN_START}<style>old</style>{BUILTIN_END}<title>t</title></head><body></body></html>"
        );
        let served = with_current_block(page);
        assert!(served.contains(FIT_STYLE) && served.contains(FIT_SCRIPT));
        assert!(!served.contains("<style>old</style>"));
    }

    #[test]
    fn a_narrow_screen_gets_the_phone_gutter_and_no_sideways_page() {
        let narrow = &FIT_STYLE[FIT_STYLE
            .find("@media (max-width:600px)")
            .expect("the phone rules")..];
        assert!(narrow.contains("--gx-gutter:16px"));
        assert!(narrow.contains("html{overflow-x:clip}"));
        assert!(FIT_STYLE.contains("text-size-adjust:100%"));
    }
}
