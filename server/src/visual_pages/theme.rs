/// The light theme's tokens, the same in the media query and under `data-gx-theme=light`.
macro_rules! light_tokens {
    () => {
        concat!(
            "color-scheme:light;",
            "--background:#f0f0f0;--foreground:#262626;",
            "--muted:#f1f1f1;--muted-foreground:#626262;",
            "--card:#ffffff;--card-foreground:#262626;",
            "--border:rgba(0,0,0,0.14);--input:rgba(0,0,0,0.16);--ring:#737373;",
            "--primary:#2563eb;--primary-foreground:#ffffff;",
            "--accent:#e9e9e9;--accent-foreground:#262626;",
            "--destructive:#b91c1c;--warning:#b45309;--success:#047857;--info:#0369a1;",
            "--code-background:#f6f8fa;--code-foreground:#1f2328;",
            "--chart-1:#2563eb;--chart-2:#d97706;--chart-3:#059669;",
            "--chart-4:#db2777;--chart-5:#7c3aed;--chart-6:#0891b2;",
            "--gx-bg:#f0f0f0;--gx-surface:rgba(255,255,255,0.55);--gx-raised:rgba(255,255,255,0.78);",
            "--gx-line:rgba(51,51,51,0.09);--gx-line-strong:rgba(51,51,51,0.15);",
            "--gx-fg:#18181b;--gx-text:#3f3f46;--gx-muted:#71717b;",
            "--gx-accent:#2563eb;--gx-warn:#a16207;--gx-good:#047857;--gx-bad:#b91c1c;--gx-code-bg:rgba(255,255,255,0.62);--gx-pick:#262626;",
            "--gx-card:rgba(255,255,255,0.62);--gx-footer:rgba(255,255,255,0.68);--gx-option:rgba(255,255,255,0.5);",
            "--gx-card-border:rgba(67,67,67,0.13);--gx-option-border:rgba(67,67,67,0.1);",
            "--gx-glass-shadow:0 14px 36px rgba(89,89,89,0.12);--gx-glass-dim:var(--gx-glass-tint-light,rgba(247,247,247,0.7));",
            "--gx-backdrop:radial-gradient(60% 50% at 85% 100%,rgba(185,185,185,0.28),transparent 70%),radial-gradient(55% 45% at 5% 0%,rgba(165,165,165,0.3),transparent 70%),radial-gradient(40% 35% at 100% 0%,rgba(163,163,163,0.16),transparent 70%),linear-gradient(180deg,#f0f0f0 0%,#eeeeee 45%,#eeeeee 100%);--gx-backdrop-top:#f0f0f0;",
            "--gx-card-muted:#71717b;--gx-control:#18181b;",
            "--gx-tip-bg:#fcfcfc;--gx-tip-fg:#292929;--gx-tip-border:rgba(0,0,0,0.12)"
        )
    };
}

/// The `<style id="ghostex-theme">` every published page starts with: the theme's tokens for dark
/// and light, and a plain base so an unstyled page already reads like the app. It comes first in
/// `<head>`, and every token rule is wrapped in `:where()`, so the page's own CSS always wins.
///
/// One attribute decides the theme: `data-gx-theme` on `<html>`, which `PAGE_BOOTSTRAP` always sets
/// (Ghostex's theme from the page's address, else the system's) and which the kit (kit.rs) may
/// change: the reader's Dark / Light switch, or the page's own painted background ("follow paint").
/// The media query only covers a page whose scripts never ran.
///
/// Text colours come from the app's default theme (`packages/core-ui/styles/theme.css` for dark,
/// `packages/core-ui/styles/modals-light.css` for light); status colors from the chat's status
/// tones (`packages/gx-chat-core/visual/status-tone.json`). The page itself is the glass look
/// below: a painted backdrop (`--gx-backdrop`, a dark dusk in dark, a soft dawn in light) under
/// translucent surfaces that the kit frosts. In the desktop's page window the backdrop is the
/// picture behind the app's glass under the work area's own tint (`--gx-glass-tint-dark` /
/// `-light`, which the page window sets for the app's theme; `--gx-glass-dim` otherwise).
///
/// CDXC:SessionChat 2026-11-02 DECISION:
/// User picked "A. Real glass, your wallpaper" on the page asking how every page should look as good as the screenshot page (issues-1027/showcase), with the note "a and skill adjustments also. i like this one you just gave me a lot for example … i love the glassy selections". Every page now paints a toned-down dusk (dark) or dawn (light) behind frosted question cards, choices, note boxes, send bar and tied option sections, with a warm amber (`--gx-pick`) for picks. This supersedes the 2026-10-11 rule that the question card takes the chat card's own colours. Pages that paint their own background keep it.
/// CDXC:SessionChat 2026-11-02 WHY:
/// The page window's page is a windowed CEF browser, and CEF paints a windowed browser opaque whatever its background colour says (only off-screen browsers can be transparent), so a page can't show the app's own glass through itself. The desktop's page window hands the page the picture behind its glass instead (`data-gx-glass`, CDXC:SessionChat 2026-11-02 DECISION in apps/desktop/src/app/window/visual_page_modal/glass.rs), and every other client paints the dusk or dawn.
///
/// CDXC:SessionChat 2026-10-27 DECISION:
/// User picked the v2 page template from the plan page (issues-1027/page-template): the "Editorial" look, a 720px column with a wide band up to 1120px, Ghostex's theme plus a Dark / Light switch on the page, "follow paint" when a page paints the other theme, and the same template for pages meant for other people. The kit's question cards keep their decided look; only their colours stop depending on `light-dark()` and on the page's `--background`.
/// CDXC:SessionChat 2026-10-27 WHY:
/// The kit's colours used to be `light-dark()`, which follows the CSS `color-scheme` property rather than the colours a page paints: a page that painted itself light while Ghostex was dark got white question titles and a white Send button on white, and a page that wrote its own `color-scheme` got light cards on a dark page. Every colour is now a token per theme, keyed on the one attribute, and nothing is mixed from `--background`.
/// CDXC:SessionChat 2026-11-02 DECISION:
/// User, of a page in dark mode: "make it darker bg on dark mode, it's too light now". The dark dusk, its glows, the page colour and the cards' glass are darker, and over the desktop's picture the page takes the work area's own tint (CDXC:SessionChat 2026-11-02 DECISION in apps/desktop/src/app/window/visual_page_modal/glass.rs).
/// CDXC:SessionChat 2026-11-04 DECISION:
/// User: "can we make the titlebar part of the inline html instead of being a different color? it looks bad now with the black titlebar". The desktop's page window paints the page's backdrop behind its header: the picture and tint the page gets, or without a picture `--gx-backdrop-top`, the colour the dusk (`#131313`) and dawn (`#f0f0f0`) start from. A floating page (`data-gx-floating`) fades its painted backdrop into that colour over its top 56px, so its glows never meet the header in a seam (apps/desktop/src/app/window/visual_page_modal/glass.rs `PAGE_TOP_DARK` / `PAGE_TOP_LIGHT` hold the same colours). A page that paints its own background keeps it.
/// CDXC:SessionChat 2026-11-04 DECISION:
/// User, shown four glass looks for the elements inside pages (issues-1027/page-element-looks, Q1213), picked a mix: "D but i want the tint to not be towards blue/purple like we have now that's all. Should be nutral color for the tint of the glass please but same degree for the glass transparency and color lightness", and "Neutral ink" for picks (Q1214). Every glass, edge, shadow and backdrop colour (the dusk and dawn, their glows, `--gx-card`, `--gx-surface`, the lines and borders, `--gx-bg`) is now a grey of the same luminance and alpha as before, and `--gx-pick` is the app's neutral ink (#e5e5e5 dark, #262626 light) instead of amber. Text, accent and status colours keep the app's own. Supersedes the warm amber of the 2026-11-02 decision above.
/// SEE-ALSO: kit.rs (`KIT_STYLE` uses the `--gx-*` tokens; the switch and "follow paint" are in `KIT_SCRIPT`), skills/ghostex-visuals/SKILL.md (the tokens agents may use).
pub(super) const THEME_STYLE: &str = concat!(
    "<style id=\"ghostex-theme\">",
    ":where(:root){color-scheme:dark;",
    "--background:#131313;--foreground:#c8cdd5;",
    "--muted:#2a2a2a;--muted-foreground:#747b85;",
    "--card:#252525;--card-foreground:#c8cdd5;",
    "--border:rgba(255,255,255,0.11);--input:rgba(255,255,255,0.15);--ring:#737373;",
    "--primary:#7da4f8;--primary-foreground:#0e0e0e;",
    "--accent:#2a2a2a;--accent-foreground:#e5e7eb;",
    "--destructive:#fb7185;--warning:#fbbf24;--success:#34d399;--info:#86d3f8;",
    "--code-background:#161616;--code-foreground:#e6edf3;",
    "--chart-1:#60a5fa;--chart-2:#f59e0b;--chart-3:#34d399;",
    "--chart-4:#f472b6;--chart-5:#a78bfa;--chart-6:#22d3ee;",
    "--radius:8px;",
    "--font-sans:-apple-system, BlinkMacSystemFont, \"Segoe UI\", system-ui, sans-serif;",
    "--font-mono:\"SF Mono\", Menlo, Consolas, \"Liberation Mono\", monospace;",
    // The page template's tokens (kit.rs): widths, surfaces, text and tones.
    "--gx-text-width:720px;--gx-wide-width:1120px;--gx-gutter:24px;",
    "--gx-bg:#131313;--gx-surface:rgba(246,246,246,0.06);--gx-raised:rgba(246,246,246,0.1);",
    "--gx-line:rgba(236,236,236,0.09);--gx-line-strong:rgba(236,236,236,0.17);",
    "--gx-fg:#fcfcfc;--gx-text:#c8cdd5;--gx-muted:#8b919a;",
    "--gx-accent:#7da4f8;--gx-warn:#d2a024;--gx-good:#34d399;--gx-bad:#fb7185;--gx-code-bg:rgba(11,11,11,0.42);--gx-pick:#e5e5e5;",
    // The question card's glass and the painted backdrop behind every page (CDXC:SessionChat 2026-11-02 DECISION above).
    "--gx-card:rgba(29,29,29,0.55);--gx-footer:rgba(29,29,29,0.6);--gx-option:rgba(246,246,246,0.035);",
    "--gx-card-border:rgba(233,233,233,0.13);--gx-option-border:rgba(233,233,233,0.09);",
    "--gx-glass-shadow:0 16px 40px rgba(5,5,5,0.32);--gx-glass-dim:var(--gx-glass-tint-dark,rgba(12,12,12,0.68));",
    "--gx-backdrop:radial-gradient(60% 50% at 82% 100%,rgba(160,160,160,0.11),transparent 70%),radial-gradient(55% 45% at 8% 0%,rgba(122,122,122,0.13),transparent 70%),radial-gradient(40% 35% at 100% 0%,rgba(113,113,113,0.08),transparent 70%),linear-gradient(180deg,#131313 0%,#151515 32%,#161616 60%,#181818 84%,#191919 100%);--gx-backdrop-top:#131313;",
    "--gx-card-muted:#b4b8bf;--gx-control:#e5e5e5;",
    // The app's tooltip bubble without window glass (apps/desktop/src/app/helpers/titlebar/chrome_colors.rs
    // `apply_gpui_component_theme`: the menu colour, darkened in dark mode; its border and text).
    "--gx-tip-bg:#0a0a0a;--gx-tip-fg:#fcfcfc;--gx-tip-border:rgba(255,255,255,0.12)}",
    "@media (prefers-color-scheme: light){:where(:root:not([data-gx-theme=dark])){",
    light_tokens!(),
    "}}",
    ":where(:root[data-gx-theme=light]){",
    light_tokens!(),
    "}",
    // In the desktop's page window the backdrop fades into its top colour at the top edge, which the
    // window's header above the page paints (CDXC:SessionChat 2026-11-04 DECISION below).
    ":where(:root[data-gx-floating]){--gx-backdrop-fade:linear-gradient(var(--gx-backdrop-top),transparent 56px),}",
    "html{background-color:var(--background);background-image:var(--gx-backdrop-fade,) var(--gx-backdrop,none);background-attachment:fixed;min-height:100%;color:var(--foreground);font-family:var(--font-sans);",
    "font-size:14px;line-height:1.5;-webkit-font-smoothing:antialiased}",
    // The picture behind the app's glass, when the desktop's page window hands one over
    // (apps/desktop/src/app/window/visual_page_modal/glass.rs), dimmed under the page's cards.
    "html[data-gx-glass]{background-image:linear-gradient(var(--gx-glass-dim),var(--gx-glass-dim)),var(--gx-glass-picture);",
    "background-size:auto,var(--gx-glass-size,cover);background-position:0 0,var(--gx-glass-position,center);background-repeat:no-repeat}",
    "body{margin:0;padding:16px}",
    // A page laid out by the template's container (kit.rs) has its own gutters.
    ":where(body:has(>gx-page,>gx-questions)){padding:0}",
    // The desktop's page window holds its controls in a 36px band over the page's top edge
    // (apps/desktop/src/app/window/visual_page_modal.rs `TOP_BAND_HEIGHT`), so a floating page
    // starts that much lower and the template's 56px top padding drops by as much.
    "html[data-gx-floating] gx-page,html[data-gx-floating] gx-questions{padding-top:20px}",
    "code,kbd,pre,samp{font-family:var(--font-mono)}",
    // The notice an old-layout page shows above itself (`PAGE_API`).
    "#gx-layout-notice{display:flex;align-items:flex-start;gap:10px;box-sizing:border-box;",
    "width:min(100% - 32px,720px);margin:12px auto;padding:9px 10px 9px 14px;border-radius:10px;",
    "font:400 13px/1.5 var(--font-sans);color:var(--gx-text);",
    "background:color-mix(in srgb,var(--gx-warn) 9%,var(--gx-bg));",
    "border:1px solid color-mix(in srgb,var(--gx-warn) 28%,transparent)}",
    "#gx-layout-notice>span{flex:1;min-width:0}",
    "#gx-layout-notice b{color:var(--gx-fg);font-weight:600}",
    "#gx-layout-notice button{flex:none;width:22px;height:22px;padding:0;border:0;border-radius:6px;",
    "background:transparent;color:var(--gx-muted);font:16px/22px var(--font-sans);cursor:pointer}",
    "#gx-layout-notice button:hover{color:var(--gx-fg);background:color-mix(in srgb,var(--gx-fg) 8%,transparent)}",
    "</style>",
);

/// Runs first in every published page, before the page's own styles and scripts.
///
/// It reads what a Ghostex window hands the page in its address: the theme (`#gx-theme=light` or
/// `#gx-theme=dark`), so a page opened over the chat follows the app's appearance rather than the
/// system's from its first paint (a theme the desktop's page window already set before this script
/// keeps a reloaded page, whose address no longer has the fragment, on the app's theme; a page
/// opened from a bare link takes the system's; either way
/// `data-gx-theme` is always set, the one attribute `THEME_STYLE` keys every colour on), and `gx-floating=1` when the page is in the floating window, which
/// `PAGE_API` closes once answers are sent. It then drops that fragment so the page's own hash
/// routing never sees it. A web link the reader clicks opens in a new window, which the floating
/// window and the browser both send to the reader's browser, so the page itself is never navigated
/// away.
pub(super) const PAGE_BOOTSTRAP: &str = concat!(
    "<script>(function(){var d=document.documentElement;",
    "try{var h=location.hash,m=/[#&]gx-theme=(light|dark)/.exec(h);",
    "d.setAttribute(\"data-gx-theme\",m?m[1]:d.getAttribute(\"data-gx-theme\")||(matchMedia(\"(prefers-color-scheme: light)\").matches?\"light\":\"dark\"));",
    "if(/[#&]gx-floating=1/.test(h))d.setAttribute(\"data-gx-floating\",\"\");",
    "if(/[#&]gx-/.test(h))history.replaceState(history.state,\"\",location.pathname+location.search);}catch(e){}",
    "document.addEventListener(\"click\",function(e){var t=e.target,l=t&&t.closest?t.closest(\"a[href]\"):null,u;",
    "if(!l)return;try{u=new URL(l.getAttribute(\"href\"),document.baseURI);}catch(x){return;}",
    "if(!/^https?:$/.test(u.protocol)||u.href.split(\"#\")[0]===location.href.split(\"#\")[0])return;",
    "l.setAttribute(\"target\",\"_blank\");l.setAttribute(\"rel\",\"noopener\");},true);})();</script>",
);

/// Where `serve` puts the page's answered state for `PAGE_API` to read.
pub(crate) const PAGE_STATE_MARKER: &str = "<!--gx-page-state-->";

/// The page's side of answering: `ghostex.submit(answers)` posts any JSON to the page's own
/// `/answers` route, and `ghostex.answered` / `ghostex.answers` tell a reopened page what was sent.
/// A page that was answered also shows a short note, so a reader who reopens it sees the answers
/// went through even when the page itself ignores the state. The note goes into the page's
/// `[data-gx-answered]` element when it has one, else at the top of the question kit's column
/// (`<gx-questions>`, kit.rs), so it is as wide as the questions, else at the top of the page.
/// In the floating window (`data-gx-floating`, `PAGE_BOOTSTRAP`) the page closes itself once the
/// answers are sent, which the window takes as its cue to close.
///
/// Until then the reader's work is kept on the host: the kit's picks (`__saveDraft("kit", …)`),
/// a page's own `ghostex.saveDraft(value)` (read back as `ghostex.draft`), and the value of every
/// named field outside the kit are posted to the page's `/draft` route a moment after each change
/// and when the page is hidden or closed, and handed back by serve.rs when the page opens again;
/// plain fields are filled back in by themselves.
/// The routes are answers.rs and drafts.rs; serve.rs injects the state.
///
/// A page written without the v2 marker (version.rs) gets `oldLayout` in that state and shows a
/// one-line notice at the top of the page saying an older ghostex-visuals skill made it, until the
/// reader closes it; closing posts to the page's `/notice` route (notice.rs), so it stays closed.
///
/// CDXC:SessionChat 2026-10-27 DECISION:
/// User: show the notice "only … when u detect that the opened page is missing something that indicates … v2", and not again for that page once closed. Bundled skills refresh themselves from GitHub main shortly after gxserver starts (agent_skills_remote.rs `spawn_startup_skill_refresh`), so the notice offers nothing to reinstall: it says the agent's next page gets the new look. It is drawn by the page itself, so it shows wherever the page opens (the floating window, the Files view, a browser, the phone).
///
/// CDXC:SessionChat 2026-10-26 WHY:
/// A page on the phone reaches gxserver through the phone's SSH forward, which goes away whenever the link drops and comes back a moment later: the user typed answers into every question on the phone, the drafts stopped landing when the link dropped (their failure was ignored), and Send answered "Failed to fetch" and gave up, so closing the page lost everything typed after the drop. So a draft that does not land stays pending and is posted again until it does, and a send that cannot reach the host (no answer, or a 5xx while gxserver restarts) keeps trying with the answers held, telling the page through `submit`'s second argument; only a refusal from gxserver itself (already sent, no such page, a bad body) ends a send. A retried send that finds the answers already stored (409 after an earlier try whose reply was lost) counts as sent.
/// SEE-ALSO: apps/mobile/app/modules/ghostex-native/android/src/main/java/expo/modules/ghostexnative/GhostexPortForward.kt (the forward keeps its phone port across reconnects, so the page's address works again once the link is back), apps/mobile/app/src/chat/native/cards/PageSheet.tsx (reads `ghostex.__pending()`, the draft not yet saved, when the sheet closes or the page has to move to a new port, and keeps it until it can be posted: apps/mobile/app/src/chat/native/cards/pageDrafts.ts).
pub(super) const PAGE_API: &str = r#"<script>(function(){var st=null,draft={},dirty=false,timer=0,posting=false,changes=0,pause=0,base=location.pathname.replace(/\/+$/,"");
function later(wait){return Math.min(wait?wait*2:1000,10000);}
function note(){if(!st||!st.answered||!document.body||document.getElementById("gx-answered"))return;var b=document.createElement("div");b.id="gx-answered";b.setAttribute("role","status");b.setAttribute("style","margin:0 0 12px;padding:8px 12px;border-radius:8px;font:500 12.5px/1.4 var(--font-sans);color:var(--success);background:color-mix(in srgb,var(--success) 12%,transparent);border:1px solid color-mix(in srgb,var(--success) 32%,transparent)");var when="";try{when=st.answeredAt?" on "+new Date(st.answeredAt).toLocaleString():"";}catch(e){}b.textContent="Answered"+when+". The agent has your answers.";var slot=document.querySelector("[data-gx-answered]"),kit=document.querySelector("gx-questions");if(slot)slot.appendChild(b);else if(kit){b.style.margin="0";kit.insertBefore(b,kit.firstChild);}else document.body.insertBefore(b,document.body.firstChild);}
function layoutNotice(){if(!st||!st.oldLayout||st.noticeDismissed||!document.body||document.getElementById("gx-layout-notice"))return;var b=document.createElement("div");b.id="gx-layout-notice";b.setAttribute("role","status");var t=document.createElement("span");t.innerHTML="<b>Made with an older ghostex-visuals skill.</b> This page uses the old layout. Ghostex keeps the skill up to date on its own, so the agent&#39;s next page gets the new look.";var x=document.createElement("button");x.type="button";x.textContent="\u00d7";x.setAttribute("aria-label","Close this notice");x.setAttribute("data-gx-tip","Close");x.addEventListener("click",function(){b.remove();st.noticeDismissed=true;fetch(base+"/notice",{method:"POST",body:"{}",keepalive:true}).catch(function(){});});b.append(t,x);document.body.insertBefore(b,document.body.firstChild);}
function post(leaving){if(!dirty||(st&&st.answered))return;clearTimeout(timer);var body=JSON.stringify({draft:draft});if(leaving){if(navigator.sendBeacon)navigator.sendBeacon(base+"/draft",body);return;}if(posting)return;posting=true;var at=changes;
fetch(base+"/draft",{method:"POST",body:body,keepalive:true}).then(function(r){posting=false;if(r.status>=500)throw r;pause=0;if(changes===at)dirty=false;else post();}).catch(function(){posting=false;pause=later(pause);clearTimeout(timer);timer=setTimeout(post,pause);});}
function save(part,value){if(st&&st.answered)return;if(value==null)delete draft[part];else draft[part]=value;changes++;dirty=true;clearTimeout(timer);timer=setTimeout(post,300);}
addEventListener("pagehide",function(){post(true);});
document.addEventListener("visibilitychange",function(){if(document.visibilityState==="hidden")post();});
var restoring=false;
function fieldKey(el){if(!el||!el.tagName||(el.closest&&el.closest("gx-questions,[data-gx-nodraft]")))return null;var tag=el.tagName,type=(el.type||"").toLowerCase(),name=el.name||el.id;if(!name||!/^(INPUT|TEXTAREA|SELECT)$/.test(tag)||/^(password|file|hidden|submit|button|reset|image)$/.test(type))return null;if(type==="radio")return"radio:"+name;if(type==="checkbox")return"check:"+name+":"+el.value;return"value:"+name;}
function fieldValue(el){var type=(el.type||"").toLowerCase();if(type==="checkbox")return el.checked;if(type==="radio")return el.checked?el.value:undefined;return el.value;}
function noteField(e){if(restoring)return;var key=fieldKey(e.target);if(!key)return;var value=fieldValue(e.target);if(value===undefined)return;var fields=draft.fields&&typeof draft.fields==="object"?draft.fields:{};fields[key]=value;save("fields",fields);}
document.addEventListener("input",noteField,true);document.addEventListener("change",noteField,true);
function restoreFields(){var fields=draft.fields;if(!fields||typeof fields!=="object"||(st&&st.answered))return;restoring=true;try{Array.prototype.forEach.call(document.querySelectorAll("input,textarea,select"),function(el){var key=fieldKey(el);if(!key||!(key in fields))return;var value=fields[key],type=(el.type||"").toLowerCase();if(type==="checkbox")el.checked=!!value;else if(type==="radio")el.checked=el.value===value;else el.value=value;el.dispatchEvent(new Event("input",{bubbles:true}));el.dispatchEvent(new Event("change",{bubbles:true}));});}finally{restoring=false;}}
window.ghostex={get answered(){return!!(st&&st.answered);},get answers(){return st&&st.answered?st.answers:null;},
get draft(){return draft.page===undefined?null:draft.page;},saveDraft:function(value){save("page",value===undefined?null:value);},
submit:function(answers,retrying){var body=JSON.stringify({answers:answers===undefined?null:answers}),tries=0,wait=0;return new Promise(function(resolve,reject){if(window.__gxReadOnly){reject(new Error("This page was posted by a Worker, so it is read-only here. Nothing was sent: answer through your coordinator."));return;}
function sent(j){st={answered:true,answers:answers,answeredAt:j.answeredAt};draft={};dirty=false;clearTimeout(timer);note();if(document.documentElement.hasAttribute("data-gx-floating"))setTimeout(function(){window.close();},700);resolve(j);}
function again(){if(typeof retrying==="function")try{retrying("Not sent yet: Ghostex can't be reached. Trying again… Your answers are kept.");}catch(e){}wait=later(wait);setTimeout(attempt,wait);}
function attempt(){tries++;fetch(base+"/answers",{method:"POST",body:body}).then(function(r){return r.json().catch(function(){return{};}).then(function(j){if(r.ok&&j.ok)sent(j);else if(r.status===409&&tries>1)sent(j);else if(r.status>=500)again();else reject(new Error(j.error||"Your answers were not sent."));});},again);}
attempt();});},
__draft:function(part){return draft[part]===undefined?null:draft[part];},__saveDraft:save,__pending:function(){return dirty&&!(st&&st.answered)?draft:null;},
__state:function(s){st=s;draft=s&&!s.answered&&s.draft&&typeof s.draft==="object"?s.draft:{};var ready=function(){note();restoreFields();layoutNotice();};if(document.readyState==="loading")document.addEventListener("DOMContentLoaded",ready);else ready();}};})();</script>"#;

#[cfg(test)]
mod tests {
    use super::THEME_STYLE;

    /// The desktop's page window paints these behind its header (`PAGE_TOP_DARK` /
    /// `PAGE_TOP_LIGHT` in apps/desktop/src/app/window/visual_page_modal/glass.rs).
    #[test]
    fn a_floating_page_starts_under_its_window_band_where_a_tab_would() {
        // 56px in a browser tab, less the page window's 36px band.
        assert!(THEME_STYLE.contains(
            "html[data-gx-floating] gx-page,html[data-gx-floating] gx-questions{padding-top:20px}"
        ));
    }

    #[test]
    fn a_floating_page_fades_into_the_colour_its_window_header_paints() {
        assert!(THEME_STYLE.contains("--gx-backdrop-top:#131313;"));
        assert_eq!(THEME_STYLE.matches("--gx-backdrop-top:#f0f0f0;").count(), 2);
        assert!(THEME_STYLE.contains(
            ":where(:root[data-gx-floating]){--gx-backdrop-fade:linear-gradient(var(--gx-backdrop-top),transparent 56px),}"
        ));
        assert!(THEME_STYLE
            .contains("html{background-color:var(--background);background-image:var(--gx-backdrop-fade,) var(--gx-backdrop,none);"));
    }
}
