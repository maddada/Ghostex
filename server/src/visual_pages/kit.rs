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
//! A list of items asked about carries its picks on its rows (`bindRows`): a `<gx-pick value>` (its
//! Markdown already rendered by prepare.rs) or any element with `data-pick`, tied to the question
//! named by its own `question` / `data-question` or the nearest `data-question` around it. Each row
//! becomes a choice of its question when the question doesn't list it, gets a checkbox (a radio for
//! pick one), a Note and, when long, a fold; `[data-pick-group]` groups rows under a checkbox. A
//! question whose every choice has its row or picture shows the picks as chips instead of its list,
//! and a question that repeats items listed above it is reported in `window.__ghostexKit.repeated`.
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
//! User, about the yellow "?" tile at the left of every question heading: "remove this yellow thing here please so i can tell what i answered and what i didn't answer". A question shows the green check as soon as it has an answer (a pick made, text typed, or a restored draft). After the page is sent, an answered question keeps the green check and one left blank keeps the plain tile. User (2026-10-19): "can we make this match the look of other icons we just worked on please?", so that tile is the chat's shared one (the SessionChat 2026-10-18 DECISION on `SINGLE_LINE_CARD` in apps/desktop/src/app/native_chat/cards.rs: 28px neutral tile, question-bubble glyph, a 12px green check badge), superseding the tinted green and muted squares. Then, of the empty slot an unanswered question kept: "fix this not showing an icon here, should show one with this color question mark #D2A024", so a question still waiting shows the chat's waiting tile (the checklist glyph with a #D2A024 "?" badge, Q305) until it has an answer, superseding the empty slot from "remove this yellow thing here please"; the green check still tells an answered question from a waiting one. User (2026-11-11): "let's change these to the same #F472B6 color that we use to represent questions everywhere", then "All four": that "?" badge is the questions pink #F472B6 in both themes (the ink stays #161616), superseding #D2A024, as in the chat (`ask_color` in apps/desktop/src/app/native_chat/ask_card.rs).
//! User picked from the looks page: a question's tag is a filled chip ("make it show like eyebrow"), and a choice keeps its radio or checkbox with the letter as a muted keycap on the right; the in-chat form (apps/desktop/src/app/native_chat/question_form.rs, the phone's QuestionForm.tsx) matches. User (2026-10-19): "please move the Q294 to be right aligned same row as the question", so the chip sits at the right end of the title's first line and a long title wraps beside it, superseding the chip above the title.
//! User: Ctrl+Enter (Cmd+Enter on macOS) presses the page's send button from anywhere on it, a note box included; the in-chat form does the same.
//! User: a reader picks an option right at its mockup (a Pick button by the mockup's title that picks that choice in its question, shown picked on both and cleared from either), and (first version) wrote a note under each mockup, keyed to that question and choice.
//! User 2026-10-30: no note box under each option in pages, for every page and for customers. The kit no longer makes the box (its collect, restore, lock and CSS are gone); the Pick button, each question's own "Note (optional)" box and the form's closing notes box stay. Answers to pages sent earlier with a `"<question> · <choice> (note)"` key still show in the answers card (packages/gx-chat-core/src/transcript/page_answers.rs reads them), but the kit sends no new ones. Superseded for rows on 2026-11-20 (Q4235): every row of a list has a Note again (`bindRows`, KIT_SCRIPT); pictures still have none.
//! User: "question cards in the html having different width is an absolute NO GO … all take up the width as they should": every question card fills the column, even when a page's own CSS centres it with auto margins (which shrink a grid item to its content).
//! User: the picked mockup's outline must never be cut off and needs more padding so it is not tight on the design: a tagged option gets 14px of room and a 2px ring inside its own box (content-box, so a fixed-width mockup keeps its width), because an outline drawn outside the box is clipped by a scrolling wrapper such as an `overflow-x:auto` frame.
//! User: every question page offers "Other / none of these" with a note box, consistently, instead of each agent hand-rolling it, and the reader may leave a question unanswered. No choice is ever preselected; an agent marks the one it recommends with a Recommended mark and the reader picks. Mockups can use the full width of the page between or above the questions, and the Answered note is as wide as the question cards.
//!
//! CDXC:SessionChat 2026-11-02 DECISION:
//! User: "make the 'recommended' tag more subtle, not like a pill, and not all caps." The recommended choice in a question and the recommended variant's `.gx-ribbon` both show the same quiet mark: a small muted "Recommended" in sentence case after a faint star in the pick colour (`--gx-pick`, neutral ink since 2026-11-04, theme.rs), with no pill, fill or border. The chat's own question card (apps/desktop/src/app/native_chat/ask_card.rs `recommended_mark`, the phone's askCard.tsx `RecommendedMark`) shows the same mark. The ribbon sits centred just above its card instead of on the card's top edge, so no card's title or layout moves for it (beside the title it pushed long titles onto two lines).
//!
//! CDXC:SessionChat 2026-10-24 DECISION:
//! User, after an agent's page showed its variants A, B, C but listed the question's choices C, B, A (recommended first): "some agent gave me the options flipped in the selection area below!! ... the order of the options above, has to match their order below!" The kit enforces it instead of trusting every agent: before lettering, a question's choices are sorted into the order their `data-question` / `data-choice` mockups appear in the document (matched by the choice's value or label; a bare letter cannot rank because letters come from the order). Choices no mockup names keep their relative order after the matched ones, "Other / none of these" stays last, and a question with no mockups is left as written. server/src/ghostex_cli/visual_guide.md teaches the same rule.
//!
//! CDXC:SessionChat 2026-10-11 SEE-ALSO:
//! The answers card folds the `"<key> (note)"` entries (and the `"<key> · <choice> (note)"` ones older pages sent) into their question's row (packages/gx-chat-core/src/transcript/page_answers.rs); server/src/ghostex_cli/visual_guide.md teaches agents this markup. Change them together.

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
gx-page,gx-questions{display:grid;grid-template-columns:minmax(var(--gx-gutter),1fr) minmax(0,var(--gx-text-width)) minmax(var(--gx-gutter),1fr);row-gap:20px;align-content:start;padding:56px 0 88px;font-size:15px;line-height:1.65;color:var(--gx-text);overflow-wrap:break-word}
gx-page{display:grid}
gx-page>*,gx-questions>*{grid-column:2;min-width:0}
/* The Mark as read footer (SEEN_FOOTER) inside a page's own layout takes the page's width, not gutters of its own. */
gx-page gx-questions[data-gx-seen]{grid-template-columns:0 minmax(0,1fr) 0;padding:36px 0 0}
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
.gx-stats{display:grid;gap:6px;grid-template-columns:repeat(auto-fit,minmax(min(100%,var(--gx-stat-min,96px)),1fr))}
.gx-stat{min-width:0;padding:7px 10px 6px;border-radius:10px;border:1px solid var(--gx-line);background:var(--gx-surface)}
.gx-stat small{display:block;font-size:10px;font-weight:650;letter-spacing:.1em;text-transform:uppercase;color:var(--gx-muted)}
.gx-stat b{display:block;font-size:15px;line-height:1.35;text-wrap:balance;font-variant-numeric:tabular-nums}
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
.gx-q-badge.gx-ask{background:#F472B6;color:#161616}
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
textarea.gx-src{display:none!important}
.gx-box{position:relative;overflow-wrap:anywhere;white-space:pre-wrap;cursor:text;-webkit-user-modify:read-write-plaintext-only}
.gx-box[contenteditable=false]{-webkit-user-modify:read-only;cursor:default}
.gx-box.gx-empty::before{content:attr(data-placeholder);position:absolute;left:10px;top:6px;right:10px;color:var(--gx-muted);pointer-events:none;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.gx-ref{display:inline;padding:0 0 0 1px;border-radius:4px;font-weight:500;color:#8cb59e;white-space:nowrap;cursor:pointer;-webkit-user-modify:read-only}
html[data-gx-theme=light] .gx-ref{color:#5f9878}
.gx-ref svg{display:inline-block;width:1em;height:1em;margin:0 3px 0 0;vertical-align:-.15em}
.gx-ref:hover,.gx-ref.gx-on{background:color-mix(in srgb,currentColor 14%,transparent)}
#gx-peek{position:fixed;left:0;top:0;z-index:2147483646;margin:0;inset:auto;display:flex;flex-direction:column;gap:6px;box-sizing:border-box;width:max-content;max-width:min(340px,calc(100vw - 16px));padding:6px;border-radius:12px;border:1px solid var(--gx-tip-border);background:var(--gx-tip-bg);color:var(--gx-tip-fg);-webkit-backdrop-filter:blur(20px) saturate(1.15);backdrop-filter:blur(20px) saturate(1.15);box-shadow:var(--gx-tip-shadow);font:500 12.5px/1.4 Inter,var(--font-sans);opacity:0;visibility:hidden;transition:opacity .12s ease,visibility 0s linear .12s}
#gx-peek.gx-peek-on{opacity:1;visibility:visible;transition:opacity .12s ease}
#gx-peek img{display:block;max-width:100%;max-height:220px;min-width:48px;min-height:32px;border-radius:8px;object-fit:contain;cursor:zoom-in}
#gx-peek .gx-peek-wait{display:flex;align-items:center;justify-content:center;width:200px;height:120px;border-radius:8px;color:color-mix(in srgb,var(--gx-tip-fg) 60%,transparent);background:color-mix(in srgb,var(--gx-tip-fg) 6%,transparent)}
#gx-peek .gx-peek-row{display:flex;align-items:center;gap:8px;min-width:0;padding:0 2px 0 4px}
#gx-peek .gx-peek-label{flex:none}
#gx-peek .gx-peek-path{flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;direction:rtl;text-align:left;color:color-mix(in srgb,var(--gx-tip-fg) 58%,transparent);font-weight:400}
#gx-peek .gx-pill-x{flex:none;width:auto;height:22px;padding:0 7px;gap:3px;border-radius:6px;color:color-mix(in srgb,var(--gx-tip-fg) 78%,transparent);font:500 12px/1 Inter,var(--font-sans)}
#gx-peek .gx-pill-x:hover{background:color-mix(in srgb,var(--gx-tip-fg) 12%,transparent);color:var(--gx-tip-fg)}
.gx-thumb.gx-on{border-color:var(--gx-fg);box-shadow:0 0 0 1px var(--gx-fg)}
.gx-pills{display:flex;flex-wrap:wrap;gap:4px 10px;margin:6px 0 0}
.gx-pills[hidden]{display:none}
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
.gx-tally{position:fixed;right:max(var(--gx-gutter,16px),env(safe-area-inset-right,0px));bottom:max(var(--gx-gutter,16px),env(safe-area-inset-bottom,0px));left:auto;max-width:calc(100% - 2*max(var(--gx-gutter,16px),env(safe-area-inset-right,0px)));overflow:hidden;text-overflow:ellipsis;z-index:20;box-sizing:border-box;padding:3px 10px;border-radius:999px;border:1px solid var(--gx-card-border);background:var(--gx-card);color:var(--gx-muted);font:500 12px/18px var(--font-sans);font-variant-numeric:tabular-nums;white-space:nowrap;box-shadow:var(--gx-glass-shadow);-webkit-backdrop-filter:blur(22px) saturate(1.15);backdrop-filter:blur(22px) saturate(1.15);pointer-events:none}
.gx-tally.gx-all{color:var(--success)}
.gx-tally-room{display:block;height:calc(var(--gx-gutter,16px) + 34px)}
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
/* Dictate: a microphone in each note and text box where the page's host can dictate (CDXC:Dictate 2026-11-16 in apps/desktop/src/app/window/visual_page_modal/page_dictation.rs). */
.gx-field{position:relative;margin:8px 0 0}
.gx-field>.gx-note,.gx-field>.gx-text{margin-top:0}
.gx-field>.gx-box{padding-right:38px}
.gx-field>.gx-box.gx-empty::before{right:38px}
.gx-mic{position:absolute;top:5px;right:6px;display:inline-flex;align-items:center;justify-content:center;width:24px;height:24px;padding:0;border:0;border-radius:6px;background:none;color:var(--gx-muted);cursor:pointer}
.gx-mic svg{width:16px;height:16px;pointer-events:none}
.gx-mic:hover:not(:disabled){background:color-mix(in srgb,var(--gx-fg) 10%,transparent);color:var(--gx-fg)}
.gx-mic:focus-visible{outline:none;box-shadow:0 0 0 2px color-mix(in srgb,var(--ring) 45%,transparent)}
.gx-mic:disabled{opacity:.4;cursor:default}
.gx-mic[hidden]{display:none}
.gx-mic.gx-mic-on,.gx-mic.gx-mic-on:hover:not(:disabled){color:var(--destructive);background:color-mix(in srgb,var(--destructive) 14%,transparent)}
.gx-mic.gx-mic-busy svg{animation:gx-mic-spin 1s linear infinite}
.gx-dict-restore{margin:0 0 0 6px;padding:0 6px;border:0;border-radius:5px;background:color-mix(in srgb,var(--gx-fg) 8%,transparent);color:var(--gx-fg);font:inherit;font-weight:600;cursor:pointer}
.gx-dict-restore:hover{background:color-mix(in srgb,var(--gx-fg) 14%,transparent)}
@keyframes gx-mic-spin{to{transform:rotate(360deg)}}
/* Picks on rows (CDXC:SessionChat 2026-11-20 DECISION on KIT_SCRIPT `bindRows`): a checkbox (a radio for pick one) leading every row, the row's own content beside it, a Note on every row, long rows folded. */
gx-pick{display:block}
.gx-row:not(tr){display:grid;grid-template-columns:auto minmax(0,1fr);column-gap:12px;align-items:start;padding:12px 10px;border-radius:12px;border:1px solid transparent;list-style:none}
.gx-row{cursor:pointer;outline:none;transition:background-color .15s,border-color .15s}
.gx-row:not(tr)+.gx-row:not(tr){margin-top:2px}
.gx-row:not(tr):hover:not(.gx-row-off){background:color-mix(in srgb,var(--gx-fg) 3%,transparent)}
.gx-row:not(tr)[aria-checked=true]{background:color-mix(in srgb,var(--gx-pick) 7%,transparent);border-color:color-mix(in srgb,var(--gx-pick) 35%,transparent)}
tr.gx-row[aria-checked=true]>td{background:color-mix(in srgb,var(--gx-pick) 7%,transparent)}
.gx-row:focus-visible,.gx-row-group:focus-visible{box-shadow:0 0 0 2px color-mix(in srgb,var(--ring) 55%,transparent)}
.gx-row.gx-row-off{cursor:default}
.gx-row-cell{width:18px;padding-right:0}
.gx-row-ctl{display:grid;place-items:center;width:18px;height:18px;margin-top:2px;box-sizing:border-box;border-radius:5px;border:1.5px solid color-mix(in srgb,var(--gx-muted) 75%,transparent);color:var(--gx-bg);font:700 12px/1 var(--font-sans)}
.gx-row[role=radio] .gx-row-ctl{border-radius:50%}
.gx-row[aria-checked=true] .gx-row-ctl,.gx-row-ctl.gx-all{background:var(--gx-pick);border-color:var(--gx-pick)}
.gx-row[role=checkbox][aria-checked=true] .gx-row-ctl::before,.gx-row-ctl.gx-all::before{content:"\2713"}
.gx-row[role=radio][aria-checked=true] .gx-row-ctl{background:radial-gradient(circle,var(--gx-pick) 0 4px,transparent 4.5px)}
.gx-row-ctl.gx-some{border-color:var(--gx-pick)}
.gx-row-ctl.gx-some::before{content:"";width:8px;height:2px;border-radius:2px;background:var(--gx-pick)}
.gx-row-main{min-width:0;color:var(--gx-text);font-size:14px;line-height:1.55}
.gx-row-main>*{margin-top:0;margin-bottom:0}
.gx-row-main>*+*{margin-top:8px}
.gx-row-title{display:flex;flex-wrap:wrap;align-items:flex-start;gap:4px 10px;margin:0;color:var(--gx-fg);font-size:15px;line-height:22px;font-weight:600;letter-spacing:0;text-transform:none}
.gx-row-label{flex:1 1 200px;min-width:0}
.gx-row-label code{font-size:12px;margin-right:2px;padding:0;background:none;color:var(--gx-muted)}
.gx-row-side{display:flex;flex:none;align-items:center;gap:8px;margin-left:auto;font-weight:400}
.gx-options>.gx-row .gx-row-label{flex-basis:140px}
.gx-row-note-btn{flex:none;height:24px;padding:0 8px;border-radius:7px;border:1px solid transparent;background:none;color:var(--gx-muted);font:400 12px/22px var(--font-sans);cursor:pointer;white-space:nowrap}
.gx-row:hover .gx-row-note-btn,.gx-row:focus-within .gx-row-note-btn,.gx-row-note-btn.gx-has{border-color:var(--gx-line-strong)}
.gx-row-note-btn.gx-has{color:var(--gx-fg)}
.gx-row-note-btn[hidden]{display:none}
@media (hover:none){.gx-row-note-btn{border-color:var(--gx-line-strong)}}
.gx-row-note>.gx-note,.gx-row-note .gx-field{margin-top:0}
.gx-row-note[hidden],.gx-row-note-tr[hidden],.gx-folded,.gx-row-filtered{display:none!important}
.gx-row-scroll{max-width:100%;overflow-x:auto}
.gx-more{display:inline-block;max-width:100%;box-sizing:border-box;padding:4px 11px;border-radius:13px;border:1px solid var(--gx-line-strong);background:var(--gx-surface);color:var(--gx-fg);font:500 12px/16px var(--font-sans);text-align:left;cursor:pointer}
.gx-more>span{color:var(--gx-muted);font-weight:400}
.gx-row-group{display:flex;align-items:center;gap:10px;margin:12px 0 2px;padding:6px 10px;border-radius:8px;color:var(--gx-muted);font:650 11.5px/1.4 var(--font-sans);letter-spacing:.07em;text-transform:uppercase;outline:none}
.gx-row-group[role=checkbox]{cursor:pointer}
.gx-row-group[role=checkbox]:hover{background:var(--gx-surface)}
.gx-row-glabel{flex:1;min-width:0}
.gx-row-group>small{flex:none;font-size:12px;font-weight:400;letter-spacing:0;text-transform:none;font-variant-numeric:tabular-nums;white-space:nowrap}
.gx-row-tools{display:flex;flex-wrap:wrap;align-items:center;gap:8px}
.gx-row-tools[hidden],.gx-row-tool[hidden]{display:none}
.gx-row-filter{flex:1 1 170px;min-width:0;height:30px;box-sizing:border-box;padding:0 10px;border-radius:8px;border:1px solid var(--gx-line-strong);background:var(--gx-surface);color:var(--gx-fg);font:inherit;font-size:13px}
.gx-row-filter:focus{outline:none;border-color:var(--ring)}
.gx-row-filter::placeholder{color:var(--gx-muted)}
.gx-row-tool{flex:none;height:30px;padding:0 11px;border-radius:8px;border:1px solid var(--gx-line-strong);background:transparent;color:var(--gx-text);font:500 12.5px/1 var(--font-sans);cursor:pointer;white-space:nowrap}
.gx-row-tool:hover{background:var(--gx-surface);color:var(--gx-fg)}
.gx-row-tool[aria-pressed=true]{background:color-mix(in srgb,var(--gx-pick) 12%,transparent);border-color:color-mix(in srgb,var(--gx-pick) 50%,transparent);color:var(--gx-fg)}
.gx-options>.gx-row:not(tr){padding:14px 12px;border:1px solid var(--gx-card-border);background:var(--gx-card);box-shadow:var(--gx-glass-shadow);-webkit-backdrop-filter:blur(22px) saturate(1.15);backdrop-filter:blur(22px) saturate(1.15)}
.gx-options>.gx-row:not(tr)[aria-checked=true]{border-color:color-mix(in srgb,var(--gx-pick) 70%,transparent);box-shadow:0 0 0 4px color-mix(in srgb,var(--gx-pick) 10%,transparent),var(--gx-glass-shadow)}
/* The question card under picks shows what was picked, not the list again. */
gx-question.gx-q-tied>.gx-choices{display:none}
.gx-pickbar{display:flex;flex-wrap:wrap;align-items:center;gap:6px 8px;min-height:32px;box-sizing:border-box;padding:4px 4px 4px 10px;border-radius:10px;background:var(--gx-option);border:1px solid var(--gx-option-border)}
.gx-picked-label{flex:none;font-size:12.5px;color:var(--gx-muted);white-space:nowrap}
.gx-picked-chips{display:flex;flex:1 1 140px;flex-wrap:wrap;gap:4px;min-width:0}
.gx-picked-chip{display:inline-flex;flex:none;align-items:center;max-width:100%;height:22px;box-sizing:border-box;padding:0 2px 0 8px;border-radius:999px;border:1px solid color-mix(in srgb,var(--gx-pick) 40%,transparent);background:color-mix(in srgb,var(--gx-pick) 9%,transparent);color:var(--gx-fg);font:500 11.5px/20px var(--font-sans);white-space:nowrap}
.gx-picked-chip>button{border:0;background:none;color:var(--gx-muted);font:inherit;padding:0 5px;cursor:pointer}
.gx-picked-chip>.gx-picked-go{min-width:0;max-width:240px;overflow:hidden;text-overflow:ellipsis;padding:0;color:var(--gx-fg)}
.gx-picked-chip>.gx-picked-go:only-child{padding-right:6px}
.gx-picked-none{font-size:12.5px;color:var(--gx-muted)}
.gx-picked-none>button{border:0;background:none;padding:0;color:var(--gx-fg);font:inherit;text-decoration:underline dotted;text-underline-offset:3px;cursor:pointer}
.gx-pickbar>.gx-other{width:auto;flex:none;margin-left:auto;padding:2px 9px;gap:6px;border-radius:7px;font-size:12px;line-height:20px}
.gx-pickbar>.gx-other .gx-mark{width:12px;height:12px;margin-top:4px}
.gx-pickbar>.gx-other .gx-letter{display:none}
/* The bar at the bottom while a list's rows are on screen: what is picked there, the page's count, Send. It takes the answered pill's place. */
.gx-rowbar{position:fixed;left:50%;bottom:max(var(--gx-gutter,16px),env(safe-area-inset-bottom,0px));z-index:21;transform:translateX(-50%);box-sizing:border-box;width:min(var(--gx-text-width,720px),calc(100% - 2*var(--gx-gutter,16px)));display:flex;flex-wrap:wrap;align-items:center;gap:6px 10px;padding:7px 7px 7px 14px;border-radius:14px;border:1px solid var(--gx-card-border);background:var(--gx-footer);box-shadow:var(--gx-glass-shadow);-webkit-backdrop-filter:blur(22px) saturate(1.15);backdrop-filter:blur(22px) saturate(1.15);color:var(--gx-muted);font:13px/18px var(--font-sans);opacity:0;visibility:hidden;transition:opacity .15s,visibility .15s}
.gx-rowbar.gx-on{opacity:1;visibility:visible}
.gx-rowbar-now{flex:1 1 150px;min-width:0}
.gx-rowbar-now b{color:var(--gx-fg);font-weight:600}
.gx-rowbar-tag{margin-right:6px;padding:0 7px;border-radius:999px;background:color-mix(in srgb,var(--gx-muted) 18%,transparent);font:500 10.5px/18px var(--font-mono);white-space:nowrap}
.gx-rowbar-all{flex:none;font-size:12.5px;font-variant-numeric:tabular-nums;white-space:nowrap}
.gx-rowbar>.gx-send-button{margin-left:0}
.gx-tally.gx-tally-off{visibility:hidden}
/* Compact mention menu (`*` Boosts, `+` agents, `/` or `$` skills) in any editable box on a Live page. */
#gx-mention{position:fixed;left:0;top:0;z-index:2147483647;margin:0;inset:auto;display:none;flex-direction:column;gap:2px;box-sizing:border-box;width:min(320px,calc(100vw - 16px));max-height:224px;overflow-y:auto;padding:4px;border-radius:10px;border:1px solid var(--gx-tip-border);background:var(--gx-tip-bg);color:var(--gx-tip-fg);-webkit-backdrop-filter:blur(20px) saturate(1.15);backdrop-filter:blur(20px) saturate(1.15);box-shadow:var(--gx-tip-shadow);font:400 12.5px/1.35 Inter,var(--font-sans);letter-spacing:0;text-align:left}
#gx-mention.gx-m-on{display:flex}
#gx-mention .gx-m-head{padding:5px 8px 3px;font-size:10.5px;font-weight:600;letter-spacing:.04em;text-transform:uppercase;color:color-mix(in srgb,var(--gx-tip-fg) 52%,transparent)}
#gx-mention .gx-m-row{display:flex;align-items:center;gap:7px;width:100%;box-sizing:border-box;padding:6px 8px;border:0;border-radius:6px;background:transparent;color:var(--gx-tip-fg);font:inherit;text-align:left;cursor:pointer}
#gx-mention .gx-m-row:hover,#gx-mention .gx-m-row.gx-m-sel{background:color-mix(in srgb,var(--gx-tip-fg) 10%,transparent)}
#gx-mention .gx-m-ico{flex:none;width:14px;height:14px;display:inline-flex;align-items:center;justify-content:center;color:color-mix(in srgb,var(--gx-tip-fg) 68%,transparent)}
#gx-mention .gx-m-ico svg{width:14px;height:14px;display:block}
#gx-mention .gx-m-ico.gx-m-boost{color:#d8a344}
html[data-gx-theme=light] #gx-mention .gx-m-ico.gx-m-boost{color:#a96c00}
#gx-mention .gx-m-ico.gx-m-main{color:#e2b93b}
html[data-gx-theme=light] #gx-mention .gx-m-ico.gx-m-main{color:#b08415}
#gx-mention .gx-m-lbl{flex:none;max-width:60%;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-weight:500}
#gx-mention .gx-m-det{flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;color:color-mix(in srgb,var(--gx-tip-fg) 56%,transparent);font-size:12px}
#gx-mention .gx-m-det.gx-m-work{color:#f5a524}
#gx-mention .gx-m-empty{padding:6px 8px;color:color-mix(in srgb,var(--gx-tip-fg) 56%,transparent);font-size:12px}
</style>"#;

/// The kit's behaviour: turns the first `<gx-questions>` into a working form once the page has
/// loaded, after `PAGE_API` (theme.rs) has its answered state.
///
/// CDXC:Clipboard 2026-11-02 DECISION:
/// User: "i also want to be able to paste images into the notes of questions in inline html or the inline questions you send please. when i paste an image now nothing happens. pls fix. (it can just show like this like we show it in chat: [Image #1](…/2026-11-02_Ghostex_06-18-47@2x.png)". An image pasted into a note or text box is saved on the computer through the page's own `/image` route (images.rs, beside the chat's pasted images) and written into the box as `[Image #N](path)`, the reference a chat message carries, so the answers carry it and the agent can open the file. The numbers count across every box of the page, which all go in one answers message. A textarea cannot draw a pill inside its text, so each box lists its images under it as pills (the chat's image icon and colour, the path in the tooltip, × removes the reference). The in-chat form does the same with the composer's own pills (apps/desktop/src/app/native_chat/form_attachments.rs).
///
/// CDXC:Clipboard 2026-11-11 DECISION:
/// User, of a question page whose note box showed `[Image #1](/Users/…/ghostex/i/1794352947512.png)` as text with an "Image #1 ×" chip under it: "why can't see the image? and why doesn't show as inline image #1 pill like we show in the chat composer?" A page's note and text boxes draw each pasted picture as the composer's pill, inline where it was pasted: the image icon and "Image #N" in the composer's green, one unit the caret steps over and Backspace or Delete takes out whole. Hovering the pill shows the picture above it in the tooltip's glass, with its path and Remove; clicking it opens the page's picture viewer (the floating window's, the phone sheet's), or keeps that preview open where the page has no viewer. What the agent gets is unchanged: the box's text with `[Image #N](path)` in it, kept in the kit's own textarea (hidden), which every answer, draft and check still reads and writes. A reopened or answered page draws the pills again from that text and reads the pictures through `GET /visual/<id>/image` (images.rs). The "Anything else?" box keeps its thumbnails under the text as well, as the composer keeps its tiles. Supersedes the pills listed under the box in the 2026-11-02 DECISION above. The in-chat form already drew the composer's pills (apps/desktop/src/app/native_chat/form_attachments.rs).
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
/* Note and text boxes: see the Clipboard 2026-11-11 DECISION on KIT_SCRIPT. The kit's textarea
   stays the box's value (hidden, `.gx-src`); the box the reader types in (`.gx-box`) draws that
   value with each `[Image #N](path)` as a pill and writes every edit back into it. */
var ZW=/\u200b/g,base=location.pathname.replace(/\/+$/,"");
function refs(text){var re=/\[(Image #\d+)\]\(((?:\\.|[^)\\\r\n])*)\)/g,out=[],m;while((m=re.exec(text)))out.push({label:m[1],path:m[2],source:m[0],start:m.index,end:re.lastIndex});return out;}
function fire(area){area.dispatchEvent(new Event("input",{bubbles:true}));}
function isRef(n){return !!n&&n.nodeType===1&&n.hasAttribute("data-ref");}
function isTail(n){return !!n&&n.nodeType===1&&n.tagName==="BR"&&!n.nextSibling;}
/* The box's text, or with `stop` the text and where (node, offset) falls in it. */
function boxText(ed,stop,offset){
  var out="",at=-1;
  (function walk(n){
    for(var i=0;i<n.childNodes.length;i++){
      if(n===stop&&i===offset&&at<0)at=out.length;
      var c=n.childNodes[i];
      if(c.nodeType===3){if(c===stop&&at<0)at=out.length+c.nodeValue.slice(0,offset).replace(ZW,"").length;out+=c.nodeValue.replace(ZW,"");}
      else if(isRef(c)){if(stop&&c.contains(stop)&&at<0)at=out.length+c.getAttribute("data-ref").length;out+=c.getAttribute("data-ref");}
      else if(c.nodeType===1&&c.tagName==="BR"){if(!(n===ed&&isTail(c)))out+="\n";}
      else if(c.nodeType===1){if(/^(DIV|P)$/.test(c.tagName)&&out&&!/\n$/.test(out))out+="\n";walk(c);}
    }
    if(n===stop&&offset>=n.childNodes.length&&at<0)at=out.length;
  })(ed);
  return stop?{text:out,at:at<0?out.length:at}:out;
}
function caretIn(ed){var sel=getSelection(),r=sel&&sel.rangeCount?sel.getRangeAt(0):null;if(!r||!ed.contains(r.startContainer))return null;return{start:boxText(ed,r.startContainer,r.startOffset).at,end:boxText(ed,r.endContainer,r.endOffset).at};}
function setCaret(ed,pos){
  var sel=getSelection(),r=document.createRange(),kids=ed.childNodes,placed=false;if(!sel)return;
  for(var i=0;i<kids.length&&!placed;i++){var c=kids[i];
    if(c.nodeType===3){var s=c.nodeValue,real=s.replace(ZW,"").length;
      if(pos<=real){var k=0,j=0;while(j<s.length&&(k<pos||s.charAt(j)==="\u200b")){if(s.charAt(j)!=="\u200b")k++;j++;}r.setStart(c,j);placed=true;}else pos-=real;}
    else if(isRef(c)){var n=c.getAttribute("data-ref").length;if(pos<n){r.setStartAfter(c);placed=true;}else pos-=n;}
    else if(c.tagName==="BR"&&!isTail(c)){if(pos<1){r.setStartBefore(c);placed=true;}else pos-=1;}
  }
  if(!placed){r.selectNodeContents(ed);if(isTail(ed.lastChild))r.setEndBefore(ed.lastChild);}
  r.collapse(!placed?false:true);sel.removeAllRanges();sel.addRange(r);
}
function refPill(ref){var p=el("span","gx-ref");p.contentEditable="false";p.setAttribute("data-ref",ref.source);p.setAttribute("data-path",ref.path);p.setAttribute("data-label",ref.label);p.setAttribute("role","img");p.setAttribute("aria-label",ref.label);p.innerHTML=IMAGE_ICON;p.append(ref.label);return p;}
/* Draws the box from its textarea. Every pill has text on both sides (a zero-width space where there is none), so the caret can stand before and after it. */
function renderBox(area){
  var ed=area.__gxBox;if(!ed)return;var v=area.value,at=0;ed.textContent="";
  refs(v).forEach(function(r){if(r.start>at)ed.append(v.slice(at,r.start));ed.append(refPill(r));at=r.end;});
  if(at<v.length)ed.append(v.slice(at));
  Array.prototype.forEach.call(ed.querySelectorAll(".gx-ref"),function(p){if(!p.previousSibling||p.previousSibling.nodeType!==3)p.before("\u200b");if(!p.nextSibling||p.nextSibling.nodeType!==3)p.after("\u200b");});
  if(/\n$/.test(v))ed.append(document.createElement("br"));
  boxState(area);
}
function boxState(area){
  var ed=area.__gxBox;if(area.__gxMic)area.__gxMic.hidden=area.readOnly||area.hidden;ed.classList.toggle("gx-empty",!area.value);ed.hidden=area.hidden;ed.setAttribute("data-placeholder",area.placeholder||"");ed.setAttribute("aria-readonly",area.readOnly?"true":"false");
  var mode=area.readOnly?"false":"plaintext-only";if(ed.getAttribute("contenteditable")!==mode)try{ed.contentEditable=mode;}catch(x){ed.contentEditable="true";}
}
/* Whether the box's nodes are not what its text draws: a reference typed or pasted as text, a line break the browser made an element of, a pill without its caret anchors. */
function stale(ed,v){
  if(refs(v).length!==ed.querySelectorAll(".gx-ref").length)return true;
  if(/\n$/.test(v)!==isTail(ed.lastChild))return true;
  for(var c=ed.firstChild;c;c=c.nextSibling){
    if(c.nodeType===1&&!isRef(c)&&!isTail(c))return true;
    if(isRef(c)&&(!c.previousSibling||c.previousSibling.nodeType!==3||!c.nextSibling||c.nextSibling.nodeType!==3))return true;
  }
  return false;
}
function syncBox(area,e){
  var ed=area.__gxBox;if(e&&e.isComposing)return;var v=boxText(ed);
  if(v!==area.value){area.value=v;area.__gxQuiet=true;try{fire(area);}finally{area.__gxQuiet=false;}}
  if(stale(ed,v)){var c=document.activeElement===ed?caretIn(ed):null;renderBox(area);if(c)setCaret(ed,c.start);}else boxState(area);
}
/* Takes reference `pill` out of its box's text (with one space beside it when `spaced`), and leaves the caret where it was. */
function removeRef(area,pill,spaced){
  var ed=area.__gxBox,i=Array.prototype.indexOf.call(ed.childNodes,pill);if(i<0)return;
  var s=boxText(ed,ed,i).at,e=s+pill.getAttribute("data-ref").length,v=area.value;if(v.slice(s,e)!==pill.getAttribute("data-ref"))return;
  if(spaced){if(s>0&&v.charAt(s-1)===" ")s--;else if(v.charAt(e)===" ")e++;}
  var focused=document.activeElement===ed;area.value=v.slice(0,s)+v.slice(e);fire(area);if(focused)setCaret(ed,s);
}
/* The pill beside a collapsed caret, on the side `back` names, past the zero-width anchors. */
function refBeside(ed,back){
  var sel=getSelection(),r=sel&&sel.rangeCount?sel.getRangeAt(0):null;if(!r||!r.collapsed||!ed.contains(r.startContainer))return null;
  var n=r.startContainer,o=r.startOffset,side;
  if(n.nodeType===3){var s=n.nodeValue;if(back?s.slice(0,o).replace(ZW,""):s.slice(o).replace(ZW,""))return null;side=back?n.previousSibling:n.nextSibling;}
  else side=back?n.childNodes[o-1]:n.childNodes[o];
  while(side&&side.nodeType===3&&!side.nodeValue.replace(ZW,""))side=back?side.previousSibling:side.nextSibling;
  return isRef(side)?side:null;
}
function richBox(area){
  if(area.__gxBox)return;
  var ed=el("div",area.className+" gx-box");area.classList.add("gx-src");
  ed.setAttribute("role","textbox");ed.setAttribute("aria-multiline","true");ed.spellcheck=true;
  ["aria-label","aria-labelledby"].forEach(function(a){var v=area.getAttribute(a);if(v)ed.setAttribute(a,v);});
  ed.__gxArea=area;area.__gxBox=ed;area.after(ed);
  area.focus=function(){ed.focus();setCaret(ed,area.value.length);};
  ed.addEventListener("input",function(e){syncBox(area,e);});
  ed.addEventListener("compositionend",function(){syncBox(area);});
  ed.addEventListener("keydown",function(e){
    if(area.readOnly||e.isComposing)return;
    /* Enter writes a line break into the text itself, so what the browser makes of it never decides the value. */
    if(e.key==="Enter"&&!e.ctrlKey&&!e.metaKey&&!e.altKey){
      e.preventDefault();var c=caretIn(ed);if(!c)return;var v=area.value;area.value=v.slice(0,c.start)+"\n"+v.slice(c.end);fire(area);setCaret(ed,c.start+1);return;}
    /* Backspace or Delete beside a pill takes the whole reference out, as in the composer. */
    if((e.key==="Backspace"||e.key==="Delete")&&!e.altKey&&!e.ctrlKey&&!e.metaKey){var p=refBeside(ed,e.key==="Backspace");if(p){e.preventDefault();removeRef(area,p,false);}}
  });
  renderBox(area);
}
/* The picture behind a reference: the pasted file itself while the page is open, else read from the computer through the page's own `/image` route (images.rs), else the small copy kept with the draft. */
var shots={},shown={},small={};
function imageFor(path){
  if(shots[path])return shots[path];
  var p=fetch(base+"/image?path="+encodeURIComponent(path)).then(function(r){if(!r.ok)throw new Error("unreadable");return r.blob();}).then(function(b){if(!/^image\//.test(b.type))throw new Error("not an image");return shown[path]=URL.createObjectURL(b);})
    .catch(function(error){delete shots[path];if(small[path])return shown[path]=small[path];throw error;});
  return shots[path]=p;
}
function knownImage(path,file){var url=URL.createObjectURL(file);shown[path]=url;shots[path]=Promise.resolve(url);}
/* The pill's preview: hovering a pill shows the picture above it in the tooltip's glass, with the file's path and, while the box can change, Remove; a click opens the page's picture viewer (the floating window's, the phone sheet's), else keeps the preview open. */
var peek=null,peekFor=null,peekPinned=false,peekTimer=0,peekHide=0;
function placePeek(){
  if(!peek||!peekFor||!peekFor.isConnected)return;
  var r=peekFor.getBoundingClientRect(),w=peek.offsetWidth,h=peek.offsetHeight,vw=document.documentElement.clientWidth||innerWidth,vh=innerHeight,m=8,gap=6;
  var x=Math.min(Math.max(r.left,m),Math.max(m,vw-w-m)),y=r.top-gap-h;if(y<m)y=Math.min(r.bottom+gap,Math.max(m,vh-h-m));
  peek.style.left=Math.round(x)+"px";peek.style.top=Math.round(y)+"px";
}
function hidePeek(){
  clearTimeout(peekTimer);clearTimeout(peekHide);peekPinned=false;
  if(peekFor)peekFor.classList.remove("gx-on");peekFor=null;
  Array.prototype.forEach.call(document.querySelectorAll(".gx-thumb.gx-on"),function(t){t.classList.remove("gx-on");});
  if(peek)peek.classList.remove("gx-peek-on");
}
function showPeek(pill,pinned){
  clearTimeout(peekTimer);clearTimeout(peekHide);hideTip();
  var box=pill.closest(".gx-box"),area=box&&box.__gxArea,path=pill.getAttribute("data-path"),label=pill.getAttribute("data-label");if(!area)return;
  if(peekFor!==pill)hidePeek();
  if(!peek){peek=el("div");peek.id="gx-peek";peek.setAttribute("role","dialog");if(peek.showPopover)peek.setAttribute("popover","manual");
    peek.addEventListener("pointerenter",function(){clearTimeout(peekHide);});
    peek.addEventListener("pointerleave",function(e){if(!peekPinned&&e.pointerType!=="touch")peekHide=setTimeout(hidePeek,200);});}
  if(!peek.isConnected)document.body.append(peek);
  if(peek.showPopover&&!peek.matches(":popover-open"))try{peek.showPopover();}catch(x){}
  peekPinned=peekPinned||!!pinned;
  if(peekFor===pill){if(pinned)openViewer();return;}
  peekFor=pill;pill.classList.add("gx-on");peek.setAttribute("aria-label",label);
  Array.prototype.forEach.call(document.querySelectorAll(".gx-thumb"),function(t){if(t.getAttribute("data-path")===path&&area.__gxThumbs&&t.parentNode===area.__gxPills)t.classList.add("gx-on");});
  peek.textContent="";var wait=el("div","gx-peek-wait",shown[path]?"":"Loading…"),row=el("div","gx-peek-row"),where=el("span","gx-peek-path","‎"+path+"‎");
  row.append(el("span","gx-peek-label",label),where);
  if(!area.readOnly){var x=el("button","gx-pill-x","× Remove");x.type="button";x.setAttribute("aria-label","Remove "+label);x.addEventListener("click",function(){hidePeek();removeRef(area,pill,true);});row.append(x);}
  peek.append(wait,row);placePeek();
  requestAnimationFrame(function(){if(peekFor===pill)peek.classList.add("gx-peek-on");});
  imageFor(path).then(function(src){
    if(peekFor!==pill)return;var img=el("img");img.alt=label;
    img.onload=function(){if(peekFor!==pill)return;wait.replaceWith(img);placePeek();if(peekPinned)openViewer();};
    img.onerror=function(){if(peekFor===pill)wait.textContent="This picture can’t be shown here.";};img.src=src;
  },function(){if(peekFor===pill)wait.textContent="This picture can’t be shown here.";});
  function openViewer(){
    var img=peek.querySelector("img");if(!img||!img.complete)return;
    /* The window's picture viewer takes a click on a picture (and cancels it); with no viewer the preview stays open. */
    if(!img.dispatchEvent(new MouseEvent("click",{bubbles:true,cancelable:true,button:0})))hidePeek();
  }
}
document.addEventListener("pointerover",function(e){
  if(e.pointerType==="touch")return;var pill=e.target&&e.target.closest&&e.target.closest(".gx-ref");
  if(pill===peekFor||(pill==null&&peek&&peek.contains(e.target))){clearTimeout(peekHide);return;}
  if(pill){clearTimeout(peekTimer);peekTimer=setTimeout(function(){showPeek(pill,false);},350);}
},true);
document.addEventListener("pointerout",function(e){
  var pill=e.target&&e.target.closest&&e.target.closest(".gx-ref");if(!pill)return;
  if(e.relatedTarget&&(pill.contains(e.relatedTarget)||(peek&&peek.contains(e.relatedTarget))))return;
  clearTimeout(peekTimer);if(pill===peekFor&&!peekPinned)peekHide=setTimeout(hidePeek,200);
},true);
document.addEventListener("click",function(e){var pill=e.target&&e.target.closest&&e.target.closest(".gx-ref");if(!pill)return;e.preventDefault();showPeek(pill,true);},true);
document.addEventListener("pointerdown",function(e){if(!peekFor)return;var t=e.target;if(t&&t.closest&&(t.closest(".gx-ref")||(peek&&peek.contains(t))))return;hidePeek();},true);
document.addEventListener("keydown",function(e){if(e.key==="Escape"&&peekFor)hidePeek();},true);
["scroll","wheel","resize"].forEach(function(name){addEventListener(name,function(e){if(peekFor&&!(peek&&e.target&&e.target.nodeType===1&&peek.contains(e.target)))hidePeek();},true);});
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
  Array.prototype.forEach.call(document.querySelectorAll("[data-question][data-choice],"+ROW_SEL),function(node){
    if(node.closest("gx-question"))return;
    if(node.matches(ROW_SEL)){if(node.parentElement&&node.parentElement.closest(ROW_SEL))return;var a=rowAsks(node);if(a&&(a===key||(tag&&a===tag)))wanted.push([rowValue(node),rowLabel(node)]);return;}
    var asked=(node.getAttribute("data-question")||"").trim();
    if(asked===key||(tag&&asked===tag))wanted.push([(node.getAttribute("data-choice")||"").trim()]);
  });
  if(!wanted.length)return els;
  var ranked=els.map(function(ce,i){
    var label=labelText(ce),value=(ce.getAttribute("value")||"").trim()||label,rank=Infinity;
    for(var w=0;w<wanted.length;w++)if(wanted[w].indexOf(value)>=0||wanted[w].indexOf(label)>=0){rank=w;break;}
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
  function sync(){note.placeholder=other.on()?"What would you prefer instead? (optional)":"Note (optional)";if(note.__gxBox)boxState(note);}
  function choice(nodes,value,recommended,label){
    var button=el("button","gx-choice"),body=el("span","gx-choice-body"),index=choices.length,letter=index<26?String.fromCharCode(97+index):String(index+1);
    button.type="button";button.setAttribute("role",multiple?"checkbox":"radio");button.setAttribute("aria-checked","false");

    nodes.forEach(function(node){body.append(node);});
    if(recommended)body.insertBefore(el("span","gx-rec","Recommended"),body.querySelector(":scope>small"));
    button.append(el("span","gx-mark"),body,el("span","gx-letter",letter));list.append(button);
    var c={button:button,value:value,label:label,letter:letter,recommended:!!recommended,on:function(){return button.getAttribute("aria-checked")==="true";},set:function(v){button.setAttribute("aria-checked",v?"true":"false");}};
    /* A pick toggles; one of N replaces the pick. */
    button.addEventListener("click",function(){var on=!c.on();if(on)choices.forEach(function(o){if(!multiple||o.exclusive||c.exclusive)o.set(false);});c.set(on);sync();notify();if(on&&c===other)note.focus();});
    choices.push(c);return c;
  }
  choiceEls.forEach(function(ce){var label=labelText(ce);choice(kids(ce),(ce.getAttribute("value")||"").trim()||label,ce.hasAttribute("recommended"),label);});
  var other=choice([document.createTextNode(OTHER)],OTHER,false,OTHER);other.button.classList.add("gx-other");
  has=function(){return choices.some(function(c){return c.on();})||!!note.value.trim();};
  sync();qe.append(list,note);
  return{el:qe,title:title,key:key,tag:(tag||"").trim(),multiple:multiple,choices:choices,collect:function(out){var on=choices.filter(function(c){return c.on();}).map(function(c){return c.value;});
      var written=note.value.trim();out[key]=on.length?(multiple?on:on[0]):written?null:SKIPPED;if(written)out[key+" (note)"]=written;},
    restore:function(answers){var v=answers[key],picked=Array.isArray(v)?v:v==null?[]:[v];
      choices.forEach(function(c){c.set(picked.indexOf(c.value)>=0);});
      if(typeof answers[key+" (note)"]==="string")note.value=answers[key+" (note)"];sync();live();},
    lock:function(){answered(choices.some(function(c){return c.on();})||!!note.value.trim());choices.forEach(function(c){c.button.disabled=true;});note.readOnly=true;if(!note.value)note.hidden=true;}};
}
/* The Pick button of one variant: at the right end of its heading's row (or a small head the kit adds when the variant has no heading), it clicks that choice, and shows it picked, here and in the question. `node` carries the picked ring. */
function attachPick(node,heading,c,q){
  var pick=el("button","gx-pick");pick.type="button";
  if(heading){heading.classList.add("gx-has-pick");heading.append(pick);}
  else{var head=el("div","gx-option-head");head.append(el("span","gx-option-label",c.label),pick);node.insertBefore(head,node.firstChild);}
  pick.addEventListener("click",function(){c.button.click();});
  return{q:q,c:c,node:node,
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
    var option=attachPick(node,heading,c,q);found.push(option);
  });
  return found;
}
/* CDXC:SessionChat 2026-10-30 DECISION:
   User, on a question page whose variants were not tied to its choices: "PLEASE for inline htmls we must show the pick button on the top right so i can pick one easily instead of having to pick below". An agent can forget `data-question` / `data-choice`, so the kit ties the variants itself when a question has none: a heading above the question (after the previous question, so only the mockups it asks about) that starts with a choice's letter ("A. Same layout") or reads as its label gets that choice's Pick button, exactly like a tied variant. A heading that already has a Pick, matches nothing or matches two choices (or a choice another heading also matches) stays untouched: a wrong Pick is worse than none. A question under mockups that still gets no Pick is listed in `window.__ghostexKit.untied`, which `ghostex show` and `ghostex visual check` (server/src/ghostex_cli/visual_page_check.rs) tell the author about.
   2026-11-15: "under mockups" means at least two mockup-sized blocks (160×80px or more, counted innermost) above the question, or a heading lettered like a choice. Any svg, logo or single card used to count, so list pages with icons got the warning and their authors worked around it (CSS-masked logos, questions moved into row cards). */
var LETTERED=/^(?:(?:option|variant|choice|design|idea|proposal)\s+)?([A-Za-z])(?:[.):]|\s+[\u2013\u2014-])(?:\s+|$)(.*)$/i;
var MOCKUP_MIN_WIDTH=160,MOCKUP_MIN_HEIGHT=80;
var MOCKUP="gx-wide,.gx-frame,.gx-figure,figure,img,svg,iframe,canvas,video,[class*=mock i],[class*=variant i],[class*=screen i],[class*=device i]";
function norm(text){return(text||"").toLowerCase().replace(/[^\p{L}\p{N}]+/gu," ").trim();}
function byLetterAny(m,real){return !!m&&real.some(function(c){return c.letter===m[1].toLowerCase();});}
function autoTie(questions,explicit){
  var tied=explicit.map(function(o){return o.q;}),options=[],report={auto:[],untied:[]},prev=null;
  var heads=Array.prototype.filter.call(document.querySelectorAll("h2,h3,h4,h5,h6"),function(h){
    return !h.closest("gx-question,gx-page>header,gx-questions>header,.gx-theme-switch,[data-question][data-choice],gx-pick,[data-pick],[data-pick-group]")&&!h.classList.contains("gx-has-pick")&&!h.querySelector(".gx-pick");
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
    good.forEach(function(x){options.push(attachPick(x.h,x.h,x.c,q));report.auto.push({question:q.key,heading:x.text,choice:x.c.letter});});
    if(good.length)return;
    /* Variants to pick from: at least two mockup-sized blocks above the question (counted innermost, so a band holding two frames counts as two). An icon, a logo or one picture is not a choice between variants. */
    var shown=Array.prototype.filter.call(document.querySelectorAll(MOCKUP),function(n){if(n.closest("gx-question,gx-questions>.gx-send,.gx-seg,.gx-theme-switch,#gx-tip,gx-pick,[data-pick]")||!above(n))return false;var r=n.getBoundingClientRect();return r.width>=MOCKUP_MIN_WIDTH&&r.height>=MOCKUP_MIN_HEIGHT;});
    var inner=shown.filter(function(n){return !shown.some(function(o){return o!==n&&n.contains(o);});});
    var under=lettered||inner.length>=2;
    if(under)report.untied.push({question:q.key,tag:q.tag,title:q.title});
  });
  return{options:options,report:report};
}
/* CDXC:SessionChat 2026-11-20 DECISION:
   Picks on rows. User, after a page listed feature requests as rows and asked about them in a question far below that listed them again: "why not add pick buttons on the rows themselves always?? I want ALWAYS to add the pick buttons there on the same row instead of having to scroll down to pick from there". Then, from the design pages: design A (Q4230), a checkbox at the start of every row (a radio for pick one) with the whole row as the button, groups with a checkbox, a filter, Picked only and Pick recommended from eight rows, a bar at the bottom of the screen, the keys; "do it for single choice also"; "stop showing the duplicate selections area at the bottom when it's not necessary, cuz we now pick on the graphic/row/choice ... but we should keep the notes area I can write/voice in" (a question whose every choice has its row or picture shows only what was picked, Other and its note box); a Note with the microphone on every row of every list (Q4235, which lifts the 2026-10-30 "no note box under each option" for rows only; pictures keep it); "we need to allow writing any markdown as part of those choices" (a `<gx-pick>` holds Markdown, prepare.rs renders it; `data-pick` goes on any element); long rows fold (Q4245): a row over about 16 lines shows whole blocks up to about ten and hides the rest whole behind a Show more that says what it hides.
   A row is a `<gx-pick value>` or any element with `data-pick`; its question is its own `question` / `data-question` or the nearest `data-question` around it. Its label is its first heading (the first element), else its text. */
var ROW_SEL="gx-pick,[data-pick]",FOLD_FROM=340,FOLD_SHOW=210,FOLD_HIDE_MIN=100;
function rowNodes(){return Array.prototype.filter.call(document.querySelectorAll(ROW_SEL),function(n){return !n.closest("gx-question")&&!(n.parentElement&&n.parentElement.closest(ROW_SEL));});}
function rowAsks(n){var a=n.getAttribute("question")||n.getAttribute("data-question");if(a)return a.trim();var p=n.parentElement&&n.parentElement.closest("[data-question]");return p?(p.getAttribute("data-question")||"").trim():"";}
function rowHeading(n){var f=n.firstElementChild;return f&&/^H[1-6]$/.test(f.tagName)?f:null;}
function rowLabel(n){var h=rowHeading(n);if(h)return h.textContent.replace(/\s+/g," ").trim();var t=n.textContent.replace(/\s+/g," ").trim();return t.length>80?t.slice(0,79)+"…":t;}
function rowValue(n){var v=(n.tagName==="GX-PICK"?n.getAttribute("value"):n.getAttribute("data-pick"))||"";return v.trim()||rowLabel(n);}
function rowRecommended(n){return n.hasAttribute("recommended")||n.hasAttribute("data-recommended");}
function asksFor(asked,q){return !!asked&&(asked===q.key||asked===q.title||(!!q.tag&&asked===q.tag));}
/* Before the questions are built: each row becomes a choice of its question unless the question already lists it, so the agent writes the items once. */
function addRowChoices(root){
  var rows=rowNodes();if(!rows.length)return;
  Array.prototype.forEach.call(root.querySelectorAll("gx-question"),function(qe){
    var title=(qe.getAttribute("title")||"").trim(),q={key:(qe.getAttribute("name")||"").trim()||title,title:title,tag:(qe.getAttribute("tag")||"").trim()};
    if((qe.getAttribute("type")||"").toLowerCase()==="text")return;
    var have=Array.prototype.filter.call(qe.children,function(c){return c.nodeName==="GX-CHOICE";});
    rows.forEach(function(n){
      if(!asksFor(rowAsks(n),q))return;
      var v=rowValue(n),l=rowLabel(n);
      if(have.some(function(c){var cl=labelText(c),cv=(c.getAttribute("value")||"").trim()||cl;return cv===v||cl===l||cl===v;}))return;
      var c=document.createElement("gx-choice");c.setAttribute("value",v);if(rowRecommended(n))c.setAttribute("recommended","");c.textContent=l;qe.append(c);have.push(c);
    });
  });
}
function idToken(text){var m=(text||"").match(/^\s*(#\d+|[A-Za-z][A-Za-z0-9]*-\d+)\b/);return m?m[1]:"";}
/* What Show more hides, in words: "a list, a quote and 2 paragraphs". */
function foldSummary(nodes){
  var names={P:["a paragraph","paragraphs"],UL:["a list","lists"],OL:["a list","lists"],BLOCKQUOTE:["a quote","quotes"],PRE:["a code block","code blocks"],TABLE:["a table","tables"],FIGURE:["a picture","pictures"],IMG:["a picture","pictures"],FRAME:["a mockup","mockups"],H:["a heading","headings"],X:["a block","blocks"]},n={},order=[];
  nodes.forEach(function(k){var t=k.tagName;if(/^H[1-6]$/.test(t))t="H";else if(t==="OL")t="UL";else if(k.classList.contains("gx-row-scroll"))t="TABLE";else if(k.classList.contains("gx-frame"))t="FRAME";else if(!names[t])t=k.querySelector("img,svg,picture")?"FIGURE":"X";if(!n[t]){n[t]=0;order.push(t);}n[t]++;});
  var parts=order.map(function(t){return n[t]===1?names[t][0]:n[t]+" "+names[t][1];});
  return parts.length>1?parts.slice(0,-1).join(", ")+" and "+parts[parts.length-1]:parts[0]||"";
}
/* One row: the control, the row's own content beside it, its Note, its fold. */
function attachRow(n,c,q,changed){
  var tr=n.tagName==="TR",ctl=el("span","gx-row-ctl"),side=el("span","gx-row-side"),noteBtn=el("button","gx-row-note-btn","Note"),noteWrap=el("div","gx-row-note"),note=el("textarea","gx-note"),noteRow=null,main=null,title=null;
  n.classList.add("gx-row");n.setAttribute("role",q.multiple?"checkbox":"radio");n.setAttribute("aria-checked","false");n.tabIndex=-1;ctl.setAttribute("aria-hidden","true");
  if(c.recommended)side.append(el("span","gx-rec","Recommended"));
  noteBtn.type="button";noteBtn.tabIndex=-1;side.append(noteBtn);
  var short=idToken(c.label)||c.label;note.rows=1;note.placeholder="Note on "+short+" (optional)";note.setAttribute("aria-label","Note on "+c.label);noteWrap.append(note);noteWrap.hidden=true;
  if(tr){
    var cell=el("td","gx-row-cell");cell.append(ctl);n.insertBefore(cell,n.firstChild);(n.lastElementChild||cell).append(side);
    noteRow=el("tr","gx-row-note-tr");var td=el("td");td.colSpan=n.children.length;td.append(noteWrap);noteRow.append(td);n.after(noteRow);noteRow.hidden=true;
  }else{
    title=rowHeading(n);main=el("div","gx-row-main");kids(n).forEach(function(k){main.append(k);});n.append(ctl,main);
    var label=el("span","gx-row-label");
    if(title){kids(title).forEach(function(k){label.append(k);});}else{title=el("div");main.insertBefore(title,main.firstChild);}
    title.classList.add("gx-row-title");title.append(label,side);main.append(noteWrap);
    /* A wide table in a row scrolls sideways in its own box instead of squeezing the row (code blocks already scroll). */
    Array.prototype.forEach.call(main.querySelectorAll("table"),function(t){if(t.closest(".gx-frame,.gx-row-scroll"))return;var s=el("div","gx-row-scroll");t.before(s);s.append(t);});
  }
  var o={q:q,c:c,node:n,note:note,value:c.value,open:false,long:false,more:null,shown:false};
  o.openNote=function(){if(c.button.disabled)return;o.shown=true;o.sync();var box=note.__gxBox||note;box.focus();};
  o.sync=function(){
    var on=c.on(),off=c.button.disabled,has=!!note.value.trim();
    n.setAttribute("aria-checked",on?"true":"false");n.classList.toggle("gx-row-off",off);if(off)n.setAttribute("aria-disabled","true");else n.removeAttribute("aria-disabled");
    noteBtn.textContent=has?"Note ✓":"Note";noteBtn.classList.toggle("gx-has",has);noteBtn.hidden=off;
    noteWrap.hidden=!(has||(o.shown&&!off));if(noteRow)noteRow.hidden=noteWrap.hidden;
  };
  noteBtn.addEventListener("click",function(e){e.stopPropagation();o.openNote();});
  note.addEventListener("input",function(){changed();});
  noteWrap.addEventListener("keydown",function(e){if(e.key==="Escape"&&!e.defaultPrevented){e.preventDefault();if(!note.value.trim())o.shown=false;o.sync();n.focus();}});
  /* A click on the control, the title or plain text picks; a link, picture, code, table, mockup or control does its own thing, and selecting text never picks. */
  n.addEventListener("click",function(e){
    if(c.button.disabled)return;var t=e.target;
    if(t.closest(".gx-row-note,.gx-row-note-tr,.gx-row-note-btn,.gx-more,a,button,input,textarea,select,label,summary,[contenteditable]"))return;
    if(!t.closest(".gx-row-title,.gx-row-ctl,.gx-row-cell")&&t.closest("img,svg,video,audio,canvas,iframe,pre,code,table,figure,.gx-frame,gx-tabs,details"))return;
    var s=window.getSelection&&getSelection();if(s&&!s.isCollapsed&&n.contains(s.anchorNode))return;
    c.button.click();n.focus({preventScroll:true});
  });
  /* The fold: only a row over FOLD_FROM folds, showing whole blocks up to FOLD_SHOW and hiding the rest whole, and only when at least FOLD_HIDE_MIN would hide. Rows side by side never fold. */
  o.refold=function(){
    if(!main||(n.parentElement&&n.parentElement.classList.contains("gx-options")))return;
    var blocks=Array.prototype.filter.call(main.children,function(k){return k!==title&&k!==noteWrap&&!k.classList.contains("gx-more");});
    blocks.forEach(function(k){k.classList.remove("gx-folded");});if(o.more){o.more.remove();o.more=null;}
    if(!blocks.length||!main.getClientRects().length)return;
    var top=blocks[0].getBoundingClientRect().top,total=blocks[blocks.length-1].getBoundingClientRect().bottom-top,cut=-1;
    o.long=total>=FOLD_FROM;if(!o.long)return;
    if(!o.open){for(var i=1;i<blocks.length;i++)if(blocks[i].getBoundingClientRect().bottom-top>FOLD_SHOW){cut=i;break;}
      if(cut<0||total-(blocks[cut].getBoundingClientRect().top-top)<FOLD_HIDE_MIN){o.long=false;return;}}
    var hidden=o.open?[]:blocks.slice(cut);hidden.forEach(function(k){k.classList.add("gx-folded");});
    var more=el("button","gx-more");more.type="button";
    if(o.open)more.textContent="Show less";else more.append("Show more",el("span",null," · "+foldSummary(hidden)));
    more.addEventListener("click",function(e){e.stopPropagation();o.open=!o.open;o.refold();if(o.list)o.list.sync();});
    (o.open?blocks[blocks.length-1]:blocks[cut-1]).after(more);o.more=more;
  };
  o.folded=function(){return o.long&&!o.open;};
  n.__gxRow=o;return o;
}
/* Rows tied to their questions. */
function bindRows(questions,changed){
  var found=[];
  rowNodes().forEach(function(n){
    var asked=rowAsks(n),q=questions.filter(function(x){return asksFor(asked,x)&&x.choices.length;})[0];if(!q)return;
    var v=rowValue(n),l=rowLabel(n);
    var c=q.choices.filter(function(x){return x.value===v;})[0]||q.choices.filter(function(x){return x.label===l||x.label===v;})[0];
    if(!c){if(window.console)console.warn("ghostex: no choice \""+v+"\" in question \""+asked+"\"");return;}
    found.push(attachRow(n,c,q,changed));
  });
  return found;
}
/* A question that lists again, as its choices, items the page shows above it (three or more ids such as FB-30 or #412), with no rows tied: reported for `ghostex visual check` to refuse. The kit adds nothing to such a page: wrapping rows it only guessed at would break the page's own layout of them, and the question still works. */
function repeatedLists(questions,tiedQs){
  var report=[],prev=null;
  questions.forEach(function(q){
    var qe=q.el,before=prev;prev=qe;if(tiedQs.indexOf(q)>=0)return;
    var real=q.choices.filter(function(c){return !c.button.classList.contains("gx-other");});if(real.length<3)return;
    function above(nd){return(!before||(before.compareDocumentPosition(nd)&Node.DOCUMENT_POSITION_FOLLOWING))&&(qe.compareDocumentPosition(nd)&Node.DOCUMENT_POSITION_PRECEDING)&&!nd.contains(qe);}
    var tokens=real.map(function(c){return idToken(c.label)||idToken(c.value);}),texts=[],walk=document.createTreeWalker(document.body,NodeFilter.SHOW_TEXT),t;
    while((t=walk.nextNode())){var p=t.parentElement;if(!p||!t.data.trim()||p.closest("gx-question,script,style,textarea,.gx-tally,.gx-rowbar")||!above(t))continue;texts.push(t);}
    function hits(tok){var re=new RegExp("(^|[^A-Za-z0-9-])"+tok.replace(/[.*+?^${}()|[\]\\]/g,"\\$&")+"(?![A-Za-z0-9-])");return texts.filter(function(x){return re.test(x.data);});}
    var named=[];
    real.forEach(function(c,i){var tok=tokens[i];if(tok&&hits(tok).length)named.push(tok);});
    if(named.length>=3)report.push({question:q.key,tag:q.tag,title:q.title,items:named});
  });
  return report;
}
/* The rows of one question as a list: group checkboxes, the tools (filter, Picked only, Pick recommended from eight rows; Open all when a row folds), the keys' stops, and what the bottom bar says. */
function rowList(q,rows,changed){
  var L={q:q,rows:rows,groups:[]};rows.forEach(function(o){o.list=L;});
  rows.forEach(function(o){var g=o.node.parentElement&&o.node.parentElement.closest("[data-pick-group]");if(!g)return;var G=L.groups.filter(function(x){return x.el===g;})[0];if(!G){G={el:g,rows:[]};L.groups.push(G);}G.rows.push(o);});
  L.groups.forEach(function(G){
    var head=Array.prototype.filter.call(G.el.querySelectorAll("h1,h2,h3,h4,h5,h6"),function(x){return !x.closest(".gx-row");})[0];
    if(!head){var name=(G.el.getAttribute("data-pick-group")||"").trim();if(!name)return;head=el("div",null,name);G.el.insertBefore(head,G.el.firstChild);}
    var lab=el("span","gx-row-glabel");kids(head).forEach(function(k){lab.append(k);});head.classList.add("gx-row-group");G.count=el("small");
    if(q.multiple){G.ctl=el("span","gx-row-ctl");G.ctl.setAttribute("aria-hidden","true");head.append(G.ctl);head.setAttribute("role","checkbox");head.tabIndex=-1;head.__gxList=L;
      head.addEventListener("click",function(){if(G.rows.some(function(o){return o.c.button.disabled;}))return;var full=G.rows.every(function(o){return o.c.on();});G.rows.forEach(function(o){if(o.c.on()===full)o.c.button.click();});});}
    head.append(lab,G.count);G.head=head;
  });
  var tools=el("div","gx-row-tools"),filter=null,only=null,recs=null,term="",onlyOn=false;
  function tool(text,toggle){var b=el("button","gx-row-tool",text);b.type="button";if(toggle)b.setAttribute("aria-pressed","false");tools.append(b);return b;}
  if(rows.length>=8){
    filter=el("input","gx-row-filter");filter.type="search";filter.placeholder="Filter "+rows.length+" items";filter.setAttribute("aria-label","Filter the items");tools.append(filter);
    filter.addEventListener("input",function(){term=filter.value.trim().toLowerCase();L.sync();});
    only=tool("Picked only",true);only.addEventListener("click",function(){onlyOn=!onlyOn;only.setAttribute("aria-pressed",onlyOn?"true":"false");L.sync();});
    if(q.multiple&&rows.some(function(o){return o.c.recommended;})){recs=tool("Pick recommended");recs.addEventListener("click",function(){rows.forEach(function(o){if(o.c.recommended&&!o.c.on()&&!o.c.button.disabled)o.c.button.click();});});}
  }
  var openAll=tool("Open all");openAll.hidden=true;
  openAll.addEventListener("click",function(){var fold=!rows.some(function(o){return o.folded();});rows.forEach(function(o){if(o.long||o.open){o.open=!fold;o.refold();}});L.sync();});
  /* The tools go just above the rows: before the element that holds them all (or its table). */
  var first=rows[0].node,anc=first.parentElement;while(anc&&!rows.every(function(o){return anc.contains(o.node);}))anc=anc.parentElement;
  var spot=!anc||/^(GX-QUESTIONS|GX-PAGE|BODY)$/.test(anc.tagName)?first:anc;spot=spot.closest("table")||spot;
  if(spot.parentElement&&spot.parentElement.closest("[data-pick-group]")&&spot===first)spot=spot.parentElement.closest("[data-pick-group]");
  spot.before(tools);L.filter=filter;
  L.stops=function(){var s=[];L.groups.forEach(function(G){if(G.ctl&&!G.head.classList.contains("gx-row-filtered"))s.push(G.head);});rows.forEach(function(o){if(!o.node.classList.contains("gx-row-filtered"))s.push(o.node);});
    s.sort(function(a,b){return a.compareDocumentPosition(b)&Node.DOCUMENT_POSITION_FOLLOWING?-1:1;});return s;};
  L.range=function(){var top=Infinity,bottom=-Infinity;rows.forEach(function(o){if(o.node.classList.contains("gx-row-filtered"))return;var r=o.node.getBoundingClientRect();if(!r.height)return;top=Math.min(top,r.top);bottom=Math.max(bottom,r.bottom);});var t=tools.getBoundingClientRect();if(t.height)top=Math.min(top,t.top);return{top:top,bottom:bottom};};
  L.now=function(){var on=rows.filter(function(o){return o.c.on();});return q.multiple?"<b>"+on.length+"</b> of "+rows.length+" picked":on.length?"<b>"+esc(idToken(on[0].c.label)||on[0].c.label)+"</b> picked":"Nothing picked";};
  L.sync=function(){
    rows.forEach(function(o){var hide=(onlyOn&&!o.c.on())||(!!term&&o.node.textContent.toLowerCase().indexOf(term)<0);o.node.classList.toggle("gx-row-filtered",hide);});
    L.groups.forEach(function(G){if(!G.head)return;var on=G.rows.filter(function(o){return o.c.on();}).length;G.count.textContent=q.multiple?on+" of "+G.rows.length:on?"Picked here":"";
      if(G.ctl){G.ctl.className="gx-row-ctl"+(on===G.rows.length?" gx-all":on?" gx-some":"");G.head.setAttribute("aria-checked",on===G.rows.length?"true":on?"mixed":"false");}
      G.head.classList.toggle("gx-row-filtered",G.rows.every(function(o){return o.node.classList.contains("gx-row-filtered");}));});
    var locked=rows.some(function(o){return o.c.button.disabled;});if(recs)recs.hidden=locked;
    var anyLong=rows.some(function(o){return o.long;});openAll.hidden=!anyLong;openAll.textContent=rows.some(function(o){return o.folded();})?"Open all":"Fold long rows";
    tools.hidden=!filter&&!anyLong;
    var s=L.stops();if(s.length&&!s.some(function(x){return x.tabIndex===0;}))s[0].tabIndex=0;
  };
  rows.forEach(function(o){o.node.__gxList=L;});
  return L;
}
function esc(t){return String(t).replace(/[&<>"]/g,function(ch){return{"&":"&amp;","<":"&lt;",">":"&gt;","\"":"&quot;"}[ch];});}
/* A question whose every choice has its row or picture shows what was picked as chips (a click goes back to it, × clears it), Other and its note box, not its choices again. A question with any choice that has neither keeps its list, in sync with the rows. */
function tidyCards(questions,ties){
  var cards=[];
  questions.forEach(function(q){
    var real=q.choices.filter(function(c){return !c.button.classList.contains("gx-other");}),mine=ties.filter(function(t){return t.q===q;});
    if(!real.length||!real.every(function(c){return mine.some(function(t){return t.c===c;});}))return;
    var qe=q.el,list=qe.querySelector(".gx-choices"),other=q.choices.filter(function(c){return c.button.classList.contains("gx-other");})[0];
    // Not "gx-picked": attachPick puts that class on a picked mockup card, and this bar's flex layout squashed the card.
    var bar=el("div","gx-pickbar"),chips=el("span","gx-picked-chips"),rowsOf=mine.some(function(t){return t.node.classList.contains("gx-row");});
    bar.append(el("span","gx-picked-label",rowsOf?"Picked above":"Picked"),chips);if(other)bar.append(other.button);
    list.before(bar);qe.classList.add("gx-q-tied");
    function nodeOf(c){var t=mine.filter(function(t){return t.c===c;})[0];return t&&t.node;}
    function show(node){if(!node)return;node.scrollIntoView({block:"center",behavior:"smooth"});if(node.classList.contains("gx-row"))node.focus({preventScroll:true});}
    cards.push({sync:function(){
      var locked=real.some(function(c){return c.button.disabled;}),on=real.filter(function(c){return c.on();});chips.textContent="";
      if(!on.length){var none=el("span","gx-picked-none");if(locked||(other&&other.on()))none.textContent="Nothing picked";else{none.append("Nothing yet. ");var go=el("button",null,rowsOf?"Pick on the rows above":"Pick on the designs above");go.type="button";go.addEventListener("click",function(){show(nodeOf(real[0]));});none.append(go);}chips.append(none);}
      on.forEach(function(c){var chip=el("span","gx-picked-chip"),tok=idToken(c.label),text=tok&&c.label.indexOf(tok)===0?tok:c.label,go=el("button","gx-picked-go",text);go.type="button";go.title=c.label;go.addEventListener("click",function(){show(nodeOf(c));});chip.append(go);
        if(!locked){var x=el("button",null,"×");x.type="button";x.setAttribute("aria-label","Clear "+c.label);x.addEventListener("click",function(){c.button.click();});chip.append(x);}chips.append(chip);});
    }});
  });
  return cards;
}
function setup(root){
  /* A page that asks nothing and waits to be seen (SEEN_FOOTER below): the same footer with no questions, a reply box and Mark as read. */
  var seen=root.hasAttribute("data-gx-seen");
  var used={},questions=[],options=[];
  function collect(){var out={};questions.forEach(function(q){q.collect(out);});if(seen)out.seen=true;var t=notes&&notes.value.trim();if(t)out[notesKey]=t;return out;}
  function restore(answers){questions.forEach(function(q){q.restore(answers);});if(notes)notes.value=typeof answers[notesKey]==="string"?answers[notesKey]:"";}
  function syncOptions(){options.forEach(function(o){o.sync();});}
  /* The reader's picks so far are kept on the host (PAGE_API's draft), so closing and reopening the page loses nothing. */
  function changed(){syncOptions();if(window.ghostex&&ghostex.__saveDraft)ghostex.__saveDraft("kit",collect());}
  addRowChoices(root);
  questions=Array.prototype.map.call(root.querySelectorAll("gx-question"),function(qe,index){return question(qe,index,used,changed);});
  var explicit=bindOptions(questions),rowTies=bindRows(questions,changed),auto=autoTie(questions,explicit.concat(rowTies));
  var repeated=repeatedLists(questions,explicit.concat(rowTies,auto.options).map(function(o){return o.q;})),allRows=rowTies,lists=[];
  allRows.forEach(function(o){if(!lists.some(function(L){return L.q===o.q;}))lists.push(rowList(o.q,allRows.filter(function(x){return x.q===o.q;}),changed));});
  /* A row's note goes with its question's answer as "<key> · <value> (note)", the key the answers card already folds into the question's row (packages/gx-chat-core/src/transcript/page_answers.rs). */
  allRows.forEach(function(o){var q=o.q,k=q.key+" \u00b7 "+o.value+" (note)",c0=q.collect,r0=q.restore,l0=q.lock;
    q.collect=function(out){c0(out);var t=o.note.value.trim();if(t)out[k]=t;};
    q.restore=function(a){r0(a);o.note.value=typeof a[k]==="string"?a[k]:"";};
    q.lock=function(){l0();o.note.readOnly=true;};});
  var cards=tidyCards(questions,explicit.concat(auto.options,rowTies));
  options=explicit.concat(auto.options,allRows,lists,cards);window.__ghostexKit={tied:explicit.length,auto:auto.report.auto,untied:auto.report.untied,rows:rowTies.length,repeated:repeated};syncOptions();
  function refoldAll(){allRows.forEach(function(o){o.refold();});lists.forEach(function(L){L.sync();});}
  if(allRows.length){
    refoldAll();addEventListener("load",refoldAll);if(document.fonts&&document.fonts.ready)document.fonts.ready.then(refoldAll);
    var foldWidth=root.clientWidth;if(window.ResizeObserver)new ResizeObserver(function(){if(root.clientWidth!==foldWidth){foldWidth=root.clientWidth;refoldAll();}}).observe(root);
    /* The keys in a list: arrows or J and K move between rows and group headers, Space or X picks, O opens or folds a long row, N writes its note; / goes to the filter of the list on screen. */
    document.addEventListener("keydown",function(e){
      var t=e.target;if(!t||!t.classList||e.metaKey||e.ctrlKey||e.altKey||e.isComposing)return;
      if(e.key==="/"&&!t.closest("input,textarea,select,[contenteditable]")){var vis=lists.filter(function(L){var r=L.range();return L.filter&&r.bottom>0&&r.top<innerHeight;})[0];if(vis){e.preventDefault();vis.filter.focus();}return;}
      var L=t.__gxList;if(!L||!(t.classList.contains("gx-row")||t.classList.contains("gx-row-group")))return;
      var s=L.stops(),i=s.indexOf(t),k=e.key;
      function go(j){if(j<0||j>=s.length)return;s.forEach(function(x){x.tabIndex=-1;});s[j].tabIndex=0;s[j].focus();}
      if(k==="ArrowDown"||k==="j"||k==="J"){e.preventDefault();go(i+1);}
      else if(k==="ArrowUp"||k==="k"||k==="K"){e.preventDefault();go(i-1);}
      else if(k===" "||k==="x"||k==="X"){e.preventDefault();t.click();}
      else if((k==="o"||k==="O")&&t.__gxRow&&(t.__gxRow.long||t.__gxRow.open)){e.preventDefault();t.__gxRow.open=!t.__gxRow.open;t.__gxRow.refold();L.sync();}
      else if((k==="n"||k==="N")&&t.__gxRow){e.preventDefault();t.__gxRow.openNote();}
    });
  }
  /* An exported page's answers name each question as well as its key (theme.rs PAGE_API, exported.rs): its tag, its text, the value and label of what was picked, and its note. */
  window.__ghostexKit.describe=function(a){a=a||{};var rows=questions.map(function(q){var v=a[q.key],r={tag:q.tag||null,question:q.title,key:q.key,value:v===undefined?null:v};
    if(q.choices.length){var lab=function(x){var c=q.choices.filter(function(c){return c.value===x;})[0];return c?c.label:x;};r.label=Array.isArray(v)?v.map(lab):v==null||v===SKIPPED?null:lab(v);}
    var n=a[q.key+" (note)"];r.note=typeof n==="string"?n:null;return r;});
    if(notes&&typeof a[notesKey]==="string")rows.push({tag:null,question:seen?"Reply about this page":"Anything else?",key:notesKey,value:a[notesKey],note:null});return rows;};
  /* "Anything else?": see the SessionChat 2026-11-04 DECISION on KIT_SCRIPT. */
  var last=questions[questions.length-1],notes=null,notesCard=null,notesKey=seen?"reply":NOTES;
  for(var copy=2;questions.some(function(q){return q.key===notesKey||q.key+" (note)"===notesKey;});copy++)notesKey=NOTES+" ("+copy+")";
  if(!(last&&!last.choices.length&&OWN_NOTES.test(last.title))){
    notesCard=el("div","gx-notes");var notesHead=el("div","gx-q-head"),notesTitle=el("div","gx-q-title",seen?"Reply about this page":"Anything else?");notesTitle.id="gx-notes-title";
    notes=el("textarea","gx-text");notes.setAttribute("aria-labelledby","gx-notes-title");notes.placeholder=seen?"Write a reply about the page, or paste images (optional)":"Write anything more, or paste images (optional)";notes.__gxThumbs=true;
    notes.addEventListener("input",changed);notesHead.append(notesTitle);notesCard.append(notesHead,notes);
  }
  var bar=el("div","gx-send"),button=el("button","gx-send-button"),label=el("span",null,root.getAttribute("submit")||"Send answers"),status=el("span","gx-send-status"),info=el("button","gx-send-info");
  /* The sentence about blank questions is an (i) icon with the quick tooltip, not a line of text beside the buttons (the SessionChat 2026-10-24 DECISION in apps/desktop/src/app/native_chat/ask_card.rs `ask_info_hint`). Any status message replaces the icon. */
  var infoText=seen?"The agent is told you saw the page, with your reply if you write one.":"Questions you leave blank are sent as skipped.";info.type="button";info.setAttribute("aria-label",infoText);info.setAttribute("data-gx-tip",infoText);info.innerHTML='<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 12a9 9 0 1 0 18 0a9 9 0 0 0 -18 0"/><path d="M12 9h.01"/><path d="M11 12h1v4h1"/></svg>';status.append(info);
  var mac=/Mac|iPhone|iPad/.test(navigator.platform||"");
  /* The shortcuts show in the kit's tooltip as key caps, the keys this platform uses, not on the buttons. */
  button.setAttribute("data-gx-tip",seen?"Mark as read":"Send answers");button.setAttribute("data-gx-tip-keys",mac?"⌘ Enter":"Ctrl Enter");
  /* Skip all: sends at once with every question as skipped and no notes, whatever is picked or typed (see the SessionChat DECISION on SKIPPED in packages/gx-visual/src/questions.rs). */
  var skipAll=el("button","gx-skip-all");skipAll.type="button";skipAll.append(el("span",null,"Skip all"));skipAll.setAttribute("data-gx-tip","Skip all");skipAll.setAttribute("data-gx-tip-keys",mac?"⌘ ⇧ Enter":"Ctrl Shift Enter");
  button.type="button";button.append(label);if(seen)bar.append(status,button);else bar.append(status,skipAll,button);if(notesCard)root.append(notesCard);root.append(bar);
  skipAll.addEventListener("click",function(){
    if(button.disabled)return;
    var out={};questions.forEach(function(q){out[q.key]=SKIPPED;});
    /* Once sent, the page shows what went: no picks and no notes. */
    send(out,function(){restore({});root.querySelectorAll("textarea").forEach(function(t){t.value="";});});
  });
  var dictStop=null;
  function lock(){if(dictStop)dictStop();questions.forEach(function(q){q.lock();});if(notes){notes.readOnly=true;notesCard.hidden=!notes.value.trim();}syncOptions();allPills();button.disabled=true;skipAll.remove();label.textContent=seen?"Seen":"Sent";button.removeAttribute("data-gx-tip");button.removeAttribute("data-gx-tip-keys");status.classList.remove("gx-error");status.textContent=seen?"Marked as seen. The agent was told.":"Sent to the agent.";}
  /* CDXC:SessionChat 2026-11-11 DECISION:
     User: "please show number of questions answered so far in bottom right corner always". A page with questions shows "3 of 10 answered" in a small glass pill fixed at the window's bottom right, whatever the scroll, updated as the reader answers. A question counts as answered as the tile's check and Send count it (`gx-has-answer`): a choice picked, or a note or text written; one left blank counts as not answered, also after Skip all. All answered turns it green. (2026-11-12: it stays fully on screen: User, on an Android phone's page sheet where the pill's right end was cut off by the screen's edge: "make questions bottom right not off screen pls". Its margin is the page's own gutter (16px on a phone, 24px otherwise) or the safe area, it shrinks with an ellipsis if the screen is narrower than its text, and the script places it by the visual viewport, so a web view whose layout is wider than the screen cannot push it off.) The page gets room at its end (the gutter plus 34px; measured from the pill since 2026-11-15, below), so scrolled to the bottom the pill sits below the send bar instead of on it. The floating window, a browser tab and the phone's page sheet all get it from here.
     CDXC:SessionChat 2026-11-15 DECISION: User, with a screenshot of a page on their Android foldable where the "1 of 2 answered" pill sat over an option card, among cards that overflowed and overlapped: "Please also make sure inline htmls don't look like this where stuff is over flowing and overlapping." Asked whether the pill should fade whenever content is under it or always show, the user answered "Always show", so the "always" above stands: the pill never hides. It cannot hide a control: clicks and taps go through it (`pointer-events:none`), and the page's room at its end is measured from the pill itself (its margin, its height at the current text size and a 12px gap), so scrolled to the end every card, the send bar and its buttons sit clear above it. Supersedes the fading pill tried the same day. */
  if(questions.length&&!seen){
    var tally=el("div","gx-tally");tally.setAttribute("role","status");tally.setAttribute("aria-live","polite");
    /* While a list's rows are on screen, a bar takes the pill's place: that list's picks, the page's count and Send (the SessionChat 2026-11-20 DECISION on `bindRows`). `ghostex visual check` treats it as it does the pill. */
    var rowbar=null,barNow=null,barAll=null;
    if(lists.length){rowbar=el("div","gx-rowbar");rowbar.setAttribute("role","region");rowbar.setAttribute("aria-label","Picks");barNow=el("span","gx-rowbar-now");barAll=el("span","gx-rowbar-all");
      var barSend=el("button","gx-send-button");barSend.type="button";barSend.append(el("span",null,root.getAttribute("submit")||"Send answers"));barSend.addEventListener("click",function(){button.click();});rowbar.append(barNow,barAll,barSend);}
    var barSync=function(){if(!rowbar)return;var vh=innerHeight,best=null,most=0;
      if(!button.disabled)lists.forEach(function(L){var r=L.range();if(!isFinite(r.top))return;var vis=Math.min(r.bottom,vh*0.85)-Math.max(r.top,vh*0.15);if(vis>most){most=vis;best=L;}});
      rowbar.classList.toggle("gx-on",!!best);tally.classList.toggle("gx-tally-off",!!best);
      if(best){var n=questions.filter(function(q){return q.el.classList.contains("gx-has-answer");}).length;barNow.innerHTML=(best.q.tag?'<span class="gx-rowbar-tag">'+esc(best.q.tag)+"</span>":"")+best.now();barAll.textContent=n+" of "+questions.length+" answered";}};
    var barQueued=false,barLater=function(){if(barQueued)return;barQueued=true;requestAnimationFrame(function(){barQueued=false;barSync();});};
    var tallied=function(){var n=questions.filter(function(q){return q.el.classList.contains("gx-has-answer");}).length,text=n+" of "+questions.length+" answered";if(tally.textContent!==text){tally.textContent=text;place();}tally.classList.toggle("gx-all",n===questions.length);barLater();};
    /* The pill must sit fully on the screen the reader sees. A fixed box is placed against the layout viewport, which a phone's web view (overview mode, a wider layout than the screen, a pinch zoom) can make wider than the screen, so the stylesheet's offsets are only a first guess: this measures where the pill landed against the visual viewport and moves it by the difference, with the same margin from the right and bottom edges as the stylesheet gives. */
    var place=function(){
      var vv=window.visualViewport,de=document.documentElement;
      tally.style.right=tally.style.bottom="";
      var cs=getComputedStyle(tally),gr=parseFloat(cs.right)||16,gb=parseFloat(cs.bottom)||16,r=tally.getBoundingClientRect();
      var dx=(vv?vv.offsetLeft+vv.width:de.clientWidth)-gr-r.right,dy=(vv?vv.offsetTop+vv.height:innerHeight)-gb-r.bottom;
      if(Math.abs(dx)>0.5)tally.style.right=(gr-dx)+"px";
      if(Math.abs(dy)>0.5)tally.style.bottom=(gb-dy)+"px";
      /* The room at the page's end is the pill's own height plus its margin and a gap, measured, so a larger text size (Android's font size setting) or a wider gutter still leaves the last content clear of it. */
      if(rowbar)rowbar.style.bottom=tally.style.bottom;
      room.style.height=Math.ceil(gb+Math.max(tally.getBoundingClientRect().height,rowbar?rowbar.offsetHeight:0)+ROOM_GAP)+"px";
    };
    var room=el("div","gx-tally-room"),ROOM_GAP=12;
    document.body.append(room,tally);if(rowbar){document.body.append(rowbar);addEventListener("scroll",barLater,{passive:true});addEventListener("resize",barLater);}tallied();place();
    window.addEventListener("resize",place);
    window.addEventListener("load",place);
    if(window.visualViewport){visualViewport.addEventListener("resize",place);visualViewport.addEventListener("scroll",place);}
    new MutationObserver(tallied).observe(root,{subtree:true,attributes:true,attributeFilter:["class"]});
  }
  /* The boxes the reader types in, drawn over the kit's textareas (richBox). */
  boxes().forEach(richBox);
  function boxes(){return Array.prototype.slice.call(root.querySelectorAll("textarea.gx-note,textarea.gx-text"));}
  function edited(area){fire(area);}
  /* Redraws a box from its text (unless the box itself just wrote it), and the "Anything else?" box's thumbnails under it. */
  function pills(area){
    if(area.__gxBox){if(area.__gxQuiet)boxState(area);else renderBox(area);}
    if(!area.__gxThumbs)return;
    var row=area.__gxPills;if(!row){row=el("div","gx-pills gx-thumbs");(area.__gxBox||area).after(row);area.__gxPills=row;}
    var found=refs(area.value),sig=found.map(function(r){return r.start+":"+r.source;}).join("\n")+(area.readOnly?"|locked":"");
    if(row.__gxSig!==sig){row.__gxSig=sig;row.textContent="";
      found.forEach(function(r){
        var thumb=el("span","gx-thumb"),img=el("img");img.alt=r.label;thumb.setAttribute("data-path",r.path);thumb.setAttribute("data-gx-tip",r.label+": "+r.path);
        function put(src){img.src=src;thumb.prepend(img);}
        if(shown[r.path])put(shown[r.path]);else imageFor(r.path).then(put,function(){thumb.remove();row.hidden=!row.querySelector(".gx-thumb")||area.hidden;});
        if(!area.readOnly){var x=el("button","gx-pill-x","\u00d7");x.type="button";x.setAttribute("aria-label","Remove "+r.label);x.addEventListener("click",function(){var v=area.value,s=r.start,e=r.end;if(v.slice(s,e)!==r.source)return;if(s>0&&v.charAt(s-1)===" ")s--;else if(v.charAt(e)===" ")e++;area.value=v.slice(0,s)+v.slice(e);edited(area);});thumb.append(x);}
        row.append(thumb);
      });}
    row.hidden=!row.querySelector(".gx-thumb")||area.hidden;
  }
  function allPills(){boxes().forEach(pills);}
  if(!window.ghostex){button.disabled=true;skipAll.remove();status.textContent="This Ghostex can't send answers from a page; type them in the chat.";return;}
  if(ghostex.answered){restore(ghostex.answers||{});lock();return;}
  /* An exported page has no Ghostex to send to: Send becomes Export answers, which shows the answers to copy or download, and the page stays open for more (exported.rs). */
  var exported=!!ghostex.exported;
  if(exported){label.textContent="Export answers";button.setAttribute("data-gx-tip","Export answers");skipAll.remove();infoText=seen?"Exports that you saw the page, with your reply if you write one.":"Questions you leave blank are exported as skipped.";info.setAttribute("aria-label",infoText);info.setAttribute("data-gx-tip",infoText);}
  var saved=ghostex.__draft&&ghostex.__draft("kit");if(saved&&typeof saved==="object"){restore(saved);syncOptions();}
  /* Pasted images: see the Clipboard DECISIONs on KIT_SCRIPT. */
  var pendingImages=0,kept={},keptSize=0,savedThumbs=ghostex.__draft&&ghostex.__draft("thumbs");
  function keepSmall(k,d){if(typeof d==="string"&&/^data:image\//.test(d)&&!kept[k]){small[k]=kept[k]=d;keptSize+=d.length;}}
  if(savedThumbs&&typeof savedThumbs==="object")Object.keys(savedThumbs).forEach(function(k){keepSmall(k,savedThumbs[k]);});
  /* A small JPEG of the pasted image for the draft, so a reopened page still shows its thumbnail. */
  function shrink(file){return new Promise(function(resolve){var url=URL.createObjectURL(file),img=new Image();
    img.onload=function(){var k=Math.min(1,112/Math.max(img.naturalWidth,img.naturalHeight,1)),c=document.createElement("canvas");c.width=Math.max(1,Math.round(img.naturalWidth*k));c.height=Math.max(1,Math.round(img.naturalHeight*k));var g=c.getContext("2d");g.fillStyle="rgb(128,128,128)";g.fillRect(0,0,c.width,c.height);g.drawImage(img,0,0,c.width,c.height);URL.revokeObjectURL(url);try{resolve(c.toDataURL("image/jpeg",0.7));}catch(x){resolve(null);}};
    img.onerror=function(){URL.revokeObjectURL(url);resolve(null);};img.src=url;});}
  function keepThumb(path,file){shrink(file).then(function(d){if(!d||keptSize+d.length>24000||button.disabled)return;kept[path]=small[path]=d;keptSize+=d.length;if(ghostex.__saveDraft)ghostex.__saveDraft("thumbs",kept);});}
  function nextImage(){var top=0;boxes().forEach(function(t){t.value.replace(/\[Image #(\d+)\]\(/g,function(_,n){top=Math.max(top,+n);});});return top+1;}
  function hint(area,text,error){var h=area.__gxHint;if(!h){h=el("div","gx-paste");(area.__gxPills||area.__gxBox||area).after(h);area.__gxHint=h;}h.textContent=text||"";h.classList.toggle("gx-error",!!error);h.hidden=!text;}
  function upload(file){if(exported)return Promise.reject(new Error("Pictures can't be added to an exported page. Describe them in words instead."));return fetch(base+"/image",{method:"POST",body:new Blob([file])}).then(function(r){return r.json().catch(function(){return{};}).then(function(j){if(r.ok&&j.ok&&j.path)return j.path;throw new Error(j.error||"The pasted image could not be saved.");});});}
  root.addEventListener("input",function(e){if(e.target&&e.target.matches&&e.target.matches("textarea.gx-note,textarea.gx-text"))pills(e.target);},true);
  root.addEventListener("paste",function(e){
    var t=e.target,ed=t&&t.closest&&t.closest(".gx-box"),area=ed?ed.__gxArea:t&&t.matches&&t.matches("textarea.gx-note,textarea.gx-text")?t:null;
    if(!area||area.readOnly||button.disabled)return;
    var files=[];Array.prototype.forEach.call(e.clipboardData&&e.clipboardData.items||[],function(i){if(i.kind==="file"&&/^image\//.test(i.type)){var f=i.getAsFile();if(f)files.push(f);}});
    if(!files.length)return;
    e.preventDefault();
    var at=ed&&caretIn(ed),original=area.value,start=at?at.start:ed?original.length:area.selectionStart,end=at?at.end:ed?original.length:area.selectionEnd,paths=[];
    pendingImages++;hint(area,files.length>1?"Saving the pasted images…":"Saving the pasted image…");
    files.reduce(function(chain,f){return chain.then(function(){return upload(f).then(function(p){paths.push(p);knownImage(p,f);if(area.__gxThumbs)keepThumb(p,f);});});},Promise.resolve()).then(function(){hint(area,"");},function(error){hint(area,error&&error.message||"The pasted image could not be saved.",true);}).then(function(){
      pendingImages--;if(!paths.length)return;
      /* Typing went on while it saved: the selection taken at the paste no longer fits, so the references go at the end. */
      var v=area.value,s=v===original?start:v.length,en=v===original?end:s;
      paths.forEach(function(p){var ref="[Image #"+nextImage()+"]("+p+")",lead=s>0&&!/\s/.test(v.charAt(s-1))?" ":"",ins=lead+ref+" ";v=v.slice(0,s)+ins+v.slice(en);s=en=s+ins.length;area.value=v;});
      edited(area);
      if(area.__gxBox&&document.activeElement===area.__gxBox)setCaret(area.__gxBox,s);else if(document.activeElement===area)area.setSelectionRange(s,s);
    });
  },true);
  /* Dictate: see the CDXC:Dictate 2026-11-16 DECISION in apps/desktop/src/app/window/visual_page_modal/page_dictation.rs. Where the page's host can dictate (`__ghostexHostDictate`: the floating page window) each box gets a microphone; the host records and transcribes, and the words go in at the caret (or over the selection) the box had when the recording started while its text is unchanged, at its end otherwise. */
  var dictateHost=typeof window.__ghostexHostDictate==="function"?window.__ghostexHostDictate:null;
  if(dictateHost){
    var MIC_ICON='<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 5a3 3 0 0 1 6 0v5a3 3 0 0 1-6 0z"/><path d="M5 10a7 7 0 0 0 14 0"/><path d="M8 21h8"/><path d="M12 17v4"/></svg>',
      STOP_ICON='<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="6.5" y="6.5" width="11" height="11" rx="2" fill="currentColor"/></svg>',
      SPIN_ICON='<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><path d="M12 3a9 9 0 1 0 9 9"/></svg>';
    var dictPhase="idle",dictTarget=null,keyboard=!window.matchMedia||matchMedia("(pointer:fine)").matches;
    dictStop=function(){if(dictPhase!=="idle")dictateHost("cancel");};
    /* Only a changed icon is redrawn, and the icon takes no pointer: a new icon under a press loses its click (CDXC:Dictate 2026-11-21 WHY in apps/desktop/src/app/page_dictation.rs). */
    var micLook=function(){boxes().forEach(function(a){var m=a.__gxMic;if(!m)return;
      var mine=!!dictTarget&&dictTarget.area===a,rec=mine&&(dictPhase==="recording"||dictPhase==="starting"),busy=mine&&dictPhase==="transcribing",tip=rec?"Stop and transcribe":busy?"Transcribing…":"Dictate";
      m.classList.toggle("gx-mic-on",rec);m.classList.toggle("gx-mic-busy",busy);var icon=rec?STOP_ICON:busy?SPIN_ICON:MIC_ICON;if(m.__gxIcon!==icon){m.innerHTML=icon;m.__gxIcon=icon;}
      m.setAttribute("aria-label",tip);m.setAttribute("data-gx-tip",tip);m.setAttribute("aria-pressed",rec?"true":"false");m.disabled=button.disabled||(dictPhase!=="idle"&&!mine);});};
    var dictHint=function(a,text,error){hint(a,text,error);a.__gxDictHint=!!text&&!error;};
    boxes().forEach(function(area){
      var ed=area.__gxBox;if(!ed||area.__gxMic)return;
      var field=el("div","gx-field"),m=el("button","gx-mic");area.before(field);field.append(area,ed,m);m.type="button";area.__gxMic=m;
      /* The press leaves the keyboard and the caret in the box, which shows where the words will land. */
      m.addEventListener("mousedown",function(e){e.preventDefault();});
      m.addEventListener("click",function(){
        if(area.readOnly||button.disabled)return;
        if(dictPhase==="idle"){if(document.activeElement!==ed)area.focus();var c=caretIn(ed),v=area.value;dictTarget={area:area,value:v,start:c?c.start:v.length,end:c?c.end:v.length};boxes().forEach(function(b){if(b.__gxDictHint)dictHint(b,"");});}
        else if(!dictTarget||dictTarget.area!==area||dictPhase==="transcribing")return;
        dictateHost("toggle");
      });
      boxState(area);
    });
    micLook();
    /* Escape cancels a recording or a transcription (the recording is kept on the computer). */
    root.addEventListener("keydown",function(e){if(e.key==="Escape"&&dictPhase!=="idle"){e.preventDefault();e.stopPropagation();dictateHost("cancel");}},true);
    window.addEventListener("gx-dictation",function(e){
      var d=e.detail||{},t=dictTarget,a=t&&t.area;
      if(typeof d.phase==="string"){dictPhase=d.phase;
        if(a){if(d.phase==="starting")dictHint(a,"Starting the microphone…");else if(d.phase==="recording")dictHint(a,keyboard?"Listening… press the microphone again to transcribe, or Escape to cancel.":"Listening… tap the microphone again to transcribe.");else if(d.phase==="transcribing")dictHint(a,"Transcribing…");
          /* A cancel kept the recording (`restorable`, CDXC:Dictate 2026-11-21 in apps/desktop/src/app/voice_agent/dictation.rs): say so, with Restore, which types it into this box. */
          else if(d.restorable&&!a.readOnly&&!button.disabled){dictHint(a,"Dictation cancelled. Your recording is kept.");var r=el("button","gx-dict-restore","Restore");r.type="button";r.setAttribute("data-gx-tip","Transcribe the cancelled recording into this box");r.addEventListener("mousedown",function(e){e.preventDefault();});r.addEventListener("click",function(){dictateHost("restore");});a.__gxHint.append(r);}
          else if(a.__gxDictHint)dictHint(a,"");}}
      if(typeof d.text==="string"&&d.text.trim()&&a){
        if(a.readOnly||button.disabled)dictHint(a,"Your answers were already sent, so the dictated words were not added.",true);
        else{var v=a.value,s,en;if(v===t.value){s=t.start;en=t.end;}else{var c=a.__gxBox&&document.activeElement===a.__gxBox?caretIn(a.__gxBox):null;s=c?c.start:v.length;en=c?c.end:v.length;}
          var lead=s>0&&!/\s/.test(v.charAt(s-1))?" ":"",tail=en<v.length&&!/\s/.test(v.charAt(en))?" ":"",ins=lead+d.text.trim()+tail;
          a.value=v.slice(0,s)+ins+v.slice(en);edited(a);s+=ins.length;if(a.__gxBox&&document.activeElement===a.__gxBox)setCaret(a.__gxBox,s);t.value=a.value;t.start=t.end=s;}}
      if(d.error&&a)dictHint(a,d.message||d.error,true);
      micLook();
    });
  }
  allPills();
  /* A newer draft than the one the page was served came after it loaded (PAGE_API's "gx-draft", sent only before the reader changed anything): show it. */
  document.addEventListener("gx-draft",function(){if(button.disabled)return;var s=ghostex.__draft("kit"),t=ghostex.__draft("thumbs");boxes().forEach(function(b){b.value="";});restore(s&&typeof s==="object"?s:{});if(t&&typeof t==="object")Object.keys(t).forEach(function(k){keepSmall(k,t[k]);});syncOptions();allPills();});
  function send(out,sent){
    if(pendingImages){status.classList.add("gx-error");status.textContent="Wait for the pasted image to finish saving.";return;}
    if(exported){status.classList.remove("gx-error");ghostex.submit(out).then(function(){status.textContent="Answers exported. Send them back to whoever shared this page.";});return;}
    button.disabled=true;status.classList.remove("gx-error");status.textContent="Sending…";
    /* A send that can't reach Ghostex keeps trying with everything left as typed, and says so (PAGE_API's submit). */
    ghostex.submit(out,function(message){status.classList.add("gx-error");status.textContent=message;}).then(function(){if(sent)sent();lock();},function(error){button.disabled=false;status.classList.add("gx-error");status.textContent=error&&error.message||"Your answers were not sent.";});
  }
  button.addEventListener("click",function(){send(collect());});
  /* Ctrl+Enter (Cmd+Enter on macOS) presses the send button from anywhere on the page, a note box included; with Shift it presses Skip all. */
  document.addEventListener("keydown",function(e){if(e.key==="Enter"&&(e.ctrlKey||e.metaKey)&&!e.isComposing&&!button.disabled){e.preventDefault();(e.shiftKey&&!seen?skipAll:button).click();}});
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
/* Mention menus (`*` Boosts, `+` agents, `/` or `$` skills) in every editable box on a Live page:
   kit note/reply boxes (`.gx-box`) and plain `<textarea>` / text `<input>` elements. Fetches the
   page's catalog from `GET /visual/<id>/mentions` and inserts the exact composer mention token. */
var BOOST_ICON='<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 3l1.9 5.1L19 10l-5.1 1.9L12 17l-1.9-5.1L5 10l5.1-1.9L12 3z"/><path d="M19 3v4M17 5h4M5 17v4M3 19h4"/></svg>';
var CROWN_ICON='<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 6l4 6 5-4-2 10H5L3 8l5 4z"/></svg>';
var AGENT_ICON='<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="4" y="8" width="16" height="12" rx="2"/><path d="M12 4v4M9 13v2M15 13v2"/></svg>';
var mentionCatalog=null,mentionFetch=null,mentionFetchedAt=0,mentionMenu=null,mentionState=null,mentionDismiss=null;
function canFetchMentions(){return !(window.ghostex&&(ghostex.exported||ghostex.answered))&&/^\/visual\/[0-9a-f]{32}$/.test(base);}
function loadMentions(force){
  if(!canFetchMentions())return Promise.resolve(null);
  var fresh=mentionCatalog&&Date.now()-mentionFetchedAt<10000;
  if(!force&&fresh)return Promise.resolve(mentionCatalog);
  if(mentionFetch)return mentionFetch;
  mentionFetch=fetch(base+"/mentions").then(function(r){return r.ok?r.json():null;}).then(function(j){
    mentionFetch=null;
    if(j&&j.ok){mentionCatalog=j;mentionFetchedAt=Date.now();if(mentionState&&mentionState.target===activeEditable())updateMention(mentionState.target,false);}
    return mentionCatalog;
  },function(){mentionFetch=null;return mentionCatalog;});
  return mentionFetch;
}
function isMentionTarget(node){
  if(!node||node.nodeType!==1)return false;
  if(node.classList&&node.classList.contains("gx-box"))return !!(node.__gxArea&&!node.__gxArea.readOnly&&!node.__gxArea.disabled);
  if(node.tagName==="TEXTAREA")return !node.classList.contains("gx-src")&&!node.readOnly&&!node.disabled;
  if(node.tagName==="INPUT"){
    if(node.classList.contains("gx-row-filter")||node.readOnly||node.disabled)return false;
    var t=(node.type||"text").toLowerCase();
    return t==="text"||t==="search"||t==="";
  }
  return false;
}
function activeEditable(){var a=document.activeElement;return isMentionTarget(a)?a:null;}
function editableState(target){
  if(target.classList&&target.classList.contains("gx-box")){
    var area=target.__gxArea,c=caretIn(target);
    if(!area||!c)return null;
    return{text:area.value,start:c.start,end:c.end,box:target,area:area};
  }
  var s=typeof target.selectionStart==="number"?target.selectionStart:target.value.length;
  var e=typeof target.selectionEnd==="number"?target.selectionEnd:s;
  return{text:target.value||"",start:s,end:e,box:null,area:target};
}
function filterBoosts(list,query){
  var q=(query||"").toLowerCase();
  return(list||[]).filter(function(b){
    if(!q)return true;
    return(b.label||"").toLowerCase().split(/[^a-z0-9]+/).some(function(w){return w&&w.indexOf(q)===0;});
  });
}
function filterSessions(list,query){
  var q=(query||"").trim().toLowerCase();
  if(!q)return(list||[]).slice();
  var words=q.split(/\s+/).filter(Boolean),sections=[],matched=[];
  (list||[]).forEach(function(s,idx){
    var title=(s.title||"").toLowerCase(),hay=title+" "+(s.projectName||"").toLowerCase()+" "+(s.computer||"").toLowerCase();
    if(!words.every(function(w){return hay.indexOf(w)>=0;}))return;
    var sec=sections.indexOf(s.section||"");if(sec<0){sec=sections.length;sections.push(s.section||"");}
    matched.push({sec:sec,later:title.indexOf(q)!==0?1:0,idx:idx,item:s});
  });
  matched.sort(function(a,b){return a.sec-b.sec||a.later-b.later||a.idx-b.idx;});
  return matched.map(function(m){return m.item;});
}
function filterSkills(list,query){
  var q=(query||"").trim().toLowerCase();
  function rank(s){
    var name=(s.name||"").toLowerCase(),det=(s.detail||"").toLowerCase();
    if(!q)return 0;
    if(name===q)return 0;
    if(name.indexOf(q)===0)return 1;
    if(name.indexOf(q)>0)return 2;
    if(det.indexOf(q)>=0)return 3;
    return -1;
  }
  var out=[];
  (list||[]).forEach(function(s,idx){var r=rank(s);if(r>=0)out.push({cmd:s.command?1:0,r:r,name:(s.name||"").toLowerCase(),idx:idx,item:s});});
  out.sort(function(a,b){return a.cmd-b.cmd||a.r-b.r||(a.name<b.name?-1:a.name>b.name?1:a.idx-b.idx);});
  return out.map(function(m){return m.item;});
}
function detectMention(text,caret){
  var before=text.slice(0,caret),lastWs=-1;
  for(var i=before.length-1;i>=0;i--)if(/\s/.test(before.charAt(i))){lastWs=i;break;}
  var tokStart=lastWs+1,tok=before.slice(tokStart);
  if(tok.charAt(0)==="*")return{kind:"boost",start:tokStart,end:caret,query:tok.slice(1)};
  if(tok.charAt(0)==="$"){var q=tok.slice(1);if(!/\s/.test(q))return{kind:"skill",slash:false,start:tokStart,end:caret,query:q};}
  if(tok.charAt(0)==="/"){var sq=tok.slice(1);if(sq.indexOf("/")<0)return{kind:"skill",slash:true,start:tokStart,end:caret,query:sq};}
  /* `+` session trigger: allows spaces in multi-word titles up to 60 chars, matching trigger.rs. */
  var from=Math.max(0,caret-60);
  for(var p=caret-1;p>=from;p--){
    var ch=before.charAt(p);
    if(ch==="\n"||ch==="\r")break;
    if(ch==="+"&&(p===0||/\s/.test(before.charAt(p-1)))){
      var tail=before.slice(p+1);
      if(tail.length>0&&/^[0-9+\-\s]/.test(tail))continue;
      if(/\s$/.test(tail)&&tail.trim().length===0)continue;
      return{kind:"session",start:p,end:caret,query:tail};
    }
  }
  return null;
}
function buildMentionPopup(trigger,cat){
  if(!trigger||!cat)return null;
  var rows=[],heading="",empty=null;
  if(trigger.kind==="boost"){
    var b=filterBoosts(cat.boosts,trigger.query);
    if(!b.length)return null;
    var firstB=b[0].section||"Boosts",prev=firstB;
    heading=firstB;
    b.forEach(function(item,i){
      var sec=item.section||"";
      rows.push({label:item.label,detail:item.description||"",token:item.token,section:i>0&&sec&&sec!==prev?sec:"",spaced:false,kind:"boost"});
      if(sec)prev=sec;
    });
    return{kind:"boost",heading:heading,rows:rows,empty:null,trigger:trigger};
  }
  if(trigger.kind==="session"){
    var ss=filterSessions(cat.sessions,trigger.query);
    if(!ss.length)return null;
    var firstSec=ss[0].section||"Agents in this project",prevS=firstSec;
    heading=firstSec;
    ss.forEach(function(item,i){
      var sec=item.section||"";
      rows.push({label:item.title,detail:item.detail||"",token:item.token,section:i>0&&sec&&sec!==prevS?sec:"",spaced:true,kind:"session",working:!!item.working,coordinator:!!item.coordinator,mainCoordinator:!!item.mainCoordinator});
      if(sec)prevS=sec;
    });
    return{kind:"session",heading:heading,rows:rows,empty:null,trigger:trigger};
  }
  if(trigger.kind==="skill"){
    var sk=filterSkills(cat.skills,trigger.query);
    if(!sk.length){
      if(trigger.slash)return null;
      return{kind:"skill",heading:cat.skillsHeading||"Skills",rows:[],empty:"No skills found",trigger:trigger};
    }
    var baseHead=cat.skillsHeading||"Skills",firstCmd=sk[0].command;
    heading=firstCmd?"Custom commands":baseHead;
    var sawCmd=firstCmd;
    sk.forEach(function(item,i){
      var sec="";
      if(i>0&&item.command&&!sawCmd){sec="Custom commands";sawCmd=true;}
      rows.push({label:(trigger.slash?"/":"$")+item.name,detail:item.detail||"",token:item.token,section:sec,spaced:false,kind:"skill"});
    });
    return{kind:"skill",heading:heading,rows:rows,empty:null,trigger:trigger};
  }
  return null;
}
function hideMention(){
  mentionState=null;
  if(mentionMenu){
    mentionMenu.classList.remove("gx-m-on");
    if(mentionMenu.hidePopover&&mentionMenu.matches(":popover-open"))try{mentionMenu.hidePopover();}catch(x){}
  }
}
function placeMention(target){
  if(!mentionMenu)return;
  var r=target.getBoundingClientRect();
  if(target.classList&&target.classList.contains("gx-box")){
    var sel=getSelection();
    if(sel&&sel.rangeCount&&target.contains(sel.anchorNode)){
      var cr=sel.getRangeAt(0).getBoundingClientRect();
      if(cr&&(cr.width>0||cr.height>0))r=cr;
    }
  }
  var w=mentionMenu.offsetWidth||300,h=mentionMenu.offsetHeight||180,vw=document.documentElement.clientWidth||innerWidth,vh=innerHeight,m=8,gap=6;
  var x=Math.min(Math.max(r.left,m),Math.max(m,vw-w-m));
  var y=r.bottom+gap;
  if(y+h>vh-m&&r.top-gap-h>=m)y=r.top-gap-h;
  else if(y+h>vh-m)y=Math.max(m,vh-h-m);
  mentionMenu.style.left=Math.round(x)+"px";mentionMenu.style.top=Math.round(y)+"px";
}
function renderMention(){
  if(!mentionState){hideMention();return;}
  if(!mentionMenu){
    mentionMenu=el("div");mentionMenu.id="gx-mention";mentionMenu.setAttribute("role","listbox");
    if(mentionMenu.showPopover)mentionMenu.setAttribute("popover","manual");
    mentionMenu.addEventListener("pointerdown",function(e){e.preventDefault();});
    mentionMenu.addEventListener("mousedown",function(e){e.preventDefault();});
  }
  if(!mentionMenu.isConnected&&document.body)document.body.append(mentionMenu);
  var pop=mentionState.popup,sel=mentionState.selected;
  mentionMenu.textContent="";
  if(pop.heading)mentionMenu.append(el("div","gx-m-head",pop.heading));
  if(!pop.rows.length&&pop.empty)mentionMenu.append(el("div","gx-m-empty",pop.empty));
  pop.rows.forEach(function(row,idx){
    if(row.section)mentionMenu.append(el("div","gx-m-head",row.section));
    var b=el("button","gx-m-row"+(idx===sel?" gx-m-sel":""));b.type="button";b.setAttribute("role","option");b.setAttribute("aria-selected",idx===sel?"true":"false");
    if(row.kind==="boost"){var ic=el("span","gx-m-ico gx-m-boost");ic.innerHTML=BOOST_ICON;b.append(ic);}
    else if(row.kind==="session"){
      var sic=el("span","gx-m-ico"+(row.coordinator&&row.mainCoordinator?" gx-m-main":""));
      sic.innerHTML=row.coordinator?CROWN_ICON:AGENT_ICON;b.append(sic);
    }
    b.append(el("span","gx-m-lbl",row.label));
    if(row.detail)b.append(el("span","gx-m-det"+(row.working?" gx-m-work":""),row.detail));
    b.addEventListener("click",function(e){e.preventDefault();e.stopPropagation();pickMention(idx);});
    mentionMenu.append(b);
  });
  mentionMenu.classList.add("gx-m-on");
  if(mentionMenu.showPopover&&!mentionMenu.matches(":popover-open"))try{mentionMenu.showPopover();}catch(x){}
  placeMention(mentionState.target);
  var active=mentionMenu.querySelector(".gx-m-sel");
  if(active&&active.scrollIntoView)try{active.scrollIntoView({block:"nearest"});}catch(x){}
}
function pickMention(index){
  if(!mentionState)return;
  var target=mentionState.target,pop=mentionState.popup,row=pop.rows[index];
  if(!row){hideMention();return;}
  var ed=editableState(target);
  if(!ed){hideMention();return;}
  var tr=pop.trigger,before=ed.text.slice(0,tr.start),after=ed.text.slice(tr.end);
  var lead=row.spaced&&before.length>0&&!/\s$/.test(before)?" ":"";
  var hasSpace=after.charAt(0)===" ";
  var ins=lead+row.token+(hasSpace?"":" ");
  var next=before+ins+after,caret=before.length+lead.length+row.token.length+1;
  mentionDismiss=null;
  hideMention();
  if(ed.box){
    ed.area.value=next;
    fire(ed.area);
    renderBox(ed.area);
    ed.box.focus();
    setCaret(ed.box,caret);
  }else{
    ed.area.value=next;
    ed.area.focus();
    try{ed.area.setSelectionRange(caret,caret);}catch(x){}
    fire(ed.area);
  }
}
function updateMention(target,refreshBoosts){
  if(!isMentionTarget(target)||!canFetchMentions()){hideMention();return;}
  var ed=editableState(target);
  if(!ed||ed.start!==ed.end){hideMention();return;}
  var tr=detectMention(ed.text,ed.start);
  if(!tr){mentionDismiss=null;hideMention();return;}
  if(mentionDismiss&&mentionDismiss.target===target&&mentionDismiss.kind===tr.kind&&mentionDismiss.start===tr.start){hideMention();return;}
  if(!mentionCatalog){loadMentions(false);mentionState={target:target,popup:{kind:tr.kind,heading:"",rows:[],empty:null,trigger:tr},selected:0};return;}
  if(refreshBoosts&&tr.kind==="boost"&&Date.now()-mentionFetchedAt>=10000)loadMentions(true);
  var pop=buildMentionPopup(tr,mentionCatalog);
  if(!pop){hideMention();return;}
  var prevSel=mentionState&&mentionState.target===target&&mentionState.popup.kind===pop.kind&&mentionState.popup.trigger.start===tr.start?mentionState.selected:0;
  mentionState={target:target,popup:pop,selected:pop.rows.length?Math.min(prevSel,pop.rows.length-1):0};
  renderMention();
}
function setupMentions(){
  if(!canFetchMentions())return;
  document.addEventListener("focusin",function(e){if(isMentionTarget(e.target))loadMentions(false);},true);
  document.addEventListener("input",function(e){var t=activeEditable();if(t)updateMention(t,true);},true);
  document.addEventListener("selectionchange",function(){var t=activeEditable();if(t)updateMention(t,false);});
  document.addEventListener("focusout",function(){setTimeout(function(){if(!activeEditable())hideMention();},0);},true);
  document.addEventListener("pointerdown",function(e){
    if(!mentionState)return;
    var t=e.target;
    if(mentionMenu&&t&&mentionMenu.contains(t))return;
    if(t&&mentionState.target&&(t===mentionState.target||mentionState.target.contains(t)))return;
    hideMention();
  },true);
  document.addEventListener("keydown",function(e){
    if(!mentionState||!mentionState.popup||!mentionMenu||!mentionMenu.classList.contains("gx-m-on"))return;
    var t=activeEditable();if(!t||t!==mentionState.target)return;
    var rows=mentionState.popup.rows;
    if(e.key==="Escape"){
      e.preventDefault();e.stopPropagation();
      mentionDismiss={target:t,kind:mentionState.popup.kind,start:mentionState.popup.trigger.start};
      hideMention();return;
    }
    if(!rows.length)return;
    if(e.key==="ArrowDown"){
      e.preventDefault();e.stopPropagation();
      mentionState.selected=(mentionState.selected+1)%rows.length;renderMention();return;
    }
    if(e.key==="ArrowUp"){
      e.preventDefault();e.stopPropagation();
      mentionState.selected=(mentionState.selected+rows.length-1)%rows.length;renderMention();return;
    }
    if((e.key==="Enter"&&!e.shiftKey&&!e.ctrlKey&&!e.metaKey&&!e.altKey)||e.key==="Tab"){
      e.preventDefault();e.stopPropagation();
      pickMention(mentionState.selected);return;
    }
  },true);
  ["scroll","resize"].forEach(function(name){addEventListener(name,function(){if(mentionState&&mentionMenu&&mentionMenu.classList.contains("gx-m-on"))placeMention(mentionState.target);},true);});
}
/* SEEN_FOOTER: a page that asks nothing, published to wait until the reader has looked at it (`awaitsSeen`, server/src/visual_pages/publish.rs), ends with a reply box and Mark as read, which send `{"seen": true, "reply": …}` as its answers. It goes at the very end of the page, so the reader reaches the end to press it. */
function seenFooter(){var r=document.createElement("gx-questions");r.setAttribute("data-gx-seen","");r.setAttribute("submit","Mark as read");(document.querySelector("gx-page")||document.body).append(r);setup(r);}
function init(){followPaint();addThemeSwitch();setupTabs();var root=document.querySelector("gx-questions");if(root)setup(root);else if(window.ghostex&&ghostex.awaitsSeen)seenFooter();setupMentions();}
if(document.readyState==="loading")document.addEventListener("DOMContentLoaded",init);else init();
addEventListener("load",followPaint);
})();</script>"#;
