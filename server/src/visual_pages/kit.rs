//! The question kit every published page carries: an agent writes its questions as markup and the
//! kit draws them the same way on every page.
//!
//! ```html
//! <gx-questions submit="Send to the agent">
//!   <h1>Settings redesign</h1>
//!   <gx-wide>…a full-width mockup the questions refer to…</gx-wide>
//!   <gx-question tag="Q1" title="Which layout?">
//!     <p>Why this matters.</p>
//!     <gx-choice recommended>Tabs<small>One page per area.</small></gx-choice>
//!     <gx-choice>One long page</gx-choice>
//!   </gx-question>
//!   <gx-question title="Anything else?" type="text"></gx-question>
//! </gx-questions>
//! ```
//!
//! A mockup can stand for one choice: an element with `data-question` (the question's `name`,
//! title or tag) and `data-choice` (the choice's value, label or letter), usually the element
//! holding the variant's title and its mockup, gets a Pick button at the right end of its first
//! heading's row that picks that choice (and shows it picked, both there and in the question). It
//! gets no note box (the reader's notes go in the question's own "Note (optional)" box).
//!
//! ```html
//! <section data-question="Q1" data-choice="tabs"><h3>A. Tabs</h3>…mockup…</section>
//! ```
//!
//! A question with choices and no tied mockup gets them tied by the kit (`autoTie`): a heading above
//! it (after the previous question) that starts with a choice's letter ("A. Same layout") or reads as
//! its label gets that choice's Pick button, at the right end of the heading's row. A heading that
//! matches nothing or is ambiguous stays untouched, and a question under mockups that gets no Pick is
//! listed in `window.__ghostexKit.untied` for `ghostex show` / `ghostex visual check` to warn about.
//!
//! The page template (v2, pages that carry `<meta name="ghostex-page" content="v2">`, version.rs):
//! `<gx-page>` and `<gx-questions>` lay their children out in one centred column
//! (`--gx-text-width`, 720px, with a `--gx-gutter` on each side). A `<gx-wide>` child is a centred
//! band up to `--gx-wide-width` (1120px; `size="full"` for the whole window) whose blocks are
//! centred as a group, each as wide as its widest frame and never narrower than the column, so a
//! variant's heading, its Pick button and its mockup share the same edges. A `<header>` child holds
//! the eyebrow, title and lede, and the kit puts the reader's Dark / Light switch beside the title
//! on a v2 page. The kit also styles headings, code, tables and a few components (`.gx-eyebrow`,
//! `.gx-lede`, `.gx-callout`, `.gx-card`, `.gx-grid`, `.gx-figure`, `.gx-frame`, `.gx-seg`,
//! `.gx-button`, `<gx-tabs>`, and for options compared side by side `.gx-options`, `.gx-ribbon`,
//! `.gx-chips`/`.gx-chip` and `.gx-stats`/`.gx-stat`), all at low specificity so a page's own CSS wins. Every colour is a
//! theme token (theme.rs `THEME_STYLE`). A page that paints its own background in the other theme
//! moves the theme to match ("follow paint") and gets no switch.
//!
//! CDXC:SessionChat 2026-10-27 DECISION:
//! User, on a published page whose sections ran from the window's left edge while its title sat in the centred column, with the Pick button at the far right: "why is the stuff u shared here not centered … pls fix and make future generated ones … not have this issue", then picked the Editorial look and 720/1120 widths. `<gx-wide>` used to span the window, and its auto margins centred only children with their own width, so headings, notes and frame wrappers started at the left edge; it is now a bounded centred band whose content is centred as a group.
//!
//! A choice question
//! (`multiple` for several picks) letters its choices, ends with "Other / none of these", and has
//! a note box; a `type="text"` question is a text box. A question left blank when the answers are
//! sent goes as `"skipped"`, and the kit adds a Skip all button that sends every question as
//! `"skipped"` at once (see the
//! SessionChat 2026-10-16 DECISION on `SKIPPED` in packages/gx-visual/src/questions.rs). The kit adds the send button, posts the answers with `ghostex.submit`
//! keyed by each question's `name` (its title when it has none), with a non-empty note as
//! `"<key> (note)"`, and shows a sent page's answers locked when it is opened again. Until then it
//! keeps the reader's picks as the page's draft on the host (theme.rs `PAGE_API`), so closing and
//! reopening the page brings them back.
//!
//! CDXC:SessionChat 2026-10-11 DECISION:
//! User: "please same style for the html modal also and the inline questions": each question is a "Clear ask, clear answer" card like the chat's (apps/desktop/src/app/native_chat/ask_card.rs): a tile beside the tag chip and title (see the later decision on the yellow "?"), picks that turn green with it, options that light up amber when picked (the mockup Pick button too), and a footer bar with the status and a filled send button showing its key. It keeps the chat card's send button and its bright labels and titles at medium weight, and a picked option, the tile, the mockup ring and the Picked button take a light amber tint instead of a strong one or a solid fill ("please make the yellow more subtle"). Its colours are now frosted glass rather than the chat card's (CDXC:SessionChat 2026-11-02 DECISION in theme.rs). Only the tile stays beside the tag chip and title, since a page spreads its questions between mockups.
//! User, about the yellow "?" tile at the left of every question heading: "remove this yellow thing here please so i can tell what i answered and what i didn't answer". A question shows the green check as soon as it has an answer (a pick made, text typed, or a restored draft). After the page is sent, an answered question keeps the green check and one left blank keeps the plain tile. User (2026-10-19): "can we make this match the look of other icons we just worked on please?", so that tile is the chat's shared one (the SessionChat 2026-10-18 DECISION on `SINGLE_LINE_CARD` in apps/desktop/src/app/native_chat/cards.rs: 28px neutral tile, question-bubble glyph, a 12px green check badge), superseding the tinted green and muted squares. Then, of the empty slot an unanswered question kept: "fix this not showing an icon here, should show one with this color question mark #D2A024", so a question still waiting shows the chat's waiting tile (the checklist glyph with a #D2A024 "?" badge, Q305) until it has an answer, superseding the empty slot from "remove this yellow thing here please"; the green check still tells an answered question from a waiting one.
//! User picked from the looks page: a question's tag is a filled chip ("make it show like eyebrow"), and a choice keeps its radio or checkbox with the letter as a muted keycap on the right; the in-chat form (apps/desktop/src/app/native_chat/question_form.rs, the phone's QuestionForm.tsx) matches. User (2026-10-19): "please move the Q294 to be right aligned same row as the question", so the chip sits at the right end of the title's first line and a long title wraps beside it, superseding the chip above the title.
//! User: Ctrl+Enter (Cmd+Enter on macOS) presses the page's send button from anywhere on it, a note box included; the in-chat form does the same.
//! User: a reader picks an option right at its mockup (a Pick button by the mockup's title that picks that choice in its question, shown picked on both and cleared from either), and (first version) wrote a note under each mockup, keyed to that question and choice.
//! User 2026-10-30: no note box under each option in pages, for every page and for customers. The kit no longer makes the box (its collect, restore, lock and CSS are gone); the Pick button, each question's own "Note (optional)" box and the form's closing notes box stay. Answers to pages sent earlier with a `"<question> · <choice> (note)"` key still show in the answers card (packages/gx-chat-core/src/transcript/page_answers.rs reads them), but the kit sends no new ones.
//! User: "question cards in the html having different width is an absolute NO GO … all take up the width as they should": every question card fills the column, even when a page's own CSS centres it with auto margins (which shrink a grid item to its content).
//! User: the picked mockup's outline must never be cut off and needs more padding so it is not tight on the design: a tagged option gets 14px of room and a 2px ring inside its own box (content-box, so a fixed-width mockup keeps its width), because an outline drawn outside the box is clipped by a scrolling wrapper such as an `overflow-x:auto` frame.
//! User: every question page offers "Other / none of these" with a note box, consistently, instead of each agent hand-rolling it, and the reader may leave a question unanswered. No choice is ever preselected; an agent marks the one it recommends with a Recommended mark and the reader picks. Mockups can use the full width of the page between or above the questions, and the Answered note is as wide as the question cards.
//!
//! CDXC:SessionChat 2026-11-02 DECISION:
//! User: "make the 'recommended' tag more subtle, not like a pill, and not all caps." The recommended choice in a question and the recommended variant's `.gx-ribbon` both show the same quiet mark: a small muted "Recommended" in sentence case after a faint star in the pick colour (`--gx-pick`, neutral ink since 2026-11-04, theme.rs), with no pill, fill or border. The chat's own question card (apps/desktop/src/app/native_chat/ask_card.rs `recommended_mark`, the phone's askCard.tsx `RecommendedMark`) shows the same mark. The ribbon sits centred just above its card instead of on the card's top edge, so no card's title or layout moves for it (beside the title it pushed long titles onto two lines).
//!
//! CDXC:SessionChat 2026-10-24 DECISION:
//! User, after an agent's page showed its variants A, B, C but listed the question's choices C, B, A (recommended first): "some agent gave me the options flipped in the selection area below!! ... the order of the options above, has to match their order below!" The kit enforces it instead of trusting every agent: before lettering, a question's choices are sorted into the order their `data-question` / `data-choice` mockups appear in the document (matched by the choice's value or label; a bare letter cannot rank because letters come from the order). Choices no mockup names keep their relative order after the matched ones, "Other / none of these" stays last, and a question with no mockups is left as written. skills/ghostex-visuals/SKILL.md teaches the same rule.
//!
//! CDXC:SessionChat 2026-10-11 SEE-ALSO:
//! The answers card folds the `"<key> (note)"` entries (and the `"<key> · <choice> (note)"` ones older pages sent) into their question's row (packages/gx-chat-core/src/transcript/page_answers.rs); skills/ghostex-visuals/SKILL.md teaches agents this markup. Change them together.

/// The kit's look. It comes before the page's own styles, and its heading and paragraph defaults
/// have no specificity (`:where`), so a page's own CSS always wins.
///
/// CDXC:SessionChat 2026-10-29 DECISION:
/// User, after typing a long note under a picked variant made its box (and the variant's whole section) grow sideways to the width of the wide band: "i dont like how writing a lot here makes this text area expand in width even tho there's spaces for it to be able to wrap the text. please let's not do this, we should just wrap when text in a line gets long (this is in inline htmls)". Every kit text box keeps the width it has when empty, wraps, and grows only in height. The boxes are `width:0;min-width:100%` instead of `width:100%`: a percentage width inside a content-sized parent (the variant section under `<gx-wide>` is `fit-content`) counts as the text's own width when the parent is measured, so `field-sizing:content` made the parent as wide as the text, while a zero width with a percentage minimum adds nothing to that measure and then fills the parent.
///
/// CDXC:SessionChat 2026-11-02 SEE-ALSO:
/// The question card's text tokens (`--gx-fg`, `--gx-card-muted`, `--gx-control`) are still the chat card's (apps/desktop/src/app/native_chat/appearance.rs `foreground`, `card_muted`, `control_primary`; the phone's copy is apps/mobile/app/src/chat/native/transcript/theme.ts), so change those together. Its surfaces (`--gx-card`, `--gx-footer`, `--gx-option`, `--gx-card-border`, `--gx-option-border`) are the page's frosted glass instead (theme.rs, CDXC:SessionChat 2026-11-02 DECISION) and no longer follow the chat card.
///
/// CDXC:SessionChat 2026-11-04 DECISION:
/// User, of the solid black "Send answers ⌘ Enter" tooltip over a question page's send bar: "please make tooltips inside the inline htmls glassy and not black bg like this". The kit's one tooltip bubble (`#gx-tip`, every `data-gx-tip` on a page and in the floating window's picture viewer) is the page's neutral glass, the material the app's own tooltips take under window glass: a translucent fill (`--gx-tip-bg`, 66% of #171717 in dark, 78% white in light, a little denser than the cards so it stays readable over a busy wallpaper), a 20px blur like the app's tooltip Menu blur, the cards' neutral edge, a 10px radius like the choices, a soft shadow (`--gx-tip-shadow`), and key caps drawn as ink washes on that glass. Supersedes the opaque #0a0a0a / #fcfcfc bubble.
pub(super) const KIT_STYLE: &str = r#"<style id="ghostex-kit">
gx-page,gx-questions{display:grid;grid-template-columns:minmax(var(--gx-gutter),1fr) minmax(0,var(--gx-text-width)) minmax(var(--gx-gutter),1fr);row-gap:20px;align-content:start;padding:56px 0 88px;font-size:15px;line-height:1.65;color:var(--gx-text)}
gx-page{display:grid}
gx-page>*,gx-questions>*{grid-column:2;min-width:0}
gx-page>gx-wide,gx-questions>gx-wide{grid-column:1/-1;justify-self:center;box-sizing:border-box;width:min(100% - 2*var(--gx-gutter),var(--gx-wide-width));display:flex;flex-direction:column;align-items:center;gap:20px}
gx-page>gx-wide[size=full],gx-questions>gx-wide[size=full]{width:100%}
gx-wide>*{width:fit-content;max-width:100%;min-width:min(100%,var(--gx-text-width));box-sizing:border-box}
gx-wide>p,gx-wide>*>p,gx-wide figcaption{max-width:var(--gx-text-width)}
gx-wide,gx-choice{display:block;min-width:0}
gx-page>gx-wide,gx-questions>gx-wide{display:flex}
:where(gx-page,gx-questions) :where(h1,h2,h3,h4,p,figure){margin:0}
:where(gx-page,gx-questions) :where(h1,h2,h3,h4){color:var(--gx-fg)}
:where(gx-page,gx-questions) :where(h1){font-size:32px;line-height:1.15;font-weight:700;letter-spacing:-.025em}
:where(gx-page,gx-questions) :where(h2){margin-top:28px;font-size:22px;line-height:1.25;font-weight:650;letter-spacing:-.015em}
:where(gx-page,gx-questions) :where(h3){margin-top:6px;font-size:17px;line-height:1.35;font-weight:600;letter-spacing:-.005em}
:where(gx-page,gx-questions) :where(h4){font-size:15px;line-height:1.4;font-weight:600}
:where(gx-page,gx-questions) :where(a){color:var(--gx-accent);text-underline-offset:2px}
:where(gx-page,gx-questions) :where(code){font-size:.87em;padding:.1em .4em;border-radius:6px;background:var(--gx-surface);color:var(--gx-fg)}
:where(gx-page,gx-questions) :where(pre){margin:0;padding:14px 16px;overflow-x:auto;border-radius:12px;background:var(--gx-code-bg);color:var(--gx-fg);font:13px/1.6 var(--font-mono)}
:where(gx-page,gx-questions) :where(pre code){padding:0;background:none;font-size:inherit}
:where(gx-page,gx-questions) :where(ul,ol){margin:0;padding-left:22px}
:where(gx-page,gx-questions) :where(li+li){margin-top:4px}
:where(gx-page,gx-questions) :where(table){border-collapse:collapse;width:100%;font-size:14px}
:where(gx-page,gx-questions) :where(th){text-align:left;font-weight:600;font-size:12px;letter-spacing:.04em;text-transform:uppercase;color:var(--gx-muted);border-bottom:1px solid var(--gx-line-strong);padding:8px 10px}
:where(gx-page,gx-questions) :where(td){border-bottom:1px solid var(--gx-line);padding:10px;vertical-align:top}
:where(gx-page>header,gx-questions>header){display:grid;grid-template-columns:minmax(0,1fr) auto;column-gap:16px;row-gap:12px;align-items:start;margin-bottom:12px}
:where(gx-page>header>*,gx-questions>header>*){grid-column:1}
.gx-eyebrow{display:inline-flex;align-items:center;gap:7px;width:max-content;max-width:100%;box-sizing:border-box;padding:4px 11px;border-radius:999px;border:1px solid var(--gx-line-strong);background:var(--gx-surface);font:700 11px/1.3 var(--font-sans);letter-spacing:.12em;text-transform:uppercase;color:var(--gx-pick)}
.gx-lede{font-size:17px;line-height:1.6;color:var(--gx-muted)}
.gx-muted{color:var(--gx-muted)}
.gx-callout{padding:14px 18px;border-radius:12px;background:color-mix(in srgb,var(--gx-tone,var(--gx-accent)) 10%,var(--gx-bg));color:var(--gx-text)}
.gx-callout[data-tone=warn]{--gx-tone:var(--gx-warn)}.gx-callout[data-tone=good]{--gx-tone:var(--gx-good)}.gx-callout[data-tone=bad]{--gx-tone:var(--gx-bad)}
.gx-callout :where(b,strong),.gx-card :where(b,strong){color:var(--gx-fg)}
.gx-card{padding:18px 20px;border-radius:16px;background:var(--gx-card);border:1px solid var(--gx-card-border);box-shadow:var(--gx-glass-shadow);-webkit-backdrop-filter:blur(22px) saturate(1.15);backdrop-filter:blur(22px) saturate(1.15)}
.gx-grid{display:grid;gap:14px;grid-template-columns:repeat(auto-fit,minmax(min(100%,var(--gx-grid-min,240px)),1fr))}
.gx-options{display:grid;gap:14px;align-items:stretch;grid-template-columns:repeat(auto-fit,minmax(min(100%,var(--gx-option-min,250px)),1fr))}
gx-wide>.gx-options{width:100%}
.gx-options>*{min-width:0;margin:0}
.gx-options>[data-question][data-choice]{display:flex;flex-direction:column;gap:10px}
.gx-ribbon,.gx-rec{display:inline-flex;flex:none;align-items:center;gap:4px;font:500 12px/18px var(--font-sans);letter-spacing:0;text-transform:none;color:var(--gx-muted);white-space:nowrap}
.gx-ribbon::before,.gx-rec::before{content:"\2605";font-size:10px;color:color-mix(in srgb,var(--gx-pick) 75%,transparent)}
.gx-ribbon{position:absolute;bottom:calc(100% + 5px);left:50%;transform:translateX(-50%);line-height:14px}
.gx-card:has(>.gx-ribbon){position:relative}
.gx-options:has(>*>.gx-ribbon){row-gap:30px;padding-top:14px}
.gx-chips{display:flex;flex-wrap:wrap;gap:6px}
.gx-chip{display:inline-flex;flex:none;align-items:center;gap:6px;padding:2px 10px;border-radius:999px;border:1px solid var(--gx-line);background:var(--gx-surface);color:var(--gx-text);font-size:12px;line-height:1.5;white-space:nowrap}
.gx-chip b,.gx-stat b{color:var(--gx-fg);font-weight:650}
.gx-stats{display:grid;gap:6px;grid-template-columns:repeat(auto-fit,minmax(72px,1fr))}
.gx-stat{min-width:0;padding:7px 10px 6px;border-radius:10px;border:1px solid var(--gx-line);background:var(--gx-surface)}
.gx-stat small{display:block;font-size:10px;font-weight:650;letter-spacing:.1em;text-transform:uppercase;color:var(--gx-muted)}
.gx-stat b{display:block;font-size:15px;white-space:nowrap;font-variant-numeric:tabular-nums}
.gx-figure{display:flex;flex-direction:column;gap:10px}
.gx-figure>img{display:block;max-width:100%;height:auto;border-radius:12px;border:1px solid var(--gx-line)}
.gx-figure figcaption{font-size:13px;line-height:1.5;color:var(--gx-muted)}
.gx-frame{max-width:100%;overflow-x:auto;padding-bottom:4px}
.gx-seg{display:inline-flex;flex:none;gap:2px;padding:2px;border-radius:10px;background:var(--gx-surface);border:1px solid var(--gx-line)}
.gx-seg>button{font:500 12.5px/1 var(--font-sans);padding:7px 11px;border:0;border-radius:8px;background:transparent;color:var(--gx-muted);cursor:pointer}
.gx-seg>button:hover{color:var(--gx-fg)}
.gx-seg>button[aria-pressed=true],.gx-seg>button[aria-selected=true]{background:var(--gx-raised);color:var(--gx-fg);box-shadow:0 0 0 1px var(--gx-line-strong)}
.gx-button{display:inline-flex;align-items:center;gap:6px;height:32px;box-sizing:border-box;padding:0 13px;border-radius:9px;border:1px solid var(--gx-line-strong);background:var(--gx-raised);color:var(--gx-fg);font:500 13px/1 var(--font-sans);cursor:pointer}
.gx-button:hover{background:color-mix(in srgb,var(--gx-fg) 6%,var(--gx-raised))}
.gx-button.gx-primary{background:var(--gx-control);border-color:var(--gx-control);color:var(--gx-bg)}
.gx-theme-switch{grid-column:2!important;grid-row:1;justify-self:end;align-self:start}
gx-page>.gx-theme-switch,gx-questions>.gx-theme-switch{grid-column:2!important;grid-row:auto}
body>.gx-theme-switch{position:absolute;top:12px;right:12px}
@media (max-width:560px){:where(gx-page>header,gx-questions>header){grid-template-columns:minmax(0,1fr)}.gx-theme-switch{grid-column:1!important;grid-row:auto;justify-self:start}}
gx-tabs{display:flex;flex-direction:column;gap:14px;min-width:0}
gx-tabs>.gx-seg{align-self:flex-start}
gx-tabs>[data-tab][hidden]{display:none!important}
gx-question{display:block;border:1px solid var(--gx-card-border);background:var(--gx-card);color:var(--gx-card-muted);border-radius:16px;padding:12px 14px 14px;box-shadow:var(--gx-glass-shadow);-webkit-backdrop-filter:blur(22px) saturate(1.15);backdrop-filter:blur(22px) saturate(1.15)}
gx-questions gx-question{width:100%;box-sizing:border-box;min-width:0}
.gx-q-head{display:flex;align-items:center;gap:10px;margin-bottom:12px}
.gx-q-tile{position:relative;flex:none;display:grid;place-items:center;width:28px;height:28px;border-radius:8px;color:color-mix(in srgb,var(--gx-fg) 80%,transparent);background:color-mix(in srgb,var(--gx-fg) 6%,transparent)}
.gx-q-tile>svg{width:17px;height:17px}
.gx-q-done,gx-question.gx-has-answer .gx-q-wait,gx-question.gx-answered .gx-q-wait,gx-question.gx-unanswered .gx-q-wait{display:none}
gx-question.gx-has-answer .gx-q-done,gx-question.gx-answered .gx-q-done,gx-question.gx-unanswered .gx-q-done{display:block}
.gx-q-badge{position:absolute;right:-4px;bottom:-4px;display:grid;place-items:center;width:16px;height:16px;box-sizing:border-box;border-radius:50%;border:2px solid var(--gx-card);background:#16a34a;color:#fff}
.gx-q-badge.gx-ask{background:#D2A024;color:#161616}
.gx-q-badge.gx-check,gx-question.gx-has-answer .gx-q-badge.gx-ask,gx-question.gx-answered .gx-q-badge.gx-ask{display:none}
gx-question.gx-has-answer .gx-q-badge.gx-check,gx-question.gx-answered .gx-q-badge.gx-check{display:grid}
.gx-q-badge svg{width:6px;height:6px}
gx-question.gx-unanswered .gx-q-badge.gx-ask,gx-question.gx-unanswered .gx-q-badge.gx-check{display:none}
.gx-q-heading{display:flex;flex:1;min-width:0;align-items:flex-start;gap:10px}
.gx-q-tag{flex:none;margin-top:2px;font:500 10.5px/18px var(--font-mono);color:var(--gx-muted);background:color-mix(in srgb,var(--gx-muted) 18%,transparent);border-radius:999px;padding:0 7px}
.gx-q-title{flex:1;min-width:0;color:var(--gx-fg);font-size:15px;font-weight:500;line-height:22px}
.gx-q-desc{color:var(--gx-muted);font-size:13px;line-height:1.5;margin:-4px 0 12px}
.gx-q-desc>:first-child{margin-top:0}.gx-q-desc>:last-child{margin-bottom:0}
/* A question's links keep the text's colour with a faint dotted underline that firms up under the pointer, like the chat's (CDXC:SessionChat 2026-11-04, packages/gx-chat-core/src/questions/form_links.rs). */
.gx-q-title a,.gx-q-desc a{color:inherit;text-decoration:underline dotted;text-decoration-thickness:1px;text-decoration-color:color-mix(in srgb,currentColor 30%,transparent);text-underline-offset:3px;transition:text-decoration-color .18s ease-out}
.gx-q-title a:hover,.gx-q-desc a:hover{text-decoration-color:currentColor}
.gx-choices{display:flex;flex-direction:column;gap:8px}
.gx-choice{display:flex;gap:12px;align-items:flex-start;width:100%;box-sizing:border-box;margin:0;text-align:left;padding:10px 12px;border:1px solid var(--gx-option-border);border-radius:10px;background:var(--gx-option);color:var(--gx-fg);font:inherit;font-size:14px;line-height:20px;cursor:pointer}
.gx-choice:hover:not(:disabled):not([aria-checked=true]){border-color:color-mix(in srgb,var(--gx-fg) 14%,transparent);background:color-mix(in srgb,var(--gx-fg) 3%,var(--gx-option))}
.gx-choice:focus-visible{outline:none;border-color:var(--ring);box-shadow:0 0 0 3px color-mix(in srgb,var(--ring) 20%,transparent)}
.gx-choice[aria-checked=true]{border-color:color-mix(in srgb,var(--gx-pick) 55%,transparent);background:color-mix(in srgb,var(--gx-pick) 9%,transparent)}
.gx-choice:disabled{cursor:default}
.gx-choice:disabled:not([aria-checked=true]){opacity:.55}
.gx-mark{flex-shrink:0;width:16px;height:16px;margin-top:2px;box-sizing:border-box;border:1.5px solid color-mix(in srgb,var(--gx-muted) 70%,transparent);border-radius:50%}
.gx-choice[role=checkbox] .gx-mark{border-radius:4px}
.gx-choice[aria-checked=true] .gx-mark{border-color:var(--gx-pick);background:radial-gradient(circle,var(--gx-pick) 0 3.5px,transparent 4px)}
.gx-choice[role=checkbox][aria-checked=true] .gx-mark{background:var(--gx-pick);box-shadow:inset 0 0 0 2px var(--gx-bg)}
gx-question.gx-answered .gx-choice[aria-checked=true]{border-color:color-mix(in srgb,var(--success) 55%,transparent);background:color-mix(in srgb,var(--success) 9%,transparent)}
gx-question.gx-answered .gx-choice[aria-checked=true] .gx-mark{border-color:var(--success);background:var(--success)}
.gx-letter{flex:none;min-width:18px;height:18px;padding:0 5px;box-sizing:border-box;margin-top:1px;border:1px solid color-mix(in srgb,var(--gx-fg) 12%,transparent);border-radius:5px;background:color-mix(in srgb,var(--gx-bg) 50%,transparent);font:500 11px/16px var(--font-mono);color:var(--gx-muted);text-align:center}
.gx-choice[aria-checked=true] .gx-letter{color:var(--gx-fg)}
.gx-choice-body{flex:1;min-width:0;font-weight:500}
.gx-other .gx-choice-body{color:var(--gx-card-muted)}
.gx-other[aria-checked=true] .gx-choice-body{color:inherit}
.gx-rec{margin-left:8px;vertical-align:top;margin-top:1px}
.gx-choice-body small{display:block;color:var(--gx-muted);font-size:13px;font-weight:400;line-height:19px;margin-top:1px}
.gx-note,.gx-text{display:block;width:0;min-width:100%;box-sizing:border-box;margin:8px 0 0;min-height:34px;resize:vertical;field-sizing:content;font:inherit;font-size:13.5px;line-height:20px;color:var(--gx-fg);background:var(--gx-option);border:1px solid var(--gx-card-border);border-radius:9px;padding:6px 10px}
.gx-text{min-height:64px}
.gx-note:focus,.gx-text:focus{outline:none;border-color:var(--ring);box-shadow:0 0 0 3px color-mix(in srgb,var(--ring) 20%,transparent)}
.gx-note::placeholder,.gx-text::placeholder{color:var(--gx-muted)}
.gx-note[readonly],.gx-text[readonly]{resize:none}
.gx-note[hidden],.gx-text[hidden]{display:none}
.gx-pills{display:flex;flex-wrap:wrap;gap:4px 10px;margin:6px 0 0}
.gx-pills[hidden]{display:none}
.gx-pill{display:inline-flex;align-items:center;gap:4px;min-width:0;max-width:100%;font:500 13.5px/20px var(--font-sans);color:#8cb59e}
html[data-gx-theme=light] .gx-pill{color:#5f9878}
.gx-pill svg{flex:none;width:15px;height:15px}
.gx-pill-x{display:inline-flex;align-items:center;justify-content:center;width:16px;height:16px;padding:0;border:0;border-radius:4px;background:none;color:var(--gx-muted);font:inherit;line-height:1;cursor:pointer}
.gx-pill-x:hover{background:color-mix(in srgb,var(--gx-fg) 10%,transparent);color:var(--gx-fg)}
.gx-paste{margin:4px 0 0;font-size:12.5px;color:var(--gx-muted)}
.gx-paste[hidden]{display:none}
.gx-paste.gx-error{color:var(--destructive)}
.gx-notes{display:block;width:100%;box-sizing:border-box;min-width:0;border:1px solid var(--gx-card-border);background:var(--gx-card);color:var(--gx-card-muted);border-radius:16px;padding:12px 14px 14px;box-shadow:var(--gx-glass-shadow);-webkit-backdrop-filter:blur(22px) saturate(1.15);backdrop-filter:blur(22px) saturate(1.15)}
.gx-notes[hidden]{display:none}
.gx-notes>.gx-q-head{margin-bottom:0}
.gx-notes .gx-text{min-height:72px}
.gx-thumbs{gap:8px;margin-top:8px}
.gx-thumb{position:relative;display:block;flex:none;width:64px;height:64px;box-sizing:border-box;border-radius:9px;overflow:hidden;border:1px solid var(--gx-card-border);background:var(--gx-option)}
.gx-thumb>img{display:block;width:100%;height:100%;object-fit:cover}
.gx-thumb>.gx-pill-x{position:absolute;top:3px;right:3px;width:18px;height:18px;border-radius:50%;background:rgba(0,0,0,.62);color:#fff;font-size:13px}
.gx-thumb>.gx-pill-x:hover{background:rgba(0,0,0,.82);color:#fff}
.gx-has-pick{display:flex;align-items:center;gap:8px}
.gx-pick{display:inline-flex;flex:none;align-items:center;gap:4px;margin-left:auto;padding:2px 10px;vertical-align:middle;border:1px solid var(--gx-line-strong);border-radius:999px;background:transparent;color:var(--gx-text);font:500 12px/1.5 var(--font-sans);cursor:pointer}
.gx-pick:hover:not(:disabled):not([aria-pressed=true]){background:var(--gx-surface);color:var(--gx-fg)}
.gx-pick[aria-pressed=true]{background:color-mix(in srgb,var(--gx-pick) 14%,transparent);border-color:color-mix(in srgb,var(--gx-pick) 50%,transparent);color:var(--gx-pick)}
.gx-pick[aria-pressed=true]:hover:not(:disabled),.gx-pick[aria-pressed=true]:active:not(:disabled){background:color-mix(in srgb,var(--gx-pick) 22%,transparent);border-color:color-mix(in srgb,var(--gx-pick) 62%,transparent);color:var(--gx-pick)}
.gx-pick:focus-visible{outline:none;box-shadow:0 0 0 3px color-mix(in srgb,var(--ring) 35%,transparent)}
.gx-pick:disabled{cursor:default}
.gx-pick:disabled:not([aria-pressed=true]){opacity:.5}
.gx-option-head{display:flex;align-items:center;justify-content:space-between;gap:10px;margin-bottom:8px;font-weight:600}
[data-question][data-choice]:not(h1,h2,h3,h4,h5,h6){box-sizing:content-box;padding:14px;border:2px solid var(--gx-card-border);border-radius:18px;transition:border-color .18s,box-shadow .18s}
:where([data-question][data-choice]:not(h1,h2,h3,h4,h5,h6)){position:relative;background:var(--gx-card);box-shadow:var(--gx-glass-shadow);-webkit-backdrop-filter:blur(22px) saturate(1.15);backdrop-filter:blur(22px) saturate(1.15)}
:where([data-question][data-choice]:not(h1,h2,h3,h4,h5,h6)){max-width:calc(100% - 32px)}
[data-question][data-choice].gx-picked:not(h1,h2,h3,h4,h5,h6){border-color:color-mix(in srgb,var(--gx-pick) 78%,transparent);box-shadow:0 0 0 5px color-mix(in srgb,var(--gx-pick) 10%,transparent),var(--gx-glass-shadow)}
.gx-send{display:flex;align-items:center;gap:12px;flex-wrap:wrap;margin-top:4px;padding:10px 14px;border:1px solid var(--gx-card-border);border-radius:16px;background:var(--gx-footer);box-shadow:var(--gx-glass-shadow);-webkit-backdrop-filter:blur(22px) saturate(1.15);backdrop-filter:blur(22px) saturate(1.15)}
.gx-send-button{display:inline-flex;align-items:center;gap:6px;margin-left:auto;height:28px;box-sizing:border-box;padding:0 11px;font:inherit;font-size:13px;font-weight:500;border-radius:8px;border:1px solid var(--gx-control);background:var(--gx-control);color:var(--gx-bg);cursor:pointer}
.gx-send-button:hover:not(:disabled){opacity:.9}
.gx-send-button:disabled{opacity:.5;cursor:default}
.gx-skip-all{display:inline-flex;align-items:center;gap:6px;margin-left:auto;height:28px;box-sizing:border-box;padding:0 11px;font:inherit;font-size:13px;font-weight:500;border-radius:8px;border:1px solid transparent;background:color-mix(in srgb,var(--gx-fg) 9%,transparent);color:var(--gx-text);cursor:pointer}
.gx-skip-all:hover{background:color-mix(in srgb,var(--gx-fg) 14%,transparent)}
.gx-send-button,.gx-skip-all{position:relative}
.gx-skip-all+.gx-send-button{margin-left:0}
.gx-send-status{flex:1;min-width:0;font-size:12.5px;color:var(--gx-muted)}
.gx-send-status.gx-error{color:var(--destructive)}
.gx-send-info{display:inline-flex;align-items:center;justify-content:center;width:20px;height:20px;padding:0;border:0;background:none;color:var(--gx-muted);font:inherit;cursor:default}
#gx-tip{position:fixed;left:0;top:0;z-index:2147483647;margin:0;inset:auto;display:flex;align-items:center;gap:16px;box-sizing:border-box;max-width:min(280px,calc(100vw - 16px));padding:8px 11px;border-radius:10px;border:1px solid var(--gx-tip-border);background:var(--gx-tip-bg);color:var(--gx-tip-fg);-webkit-backdrop-filter:blur(20px) saturate(1.15);backdrop-filter:blur(20px) saturate(1.15);font:500 12.5px/1.4 Inter,var(--font-sans);letter-spacing:0;text-align:left;white-space:normal;box-shadow:var(--gx-tip-shadow);pointer-events:none;opacity:0;visibility:hidden;transition:opacity .12s ease,visibility 0s linear .12s}
#gx-tip.gx-tip-on{opacity:1;visibility:visible;transition:opacity .12s ease}
#gx-tip .gx-tip-label{min-width:0}
#gx-tip .gx-tip-keys{display:flex;flex:none;align-items:center;gap:3px}
#gx-tip kbd{min-width:17px;box-sizing:border-box;padding:0 4px;border-radius:5px;border:1px solid color-mix(in srgb,var(--gx-tip-fg) 16%,transparent);background:color-mix(in srgb,var(--gx-tip-fg) 9%,transparent);color:color-mix(in srgb,var(--gx-tip-fg) 86%,transparent);font:600 11px/16px Inter,var(--font-sans);text-align:center}
</style>"#;

/// The kit's behaviour: turns the first `<gx-questions>` into a working form once the page has
/// loaded, after `PAGE_API` (theme.rs) has its answered state.
///
/// CDXC:Clipboard 2026-11-02 DECISION:
/// User: "i also want to be able to paste images into the notes of questions in inline html or the inline questions you send please. when i paste an image now nothing happens. pls fix. (it can just show like this like we show it in chat: [Image #1](…/2026-11-02_Ghostex_06-18-47@2x.png)". An image pasted into a note or text box is saved on the computer through the page's own `/image` route (images.rs, beside the chat's pasted images) and written into the box as `[Image #N](path)`, the reference a chat message carries, so the answers carry it and the agent can open the file. The numbers count across every box of the page, which all go in one answers message. A textarea cannot draw a pill inside its text, so each box lists its images under it as pills (the chat's image icon and colour, the path in the tooltip, × removes the reference). The in-chat form does the same with the composer's own pills (apps/desktop/src/app/native_chat/form_attachments.rs).
///
/// CDXC:SessionChat 2026-11-04 DECISION:
/// User, of a question page's send bar: "please always at the end of the inline html pages we must have a text area where i can write text and paste images there (should show thumbnails) so i can write more". Every `<gx-questions>` page ends, just above its send bar, with an "Anything else?" box the kit adds by itself (`.gx-notes`), unless the page's own last question is already a text question of that kind ("Anything else?", "Additional notes", "Notes", "Comments"). Its text arrives last in the answers under "Additional notes", the key the chat's inline forms use for their notes box (packages/gx-visual/src/questions.rs `NOTES_KEY`, numbered the same way when a question already answers under it), and an empty box sends nothing. Pasted images go through the same `/image` route and `[Image #N](path)` references as the question notes, so they reach the agent the same way; in this box they show as removable thumbnails rather than pills. A thumbnail is drawn from the pasted file itself; a small copy of each is kept with the page's draft (`thumbs`, up to about 24 KB in all, so the draft stays well under its 64 KiB limit), so a reopened page still shows them; any image without one shows as a pill.
pub(super) const KIT_SCRIPT: &str = r#"<script>(function(){
var OTHER="Other / none of these",SKIPPED="skipped",NOTES="Additional notes",OWN_NOTES=/^(anything else|any other|additional notes|other notes|notes|comments?)\b/i;
var IMAGE_ICON='<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M15 8h.01"/><path d="M6 3h12a3 3 0 0 1 3 3v12a3 3 0 0 1-3 3H6a3 3 0 0 1-3-3V6a3 3 0 0 1 3-3z"/><path d="m3 16 5-5c.928-.893 2.072-.893 3 0l5 5"/><path d="m14 14 1-1c.928-.893 2.072-.893 3 0l3 3"/></svg>';
var TILE_ICON='<svg class="gx-q-wait" viewBox="0 0 16 16" aria-hidden="true"><path d="M2.5 4.5 3.6 5.6 5.5 3.6M2.5 10.5l1.1 1.1 1.9-2M8 4.6h5.5M8 10.6h5.5" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/></svg><svg class="gx-q-done" viewBox="0 0 16 16" aria-hidden="true"><path d="M3 3.5h10a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1H7l-3 2.5V11.5H3a1 1 0 0 1-1-1v-6a1 1 0 0 1 1-1Z" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round"/><path d="M6.6 6.2a1.5 1.5 0 1 1 2.1 1.4c-.5.2-.7.5-.7 1" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/><circle cx="8" cy="10" r=".7" fill="currentColor"/></svg><span class="gx-q-badge gx-ask"><svg viewBox="0 0 10 10" aria-hidden="true"><path d="M3.4 3.6a1.7 1.7 0 1 1 2.4 1.5c-.5.25-.8.55-.8 1.1" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/><circle cx="5" cy="8.1" r=".8" fill="currentColor"/></svg></span><span class="gx-q-badge gx-check"><svg viewBox="0 0 10 10" aria-hidden="true"><path d="M2 5.2 4.2 7.4 8 2.8" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/></svg></span>';
function el(tag,cls,text){var e=document.createElement(tag);if(cls)e.className=cls;if(text!=null)e.textContent=text;return e;}
/* CDXC:SessionChat 2026-11-04 DECISION: a question's title links what it asks about: a Markdown link to a web address ([#407](https://github.com/o/r/pull/407)) or a bare http(s) address is a link (opened in a new window, PAGE_BOOTSTRAP), drawn in the text's colour with a dotted underline (.gx-q-title a); the rest stays text. The chat's form does the same (packages/gx-chat-core/src/questions/form_links.rs). */
function linked(node,text){
  var re=/\[([^\]\n]+)\]\(<?(https?:\/\/[^\s<>()]+(?:\([^\s<>()]*\)[^\s<>()]*)*)>?\)|(https?:\/\/[^\s<>]+)/gi,at=0,m;
  while((m=re.exec(text))){
    var url=m[2]||m[3],label=m[1],end=m.index+m[0].length;
    if(!label){var bare=url;for(;;){var t=bare.replace(/[.,:;!?"'*_~]+$/,"");if(/\)$/.test(t)&&(t.match(/\)/g)||[]).length>(t.match(/\(/g)||[]).length)t=t.slice(0,-1);if(t===bare)break;bare=t;}
      if(bare.length<=bare.indexOf("//")+2){continue;}end=m.index+bare.length;url=label=bare;}
    node.append(text.slice(at,m.index));var a=el("a",null,label);a.href=url;node.append(a);at=end;re.lastIndex=end;
  }
  node.append(text.slice(at));
}
/* The same for the words of a question's description: its text outside links and code. */
function linkText(root){
  var walk=document.createTreeWalker(root,NodeFilter.SHOW_TEXT),found=[],t;
  while((t=walk.nextNode()))if(/https?:\/\//i.test(t.nodeValue)&&!t.parentNode.closest("a,code,pre,script,style,textarea,svg"))found.push(t);
  found.forEach(function(t){var f=document.createElement("span");linked(f,t.nodeValue);t.replaceWith.apply(t,Array.prototype.slice.call(f.childNodes));});
}
function kids(node){return Array.prototype.slice.call(node.childNodes);}
/* Tooltips: any element with `data-gx-tip` (and optional `data-gx-tip-keys`, key caps split by spaces) gets the app's tooltip bubble on hover or keyboard focus, never the browser's own `title` box. One bubble in the top layer, placed above its control (below when there is no room) and kept inside the window; found through shadow roots too, so the floating window's picture viewer uses it. */
var tip=null,tipFor=null,tipNext=null,tipTimer=0;
function tipTarget(e){var path=e.composedPath?e.composedPath():[e.target];for(var i=0;i<path.length;i++){var n=path[i];if(n&&n.nodeType===1&&n.hasAttribute("data-gx-tip"))return n;}return null;}
function showTip(target){
  var label=target.getAttribute("data-gx-tip");if(!label||target.disabled||!document.body)return;
  if(!tip){tip=document.createElement("div");tip.id="gx-tip";tip.setAttribute("role","tooltip");if(tip.showPopover)tip.setAttribute("popover","manual");}
  if(!tip.isConnected)document.body.append(tip);
  tip.textContent="";tip.append(el("span","gx-tip-label",label));
  var keys=(target.getAttribute("data-gx-tip-keys")||"").trim();
  if(keys){var caps=el("span","gx-tip-keys");keys.split(/\s+/).forEach(function(k){caps.append(el("kbd",null,k));});tip.append(caps);}
  if(tip.showPopover&&!tip.matches(":popover-open"))try{tip.showPopover();}catch(x){}
  var r=target.getBoundingClientRect(),w=tip.offsetWidth,h=tip.offsetHeight,vw=document.documentElement.clientWidth||innerWidth,vh=innerHeight,m=8,gap=6;
  var x=Math.min(Math.max(r.left+r.width/2-w/2,m),Math.max(m,vw-w-m)),y=r.top-gap-h;
  if(y<m)y=Math.min(r.bottom+gap,Math.max(m,vh-h-m));
  tip.style.left=Math.round(x)+"px";tip.style.top=Math.round(y)+"px";
  if(tipFor&&tipFor!==target)tipFor.removeAttribute("aria-describedby");
  tipFor=target;target.setAttribute("aria-describedby","gx-tip");
  requestAnimationFrame(function(){if(tipFor===target)tip.classList.add("gx-tip-on");});
}
function hideTip(){clearTimeout(tipTimer);tipNext=null;if(tipFor){tipFor.removeAttribute("aria-describedby");tipFor=null;}if(tip)tip.classList.remove("gx-tip-on");}
/* Followed on every move, not only `pointerover`: Chromium skips the boundary events when the pointer jumps onto a control in another shadow root (the picture viewer's buttons). */
function hover(e){
  if(e.pointerType==="touch")return;var t=tipTarget(e);if(t===tipFor||(t&&t===tipNext))return;hideTip();
  if(t){tipNext=t;tipTimer=setTimeout(function(){tipNext=null;showTip(t);},350);}
}
document.addEventListener("pointerover",hover,true);
document.addEventListener("pointermove",hover,true);
document.addEventListener("pointerout",function(e){if(!e.relatedTarget)hideTip();},true);
document.addEventListener("focusin",function(e){var t=tipTarget(e);if(t&&t.matches(":focus-visible"))showTip(t);},true);
document.addEventListener("focusout",function(e){if(tipTarget(e)===tipFor)hideTip();},true);
["pointerdown","scroll","wheel"].forEach(function(name){addEventListener(name,hideTip,true);});
document.addEventListener("keydown",function(e){if(e.key==="Escape")hideTip();},true);
/* A control whose tip text changes while it shows (the picture viewer's Copy) redraws it. */
window.__ghostexTips={refresh:function(target){if(target&&target===tipFor)showTip(target);}};

function labelText(node){var c=node.cloneNode(true);c.querySelectorAll("small").forEach(function(s){s.remove();});return c.textContent.replace(/\s+/g," ").trim();}
/* The choices of a question that has mockups tied to it, in the order those mockups appear on the page, so the list below always reads like the variants above it. A mockup matches a choice by its value or label (a bare letter is only known once the choices are lettered, so it cannot rank); choices no mockup names keep their order after the matched ones. */
function inVariantOrder(els,key,tag){
  var wanted=[];
  Array.prototype.forEach.call(document.querySelectorAll("[data-question][data-choice]"),function(node){
    if(node.closest("gx-question"))return;
    var asked=(node.getAttribute("data-question")||"").trim();
    if(asked===key||(tag&&asked===tag))wanted.push((node.getAttribute("data-choice")||"").trim());
  });
  if(!wanted.length)return els;
  var ranked=els.map(function(ce,i){
    var label=labelText(ce),value=(ce.getAttribute("value")||"").trim()||label,rank=Infinity;
    for(var w=0;w<wanted.length;w++)if(wanted[w]===value||wanted[w]===label){rank=w;break;}
    return{ce:ce,i:i,rank:rank};
  });
  ranked.sort(function(a,b){return a.rank===b.rank?a.i-b.i:a.rank<b.rank?-1:1;});
  return ranked.map(function(r){return r.ce;});
}
function question(qe,index,used,changed){
  var title=(qe.getAttribute("title")||"").trim(),key=(qe.getAttribute("name")||"").trim()||title||"Question "+(index+1);
  for(var n=2;used[key];n++)key=key.replace(/ \(\d+\)$/,"")+" ("+n+")";
  used[key]=true;
  var multiple=qe.hasAttribute("multiple"),text=(qe.getAttribute("type")||"").toLowerCase()==="text";
  var titleId="gx-q-"+index,head=el("div","gx-q-head"),heading=el("div","gx-q-heading"),tile=el("span","gx-q-tile"),tag=qe.getAttribute("tag");
  qe.removeAttribute("title");
  tile.innerHTML=TILE_ICON;head.append(tile,heading);
  var titleEl=el("div","gx-q-title");linked(titleEl,title);titleEl.id=titleId;heading.append(titleEl);
  if(tag)heading.append(el("span","gx-q-tag",tag));
  /* The tile shows the waiting checklist with the "?" badge until the question has an answer, then the question bubble with the green check; once sent, a question left blank shows the bubble without a badge. */
  var has=function(){return false;};
  function live(){qe.classList.toggle("gx-has-answer",has());}
  function notify(){live();changed();}
  function answered(given){if(given)qe.classList.add("gx-answered");else qe.classList.add("gx-unanswered");}
  var choiceEls=[],desc=el("div","gx-q-desc");
  kids(qe).forEach(function(node){if(node.nodeName==="GX-CHOICE")choiceEls.push(node);else desc.append(node);});
  choiceEls=inVariantOrder(choiceEls,key,(tag||"").trim());
  qe.textContent="";qe.append(head);
  linkText(desc);if(desc.textContent.trim()||desc.querySelector("img,svg,canvas,table"))qe.append(desc);
  if(text){
    var area=el("textarea","gx-text");area.placeholder=qe.getAttribute("placeholder")||"Your answer (optional)";area.setAttribute("aria-labelledby",titleId);
    has=function(){return !!area.value.trim();};area.addEventListener("input",notify);qe.append(area);
    return{el:qe,title:title,key:key,tag:(tag||"").trim(),choices:[],collect:function(out){out[key]=area.value.trim()||SKIPPED;},
      restore:function(answers){if(typeof answers[key]==="string"&&answers[key]!==SKIPPED)area.value=answers[key];live();},
      lock:function(){answered(!!area.value.trim());area.readOnly=true;if(!area.value)area.hidden=true;}};
  }
  var list=el("div","gx-choices"),choices=[];
  list.setAttribute("role",multiple?"group":"radiogroup");list.setAttribute("aria-labelledby",titleId);
  var note=el("textarea","gx-note");note.rows=1;note.setAttribute("aria-label","Note");note.addEventListener("input",notify);
  function sync(){note.placeholder=other.on()?"What would you prefer instead? (optional)":"Note (optional)";}
  function choice(nodes,value,recommended,label){
    var button=el("button","gx-choice"),body=el("span","gx-choice-body"),index=choices.length,letter=index<26?String.fromCharCode(97+index):String(index+1);
    button.type="button";button.setAttribute("role",multiple?"checkbox":"radio");button.setAttribute("aria-checked","false");

    nodes.forEach(function(node){body.append(node);});
    if(recommended)body.insertBefore(el("span","gx-rec","Recommended"),body.querySelector(":scope>small"));
    button.append(el("span","gx-mark"),body,el("span","gx-letter",letter));list.append(button);
    var c={button:button,value:value,label:label,letter:letter,on:function(){return button.getAttribute("aria-checked")==="true";},set:function(v){button.setAttribute("aria-checked",v?"true":"false");}};
    /* A pick toggles; one of N replaces the pick. */
    button.addEventListener("click",function(){var on=!c.on();if(on)choices.forEach(function(o){if(!multiple||o.exclusive||c.exclusive)o.set(false);});c.set(on);sync();notify();if(on&&c===other)note.focus();});
    choices.push(c);return c;
  }
  choiceEls.forEach(function(ce){var label=labelText(ce);choice(kids(ce),(ce.getAttribute("value")||"").trim()||label,ce.hasAttribute("recommended"),label);});
  var other=choice([document.createTextNode(OTHER)],OTHER,false,OTHER);other.button.classList.add("gx-other");
  has=function(){return choices.some(function(c){return c.on();})||!!note.value.trim();};
  sync();qe.append(list,note);
  return{el:qe,title:title,key:key,tag:(tag||"").trim(),choices:choices,collect:function(out){var on=choices.filter(function(c){return c.on();}).map(function(c){return c.value;});
      var written=note.value.trim();out[key]=on.length?(multiple?on:on[0]):written?null:SKIPPED;if(written)out[key+" (note)"]=written;},
    restore:function(answers){var v=answers[key],picked=Array.isArray(v)?v:v==null?[]:[v];
      choices.forEach(function(c){c.set(picked.indexOf(c.value)>=0);});
      if(typeof answers[key+" (note)"]==="string")note.value=answers[key+" (note)"];sync();live();},
    lock:function(){answered(choices.some(function(c){return c.on();})||!!note.value.trim());choices.forEach(function(c){c.button.disabled=true;});note.readOnly=true;if(!note.value)note.hidden=true;}};
}
/* The Pick button of one variant: at the right end of its heading's row (or a small head the kit adds when the variant has no heading), it clicks that choice, and shows it picked, here and in the question. `node` carries the picked ring. */
function attachPick(node,heading,c){
  var pick=el("button","gx-pick");pick.type="button";
  if(heading){heading.classList.add("gx-has-pick");heading.append(pick);}
  else{var head=el("div","gx-option-head");head.append(el("span","gx-option-label",c.label),pick);node.insertBefore(head,node.firstChild);}
  pick.addEventListener("click",function(){c.button.click();});
  return{
    sync:function(){var on=c.on();node.classList.toggle("gx-picked",on);pick.setAttribute("aria-pressed",on?"true":"false");pick.textContent=on?"\u2713 Picked":"Pick";pick.setAttribute("data-gx-tip",on?"Click again to clear":"Pick \u201c"+c.label+"\u201d");pick.disabled=c.button.disabled;}};
}
/* A mockup tied to a choice (`data-question` + `data-choice`): a Pick button by its title that
   clicks that choice. It has no note box (User 2026-10-30). */
function bindOptions(questions){
  var found=[];
  Array.prototype.forEach.call(document.querySelectorAll("[data-question][data-choice]"),function(node){
    if(node.closest("gx-question"))return;
    var asked=(node.getAttribute("data-question")||"").trim(),wanted=(node.getAttribute("data-choice")||"").trim();
    var q=questions.filter(function(x){return x.key===asked||(x.tag&&x.tag===asked);})[0];
    var c=q&&q.choices.filter(function(x){return x.value===wanted||x.label===wanted||x.letter===wanted.toLowerCase();})[0];
    if(!c){if(window.console)console.warn("ghostex: no choice \""+wanted+"\" in question \""+asked+"\"");return;}
    var heading=/^H[1-6]$/.test(node.tagName)?node:node.querySelector("h1,h2,h3,h4,h5,h6");
    var option=attachPick(node,heading,c);option.q=q;found.push(option);
  });
  return found;
}
/* CDXC:SessionChat 2026-10-30 DECISION:
   User, on a question page whose variants were not tied to its choices: "PLEASE for inline htmls we must show the pick button on the top right so i can pick one easily instead of having to pick below". An agent can forget `data-question` / `data-choice`, so the kit ties the variants itself when a question has none: a heading above the question (after the previous question, so only the mockups it asks about) that starts with a choice's letter ("A. Same layout") or reads as its label gets that choice's Pick button, exactly like a tied variant. A heading that already has a Pick, matches nothing or matches two choices (or a choice another heading also matches) stays untouched: a wrong Pick is worse than none. A question under mockups that still gets no Pick is listed in `window.__ghostexKit.untied`, which `ghostex show` and `ghostex visual check` (server/src/ghostex_cli/visual_page_check.rs) tell the author about. */
var LETTERED=/^(?:(?:option|variant|choice|design|idea|proposal)\s+)?([A-Za-z])(?:[.):]|\s+[\u2013\u2014-])(?:\s+|$)(.*)$/i;
var MOCKUP="gx-wide,.gx-frame,.gx-figure,figure,img,svg,iframe,canvas,video,[class*=mock i],[class*=variant i],[class*=screen i],[class*=device i]";
function norm(text){return(text||"").toLowerCase().replace(/[^\p{L}\p{N}]+/gu," ").trim();}
function byLetterAny(m,real){return !!m&&real.some(function(c){return c.letter===m[1].toLowerCase();});}
function autoTie(questions,explicit){
  var tied=explicit.map(function(o){return o.q;}),options=[],report={auto:[],untied:[]},prev=null;
  var heads=Array.prototype.filter.call(document.querySelectorAll("h2,h3,h4,h5,h6"),function(h){
    return !h.closest("gx-question,gx-page>header,gx-questions>header,.gx-theme-switch,[data-question][data-choice]")&&!h.classList.contains("gx-has-pick")&&!h.querySelector(".gx-pick");
  });
  questions.forEach(function(q){
    var qe=q.el,before=prev;prev=qe;
    var real=q.choices.filter(function(c){return !c.button.classList.contains("gx-other");});
    if(!real.length||tied.indexOf(q)>=0)return;
    function above(n){return(!before||(before.compareDocumentPosition(n)&Node.DOCUMENT_POSITION_FOLLOWING))&&(qe.compareDocumentPosition(n)&Node.DOCUMENT_POSITION_PRECEDING)&&!n.contains(qe);}
    var hits=[],lettered=false;
    heads.filter(above).forEach(function(h){
      var text=(h.textContent||"").replace(/\s+/g," ").trim(),m=text.match(LETTERED),rest=norm(m?m[2]:text),letter=m?m[1].toLowerCase():"";
      if(byLetterAny(m,real))lettered=true;
      var byLabel=rest?real.filter(function(c){return norm(c.label)===rest||norm(c.value)===rest;}):[];
      var byLetter=letter?real.filter(function(c){return c.letter===letter;}):[];
      if(byLabel.length>1||(byLabel.length===1&&byLetter.length&&byLetter[0]!==byLabel[0]))return;
      var c=byLabel[0]||byLetter[0];
      if(c)hits.push({h:h,c:c,text:text});
    });
    var good=hits.filter(function(x){return hits.filter(function(y){return y.c===x.c;}).length===1;});
    good.forEach(function(x){options.push(attachPick(x.h,x.h,x.c));report.auto.push({question:q.key,heading:x.text,choice:x.c.letter});});
    if(good.length)return;
    var under=lettered||Array.prototype.some.call(document.querySelectorAll(MOCKUP),function(n){return !n.closest("gx-question,gx-questions>.gx-send,.gx-seg,.gx-theme-switch,#gx-tip")&&above(n);});
    if(under)report.untied.push({question:q.key,tag:q.tag,title:q.title});
  });
  return{options:options,report:report};
}
function setup(root){
  var used={},questions=[],options=[];
  function collect(){var out={};questions.forEach(function(q){q.collect(out);});var t=notes&&notes.value.trim();if(t)out[notesKey]=t;return out;}
  function restore(answers){questions.forEach(function(q){q.restore(answers);});if(notes)notes.value=typeof answers[notesKey]==="string"?answers[notesKey]:"";}
  function syncOptions(){options.forEach(function(o){o.sync();});}
  /* The reader's picks so far are kept on the host (PAGE_API's draft), so closing and reopening the page loses nothing. */
  function changed(){syncOptions();if(window.ghostex&&ghostex.__saveDraft)ghostex.__saveDraft("kit",collect());}
  questions=Array.prototype.map.call(root.querySelectorAll("gx-question"),function(qe,index){return question(qe,index,used,changed);});
  var explicit=bindOptions(questions),auto=autoTie(questions,explicit);
  options=explicit.concat(auto.options);window.__ghostexKit={tied:explicit.length,auto:auto.report.auto,untied:auto.report.untied};syncOptions();
  /* "Anything else?": see the SessionChat 2026-11-04 DECISION on KIT_SCRIPT. */
  var last=questions[questions.length-1],notes=null,notesCard=null,notesKey=NOTES;
  for(var copy=2;questions.some(function(q){return q.key===notesKey||q.key+" (note)"===notesKey;});copy++)notesKey=NOTES+" ("+copy+")";
  if(!(last&&!last.choices.length&&OWN_NOTES.test(last.title))){
    notesCard=el("div","gx-notes");var notesHead=el("div","gx-q-head"),notesTitle=el("div","gx-q-title","Anything else?");notesTitle.id="gx-notes-title";
    notes=el("textarea","gx-text");notes.setAttribute("aria-labelledby","gx-notes-title");notes.placeholder="Write anything more, or paste images (optional)";notes.__gxThumbs=true;
    notes.addEventListener("input",changed);notesHead.append(notesTitle);notesCard.append(notesHead,notes);
  }
  var bar=el("div","gx-send"),button=el("button","gx-send-button"),label=el("span",null,root.getAttribute("submit")||"Send answers"),status=el("span","gx-send-status"),info=el("button","gx-send-info");
  /* The sentence about blank questions is an (i) icon with the quick tooltip, not a line of text beside the buttons (the SessionChat 2026-10-24 DECISION in apps/desktop/src/app/native_chat/ask_card.rs `ask_info_hint`). Any status message replaces the icon. */
  info.type="button";info.setAttribute("aria-label","Questions you leave blank are sent as skipped.");info.setAttribute("data-gx-tip","Questions you leave blank are sent as skipped.");info.innerHTML='<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 12a9 9 0 1 0 18 0a9 9 0 0 0 -18 0"/><path d="M12 9h.01"/><path d="M11 12h1v4h1"/></svg>';status.append(info);
  var mac=/Mac|iPhone|iPad/.test(navigator.platform||"");
  /* The shortcuts show in the kit's tooltip as key caps, the keys this platform uses, not on the buttons. */
  button.setAttribute("data-gx-tip","Send answers");button.setAttribute("data-gx-tip-keys",mac?"⌘ Enter":"Ctrl Enter");
  /* Skip all: sends at once with every question as skipped and no notes, whatever is picked or typed (see the SessionChat DECISION on SKIPPED in packages/gx-visual/src/questions.rs). */
  var skipAll=el("button","gx-skip-all");skipAll.type="button";skipAll.append(el("span",null,"Skip all"));skipAll.setAttribute("data-gx-tip","Skip all");skipAll.setAttribute("data-gx-tip-keys",mac?"⌘ ⇧ Enter":"Ctrl Shift Enter");
  button.type="button";button.append(label);bar.append(status,skipAll,button);if(notesCard)root.append(notesCard);root.append(bar);
  skipAll.addEventListener("click",function(){
    if(button.disabled)return;
    var out={};questions.forEach(function(q){out[q.key]=SKIPPED;});
    /* Once sent, the page shows what went: no picks and no notes. */
    send(out,function(){restore({});root.querySelectorAll("textarea").forEach(function(t){t.value="";});});
  });
  function lock(){questions.forEach(function(q){q.lock();});if(notes){notes.readOnly=true;notesCard.hidden=!notes.value.trim();}syncOptions();allPills();button.disabled=true;skipAll.remove();label.textContent="Sent";button.removeAttribute("data-gx-tip");button.removeAttribute("data-gx-tip-keys");status.classList.remove("gx-error");status.textContent="Sent to the agent.";}
  if(!window.ghostex){button.disabled=true;skipAll.remove();status.textContent="This Ghostex can't send answers from a page; type them in the chat.";return;}
  if(ghostex.answered){restore(ghostex.answers||{});lock();return;}
  var saved=ghostex.__draft&&ghostex.__draft("kit");if(saved&&typeof saved==="object"){restore(saved);syncOptions();}
  /* Pasted images: see the Clipboard DECISION on KIT_SCRIPT. */
  var pendingImages=0,base=location.pathname.replace(/\/+$/,"");
  var thumbs={},kept={},keptSize=0,savedThumbs=ghostex.__draft&&ghostex.__draft("thumbs");
  if(savedThumbs&&typeof savedThumbs==="object")Object.keys(savedThumbs).forEach(function(k){var d=savedThumbs[k];if(typeof d==="string"&&/^data:image\//.test(d)){thumbs[k]=kept[k]=d;keptSize+=d.length;}});
  /* A small JPEG of the pasted image for the draft, so a reopened page still shows its thumbnail. */
  function small(file){return new Promise(function(resolve){var url=URL.createObjectURL(file),img=new Image();
    img.onload=function(){var k=Math.min(1,112/Math.max(img.naturalWidth,img.naturalHeight,1)),c=document.createElement("canvas");c.width=Math.max(1,Math.round(img.naturalWidth*k));c.height=Math.max(1,Math.round(img.naturalHeight*k));var g=c.getContext("2d");g.fillStyle="rgb(128,128,128)";g.fillRect(0,0,c.width,c.height);g.drawImage(img,0,0,c.width,c.height);URL.revokeObjectURL(url);try{resolve(c.toDataURL("image/jpeg",0.7));}catch(x){resolve(null);}};
    img.onerror=function(){URL.revokeObjectURL(url);resolve(null);};img.src=url;});}
  function keepThumb(path,file){thumbs[path]=URL.createObjectURL(file);small(file).then(function(d){if(!d||keptSize+d.length>24000||button.disabled)return;kept[path]=d;keptSize+=d.length;if(ghostex.__saveDraft)ghostex.__saveDraft("thumbs",kept);});}
  function boxes(){return Array.prototype.slice.call(root.querySelectorAll("textarea.gx-note,textarea.gx-text"));}
  function nextImage(){var top=0;boxes().forEach(function(t){t.value.replace(/\[Image #(\d+)\]\(/g,function(_,n){top=Math.max(top,+n);});});return top+1;}
  function edited(area){area.dispatchEvent(new Event("input",{bubbles:true}));}
  function pills(area){
    var row=area.__gxPills;if(!row){row=el("div",area.__gxThumbs?"gx-pills gx-thumbs":"gx-pills");area.after(row);area.__gxPills=row;}
    row.textContent="";var re=/\[(Image #\d+)\]\(((?:\\.|[^)\\\r\n])*)\)/g,m;
    while((m=re.exec(area.value))){
      var src=area.__gxThumbs&&thumbs[m[2]],pill,start=m.index,end=re.lastIndex;
      if(src){pill=el("span","gx-thumb");var img=el("img");img.src=src;img.alt=m[1];pill.append(img);pill.setAttribute("data-gx-tip",m[1]+": "+m[2]);}
      else{pill=el("span","gx-pill");pill.innerHTML=IMAGE_ICON;pill.append(el("span",null,m[1]));pill.setAttribute("data-gx-tip",m[2]);}
      if(!area.readOnly){var x=el("button","gx-pill-x","\u00d7");x.type="button";x.setAttribute("aria-label","Remove "+m[1]);x.addEventListener("click",(function(s,e){return function(){var v=area.value;if(s>0&&v.charAt(s-1)===" ")s--;else if(v.charAt(e)===" ")e++;area.value=v.slice(0,s)+v.slice(e);edited(area);};})(start,end));pill.append(x);}
      row.append(pill);
    }
    row.hidden=!row.childNodes.length||area.hidden;
  }
  function allPills(){boxes().forEach(pills);}
  function hint(area,text,error){var h=area.__gxHint;if(!h){h=el("div","gx-paste");(area.__gxPills||area).after(h);area.__gxHint=h;}h.textContent=text||"";h.classList.toggle("gx-error",!!error);h.hidden=!text;}
  function upload(file){return fetch(base+"/image",{method:"POST",body:new Blob([file])}).then(function(r){return r.json().catch(function(){return{};}).then(function(j){if(r.ok&&j.ok&&j.path)return j.path;throw new Error(j.error||"The pasted image could not be saved.");});});}
  root.addEventListener("input",function(e){if(e.target&&e.target.matches&&e.target.matches("textarea.gx-note,textarea.gx-text"))pills(e.target);},true);
  root.addEventListener("paste",function(e){
    var area=e.target;if(!area||!area.matches||!area.matches("textarea.gx-note,textarea.gx-text")||area.readOnly||button.disabled)return;
    var files=[];Array.prototype.forEach.call(e.clipboardData&&e.clipboardData.items||[],function(i){if(i.kind==="file"&&/^image\//.test(i.type)){var f=i.getAsFile();if(f)files.push(f);}});
    if(!files.length)return;
    e.preventDefault();
    var original=area.value,start=area.selectionStart,end=area.selectionEnd,paths=[];pendingImages++;hint(area,files.length>1?"Saving the pasted images…":"Saving the pasted image…");
    files.reduce(function(chain,f){return chain.then(function(){return upload(f).then(function(p){paths.push(p);if(area.__gxThumbs)keepThumb(p,f);});});},Promise.resolve()).then(function(){hint(area,"");},function(error){hint(area,error&&error.message||"The pasted image could not be saved.",true);}).then(function(){
      pendingImages--;if(!paths.length)return;
      /* Typing went on while it saved: the selection taken at the paste no longer fits, so the references go at the end. */
      var v=area.value,s=v===original?start:v.length,en=v===original?end:s;
      paths.forEach(function(p){var ref="[Image #"+nextImage()+"]("+p+")",lead=s>0&&!/\s/.test(v.charAt(s-1))?" ":"",ins=lead+ref+" ";v=v.slice(0,s)+ins+v.slice(en);s=en=s+ins.length;area.value=v;});
      if(document.activeElement===area)area.setSelectionRange(s,s);edited(area);
    });
  },true);
  allPills();
  function send(out,sent){
    if(pendingImages){status.classList.add("gx-error");status.textContent="Wait for the pasted image to finish saving.";return;}
    button.disabled=true;status.classList.remove("gx-error");status.textContent="Sending…";
    /* A send that can't reach Ghostex keeps trying with everything left as typed, and says so (PAGE_API's submit). */
    ghostex.submit(out,function(message){status.classList.add("gx-error");status.textContent=message;}).then(function(){if(sent)sent();lock();},function(error){button.disabled=false;status.classList.add("gx-error");status.textContent=error&&error.message||"Your answers were not sent.";});
  }
  button.addEventListener("click",function(){send(collect());});
  /* Ctrl+Enter (Cmd+Enter on macOS) presses the send button from anywhere on the page, a note box included; with Shift it presses Skip all. */
  document.addEventListener("keydown",function(e){if(e.key==="Enter"&&(e.ctrlKey||e.metaKey)&&!e.isComposing&&!button.disabled){e.preventDefault();(e.shiftKey?skipAll:button).click();}});
}
/* The page's theme: the attribute PAGE_BOOTSTRAP set (Ghostex's theme, else the system's). A page that paints its own background in the other theme decides instead ("follow paint"), so the kit's cards always match what the page shows; the reader's Dark / Light switch, which a v2 page gets, wins over both. */
var html=document.documentElement,picked=false,themeSwitch=null;
function lum(color){var m=(color||"").match(/[\d.]+/g);if(!m||m.length<3||(m.length>3&&+m[3]<0.5))return null;return(0.2126*m[0]+0.7152*m[1]+0.0722*m[2])/255;}
function followPaint(){
  if(picked||!document.body)return;
  var l=lum(getComputedStyle(document.body).backgroundColor);if(l==null)l=lum(getComputedStyle(html).backgroundColor);if(l==null)return;
  var want=l>0.5?"light":"dark";
  if(html.getAttribute("data-gx-theme")!==want){html.setAttribute("data-gx-theme",want);html.setAttribute("data-gx-theme-from","page");}
  /* A page that paints its own theme can't be flipped, so it gets no switch. */
  if(html.getAttribute("data-gx-theme-from")==="page"&&themeSwitch){themeSwitch.remove();themeSwitch=null;}
  markSwitch();
}
function markSwitch(){if(!themeSwitch)return;var t=html.getAttribute("data-gx-theme");Array.prototype.forEach.call(themeSwitch.children,function(b){b.setAttribute("aria-pressed",b.getAttribute("data-gx-set-theme")===t?"true":"false");});}
function isV2(){var m=document.querySelector('meta[name="ghostex-page" i]');return!!m&&(m.getAttribute("content")||"").trim().toLowerCase()==="v2";}
function addThemeSwitch(){
  if(!isV2()||html.getAttribute("data-gx-theme-from")==="page"||document.querySelector(".gx-theme-switch"))return;
  var sw=el("div","gx-seg gx-theme-switch");sw.setAttribute("role","group");sw.setAttribute("aria-label","Theme");
  ["Dark","Light"].forEach(function(name){var b=el("button",null,name);b.type="button";b.setAttribute("data-gx-set-theme",name.toLowerCase());b.addEventListener("click",function(){picked=true;html.setAttribute("data-gx-theme",name.toLowerCase());html.setAttribute("data-gx-theme-from","reader");markSwitch();});sw.append(b);});
  /* Beside the page's title: in the container's header, else at the top of the container, else the page's top right corner. */
  var slot=document.querySelector("[data-gx-theme-switch]"),container=document.querySelector("gx-page,gx-questions"),header=container&&Array.prototype.filter.call(container.children,function(c){return c.tagName==="HEADER";})[0];
  if(slot)slot.append(sw);else if(header)header.append(sw);else if(container)container.insertBefore(sw,container.firstChild);else document.body.append(sw);
  themeSwitch=sw;markSwitch();
}
/* <gx-tabs> with one child per tab (`data-tab="Label"`): the kit adds the tab bar and shows the first, or the one marked `data-tab-default`. */
function setupTabs(){
  Array.prototype.forEach.call(document.querySelectorAll("gx-tabs"),function(tabs){
    var panes=Array.prototype.filter.call(tabs.children,function(c){return c.hasAttribute("data-tab");});if(!panes.length)return;
    var bar=el("div","gx-seg");bar.setAttribute("role","tablist");
    function show(i){panes.forEach(function(p,j){p.hidden=j!==i;bar.children[j].setAttribute("aria-selected",j===i?"true":"false");});}
    panes.forEach(function(p,i){var b=el("button",null,p.getAttribute("data-tab"));b.type="button";b.setAttribute("role","tab");b.addEventListener("click",function(){show(i);});bar.append(b);});
    tabs.insertBefore(bar,tabs.firstChild);
    var start=0;panes.forEach(function(p,i){if(p.hasAttribute("data-tab-default"))start=i;});show(start);
  });
}
function init(){followPaint();addThemeSwitch();setupTabs();var root=document.querySelector("gx-questions");if(root)setup(root);}
if(document.readyState==="loading")document.addEventListener("DOMContentLoaded",init);else init();
addEventListener("load",followPaint);
})();</script>"#;
