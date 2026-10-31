---
name: ghostex-visuals
description: >-
  Show charts, stat tiles, tables and text layouts inline in the Ghostex chat
  with a fenced ```visual block, ask the reader questions as a form in the
  chat with a ```questions block, and publish self-contained HTML pages (UI
  mockups, small interactive tools, questions that need a picture) with
  `ghostex show` so the chat shows them as a card that opens the page. Use
  only when the user asks for $ghostex-visuals.
disable-model-invocation: true
---

# ghostex-visuals

Ghostex draws a fenced ```visual block in your reply natively, inline in the
chat, on desktop, web and phone.

- Prefer a block for charts, tables, stats and text layouts.
- Ask questions in a ```questions block: the chat shows them as a form.
- Use an HTML page only for what a block cannot express: UI mockups, small
  interactive tools, and questions the reader can only answer by seeing a
  picture.
- Don't restate in prose what the visual already shows. Add only what it
  doesn't: the conclusion, a caveat, the next step.

## Visual blocks

One JSON object in a fence whose info string is `visual`. The block is one of:

- **A Vega-Lite spec** at the top level: an object with `"mark"`. It may have
  `"title"` and `"height"`.
- **A composition**: `{"title"?: "…", "rows": [part, …]}`, drawn top to
  bottom. A part is one of the following, and every part may have `"title"`:
  - `{"chart": <Vega-Lite spec>}`
  - `{"stats": [{"label": "…", "value": "…", "note"?: "…", "tone"?: "good" | "bad" | "neutral"}]}`
    (`value` may be a number)
  - `{"table": {"columns": ["…", …], "rows": [["…", …], …]}}`, or rows as
    objects keyed by column name (then `"columns"` is optional)
  - `{"text": "…"}`
  - `{"columns": [part, …]}`: parts side by side
- **A page**: `{"page": {"title": "…", "url": "…"}}`. Only ever paste this from
  `ghostex show` output (below); never write one yourself.

### The Vega-Lite subset

- `mark`: `bar`, `line`, `area`, `point` (also `circle`, `square`), `arc` (pie;
  `{"type": "arc", "innerRadius": 50}` for a donut). The object form
  `{"type": "line", "point": true}` adds points to a line.
- Encoding channels: `x`, `y`, `color`, `theta`, `tooltip`, `xOffset` (grouped
  bars).
- Field definition keys: `field`, `type` (`quantitative`, `nominal`,
  `ordinal`, `temporal`), `aggregate` (`sum`, `mean`, `count`, `min`, `max`,
  `median`), `title`, `sort` (`"ascending"`, `"descending"`, `null` for data
  order, or an array of values), `axis` (`null` to hide it, or
  `{"title": …}`), `scale` (`{"domain": [min, max], "zero": false}`), `stack`
  (`null`/`false`, `true`, `"normalize"`).
- Data only as `"data": {"values": [ … ]}` with the rows inlined; `data.url`
  isn't supported. Aggregate large data before writing the block.
- Not supported: `layer`, `facet`, `repeat`, `concat`, `transform`,
  `params`/selections, `bin`, and the `rule`, `text`, `tick` and `rect` marks.
  Compute bins, totals or moving averages into the rows yourself.
- The width always fits the chat column; `"height"` sets the chart height
  (100 to 600).

### Check the block before replying

Write the JSON to a temporary file and draw it:

```bash
ghostex visual check /tmp/block.json              # prints a PNG path, or names the problem
ghostex visual check /tmp/block.json --light      # light theme
ghostex visual check /tmp/block.json --width 360  # phone width
```

Look at the PNG, fix what reads badly, then paste the same JSON into your
reply. If `ghostex visual` is an unknown command, this Ghostex is too old to
check: skip the check.

### Examples

Grouped bars:

```visual
{
  "title": "Build time by platform",
  "mark": "bar",
  "height": 220,
  "data": {"values": [
    {"platform": "macOS", "build": "debug", "minutes": 4.2},
    {"platform": "macOS", "build": "release", "minutes": 9.8},
    {"platform": "Windows", "build": "debug", "minutes": 5.1},
    {"platform": "Windows", "build": "release", "minutes": 12.4},
    {"platform": "Linux", "build": "debug", "minutes": 3.6},
    {"platform": "Linux", "build": "release", "minutes": 8.9}
  ]},
  "encoding": {
    "x": {"field": "platform", "type": "nominal", "sort": null, "axis": {"title": null}},
    "xOffset": {"field": "build"},
    "y": {"field": "minutes", "type": "quantitative", "title": "Minutes"},
    "color": {"field": "build", "type": "nominal", "title": "Build"}
  }
}
```

For stacked bars, drop `xOffset`; bars that share an `x` stack by `color`.

A line chart with two series:

```visual
{
  "title": "Response time",
  "mark": {"type": "line", "point": true},
  "data": {"values": [
    {"day": "2026-10-01", "series": "p50", "ms": 120},
    {"day": "2026-10-02", "series": "p50", "ms": 118},
    {"day": "2026-10-03", "series": "p50", "ms": 131},
    {"day": "2026-10-01", "series": "p95", "ms": 340},
    {"day": "2026-10-02", "series": "p95", "ms": 310},
    {"day": "2026-10-03", "series": "p95", "ms": 365}
  ]},
  "encoding": {
    "x": {"field": "day", "type": "temporal", "title": "Day"},
    "y": {"field": "ms", "type": "quantitative", "title": "Milliseconds"},
    "color": {"field": "series", "type": "nominal", "title": "Percentile"},
    "tooltip": [{"field": "day", "type": "temporal"}, {"field": "ms", "type": "quantitative"}]
  }
}
```

Stats above a table:

```visual
{
  "title": "Test run",
  "rows": [
    {"stats": [
      {"label": "Passed", "value": 412, "tone": "good"},
      {"label": "Failed", "value": 3, "note": "all in sync/", "tone": "bad"},
      {"label": "Duration", "value": "2m 41s"}
    ]},
    {"title": "Failures", "table": {
      "columns": ["Test", "Error"],
      "rows": [
        ["sync::resume_after_sleep", "timed out after 30s"],
        ["sync::merge_conflict", "expected 2 rows, got 3"],
        ["sync::large_file", "permission denied"]
      ]
    }}
  ]
}
```

## Questions

When you need the reader's decisions, put the questions in a ```questions
block in your reply. The chat draws it as a form under your message, on
desktop, web and phone: every question at once, choices picked with a click,
a note box per question, and one Send button. Use it instead of asking in
prose or as a numbered list, and instead of your agent's own question tool
(AskUserQuestion and the like), which shows one question at a time. Only when
the reader has to see something to answer (a UI mockup, a layout, a diagram)
use a question page instead (see "Question pages" below).

First run `ghostex visual --help`: if it doesn't mention ```questions, this
Ghostex can't show the form, so ask in prose instead.

```questions
{
  "title": "Settings redesign: layout and platforms",
  "questions": [
    {"tag": "Q1", "title": "Which layout should I build?",
     "detail": "Why it matters, in a sentence or two.",
     "choices": [
       {"label": "Tabs", "detail": "One page per area.", "value": "tabs", "recommended": true},
       {"label": "One long page", "value": "one page"}
     ]},
    {"title": "Which platforms first?", "multiple": true, "choices": ["macOS", "Windows", "Phone"]},
    {"title": "What should the new page be called?", "type": "text"}
  ]
}
```

- Always give the form a `title`: a short, specific name for what it asks,
  such as "Settings redesign: layout and platforms". Ghostex lists every open
  form and page by this name (the chat's "?" pill shows them under
  Unanswered, For later and Answered), so a generic title ("Questions",
  "Decisions", "A few questions") can't be told apart from the others.
  `submit` renames the button ("Send answers").
- Each question: `title`; `detail`, a short explanation; `tag`, an optional
  short label such as an issue id; `name`, the answer's key (the title when
  there is none); `multiple` for several picks; `type: "text"` for a text box
  instead of choices (`placeholder` sets its hint). At most 20 questions.
- Each choice is its text, or `{"label", "detail", "value", "recommended"}`:
  `value` is what you receive (the label when there is none) and
  `recommended` adds a Recommended badge. At most 26 choices.
- Write only the questions. The chat letters the choices, ends every choice
  question with "Other / none of these" and a note box, and adds the Send
  button. Nothing is ever preselected (a pick clicked again clears). Newer
  Ghostex versions send a question left blank as `"skipped"` (older ones as
  `null`). Don't add your own "Other" choice.
  A form of two or more questions also ends with a notes box for anything
  else, so don't add your own "Anything else?" question either. Newer Ghostex
  versions also have a Skip all button; a question the reader leaves blank
  arrives as `"skipped"`. Don't add your own Skip choice.
- Check the block before replying: `ghostex visual check /tmp/questions.json`
  lists the questions and their answer keys, or names the problem.
- Give questions a `tag` and refer to them by it in your replies ("Q3 is still
  open"): newer Ghostex versions turn the tag into a link to the form, and a
  "Previous question" pill steps through the questions still waiting for an
  answer.
- A question that no longer matters should stop counting as unanswered. Asking
  every tag of an earlier form again, in a later form or page, replaces it. When
  you ask only some of an earlier form's questions again, the ones you leave out
  keep counting there: withdraw any of them that are settled, in that same
  block. To drop tags without asking anything, write a block with only
  `withdraw`; it also works beside new questions. When the user answers a
  question another way (in the chat, in another form or page, or by doing what
  it asked), withdraw its tag in your next block instead of asking it again:

  ```questions
  {"withdraw": ["Q3", "Q4"]}
  ```

  Only when `ghostex visual --help` mentions `withdraw`: older Ghostex
  versions show this block as an error.
- Put one form in a reply and end your turn. Don't poll or wait in a loop: the
  answers arrive as the next user message, in the same envelope a question
  page uses, and the form then reads Answered. A note arrives under the
  question's key plus ` (note)`, and the form's notes box, when filled in,
  arrives last as `"Additional notes"`. A skipped question arrives as
  `"skipped"`: move on, and don't ask it again unless it blocks the work:

  ```text
  <ghostex-page-answers page="5f0c…" title="Settings redesign">
  {
    "Which layout should I build?": "tabs",
    "Which layout should I build? (note)": "keep search at the top",
    "Which platforms first?": ["macOS", "Other / none of these"],
    "Which platforms first? (note)": "Linux too",
    "What should the new page be called?": null,
    "Which color scheme?": "skipped",
    "Additional notes": "ship macOS first, the rest can wait"
  }
  </ghostex-page-answers>
  ```

## HTML pages

For a mockup or a small interactive tool, write **one self-contained HTML file**
from this skeleton, and keep its `<meta name="ghostex-page" content="v2">`:

```html
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="ghostex-page" content="v2">
<title>Changes view: one scope toggle</title>
<style>
  /* Only what your mockups need: no page chrome (see below). */
</style>
</head>
<body>
<gx-page>
  <header>
    <span class="gx-eyebrow">Changes view</span>
    <h1>One scope toggle</h1>
    <p class="gx-lede">What this page is for, in a sentence or two.</p>
  </header>

  <h2>Today</h2>
  <p>What the reader should notice.</p>
  <figure class="gx-figure">
    <div class="gx-frame"><!-- the mockup, at its real size --></div>
    <figcaption>Drawn from native_changes/render.rs</figcaption>
  </figure>
</gx-page>
</body>
</html>
```

The marker tells Ghostex the page was written for this layout. Without it the
reader sees a notice that an older version of this skill made the page, and
`ghostex show` says so when you publish.

### Title

Give every page a short, specific, human title in `<title>`: what it asks or
shows, the way you would name it to the reader, such as "Changes view: one
scope toggle" or "Cloud sessions: where the terminal switch goes". Ghostex
uses it as the page's name everywhere: the card in the chat and the list of
open questions and pages under the chat's "?" pill. Never a generic title
("Questions", "Mockups", "Report", "Options", "Page"), which can't be told
apart from the others in that list. Repeat it in the `<h1>` (the eyebrow can
carry the area, as in the skeleton). `ghostex show --title "…"` overrides
`<title>` and follows the same rule.

### Layout

- Everything goes in `<gx-page>`, which centres its children in one 720px
  column. Text, notes, mockups up to 720px wide and question cards all sit
  there.
- Use `<gx-wide>` only for something wider than the column: a mockup of a
  whole window, or variants side by side. It is a centred band up to 1120px,
  and what it holds is centred as a group, so a heading lines up with the
  mockup under it. `<gx-wide size="full">` spans the whole window; it is
  rarely needed.
- A mockup keeps its real size: give it a fixed width inside a `.gx-frame`,
  which scrolls sideways on a narrow window instead of squeezing it. Put
  notes and captions above or below it, never beside it.

### Phones

Every page must read well on a phone, 360 to 430px wide: readers often open
pages in the Ghostex phone app. Write for one column first:

- Side by side only with a rule that stacks it on a phone: cards in a
  `.gx-grid` (side by side when there is room, one per row on a phone; set
  `style="--gx-grid-min:300px"` for wider cards), your own
  `grid-template-columns: repeat(auto-fit, minmax(min(100%, 280px), 1fr))`,
  or a `@media (max-width: 600px)` rule. Never a bare
  `grid-template-columns: 1fr 1fr` or a row of fixed widths.
- Pills, chips, badges, keys and IDs get `white-space: nowrap` and
  `flex: none`, so `SPX-123` never breaks into `SPX-` over `123`.
- Anything with a real, fixed width (a device frame, a toolbar or composer
  mockup) goes in a `.gx-frame` with a `min-width` at its real size: it
  scrolls sideways inside its own box, with a fade at the edge, instead of
  being squashed or making the whole page wider.
- Outside a `.gx-frame`: no `width` or `min-width` over 360px, no `100vw`,
  and no `white-space: nowrap` on a sentence.
- Images shrink to the column by themselves; on the phone a tap opens them
  full size.

Ghostex fits a page that misses one of these (it stacks the columns, keeps
the pills whole and makes a too-wide block scroll sideways), but a page that
needs it reads worse. Check every page before you publish it:

```bash
ghostex visual check page.html   # screenshots at 390px (phone) and 1280px, and what doesn't fit
```

Look at both screenshots and fix everything it lists, including a "Pick
buttons" warning that a question sits under mockups with no Pick button tied
(see Question pages). `ghostex show` runs the phone check again after
publishing and lists anything left: fix it, run `ghostex show` again and reply
with the new block. If `visual check` can't
read an `.html` file, this Ghostex is older than the check: skip it.

### No page chrome of your own

Ghostex injects the page's look, so a page that styles itself fights it:

- No colours, backgrounds or fonts on `html`, `body` or `:root`, no
  `color-scheme`, no padding or margins on `body`, and no container or
  `max-width` wrapper of your own.
- No Dark / Light switch: Ghostex adds one beside the title. A page follows
  Ghostex's theme, and the reader flips the whole page with the switch.
- No toolbars, buttons, tabs, callouts or cards of your own: use these.
  - `<span class="gx-eyebrow">`, `<h1>`, `<p class="gx-lede">` in the
    `<header>`; `<h2>` for sections, `<h3>` for parts.
  - `<div class="gx-callout">` (add `data-tone="warn"`, `"good"` or `"bad"`).
  - `<div class="gx-card">`, several in a `<div class="gx-grid">`.
  - `<figure class="gx-figure">` with an `<img>` or a `.gx-frame`, and a
    `<figcaption>`.
  - `<gx-tabs>` with one child per tab: `<section data-tab="Before">…`. The
    kit adds the tab bar.
  - `<span class="gx-seg"><button aria-pressed="true">A</button><button>B</button></span>`
    for a segmented control in your own tool, `<button class="gx-button">`
    (add `gx-primary` for the main action).
  - Tables, lists, `<code>` and `<pre>` are styled already.

### Theme

Style what you draw with the theme tokens, so it reads right in dark and
light: `--gx-bg`, `--gx-surface` (cards, soft fills), `--gx-raised`
(controls), `--gx-line`, `--gx-line-strong`, `--gx-fg` (titles),
`--gx-text`, `--gx-muted`, `--gx-accent`, `--gx-warn`, `--gx-good`,
`--gx-bad`, `--gx-code-bg`, `--chart-1` … `--chart-6`, `--font-sans` and
`--font-mono`.

A mockup of the app has its own palette: write the dark one on the mockup's
class and the light one under `[data-gx-theme=light]`, so the reader's switch
flips the mockups with the page:

```css
.panel { --p-bg: #141414; --p-text: #e8e8e8; background: var(--p-bg); color: var(--p-text); }
[data-gx-theme=light] .panel { --p-bg: #f6f6f6; --p-text: #1d1d1d; }
```

### Images, sandbox, publishing

1. Libraries from an https CDN load fine. Local images are embedded
   automatically, by absolute path (`<img src="/Users/me/shot.png">`,
   `url(C:/shots/a.png)`) or, from Ghostex 10.23, by a path relative to the
   HTML file (`<img src="img/a.png">`, `srcset`, CSS `url(img/a.png)`). On
   older versions use absolute paths. `ghostex show` refuses a page whose
   local images are missing or not images and names them; fix those paths.
   The finished page, images included, can be up to 8 MiB (older versions
   also cap the HTML file itself at 512 KiB). Every other request must be
   https.
2. The page runs in a sandbox with no origin of its own: `localStorage`,
   cookies and requests to `http://` or `localhost` don't work. Keep state in
   memory. The one way back to you is the reader's answers (below).
3. Publish it:

   ```bash
   ghostex show page.html --title "Settings: where the extension catalog lives"
   ```

   If it says the page has no `ghostex-page` marker, you wrote it from an
   older copy of this skill: load the skill again and use the skeleton above.
4. Paste the ```visual block it prints into your reply exactly as printed.
   The chat shows it as a card for your page. Clicking the card opens the page
   where the reader chose under Settings > Chat > Open chat pages in: a
   floating window over the chat (the default), the Files view or their
   browser, and the floating window has its own Open in browser button. The
   reader decides, so publish every page the same way and leave the block's
   `"open": "popup"` mark in: some older Ghostex apps float only a marked card
   and open any other in the browser. Older Ghostex versions print the block
   without `"open"`; paste it as printed.

If `ghostex show` is an unknown command, say this Ghostex is too old to show
pages and give the HTML file's path instead.

### Question pages

Only for questions the reader can answer by seeing something: two layouts to
choose between, a mockup to react to, a diagram to mark up. Plain questions go
in a ```questions block (above). Write the page with the question kit every
page has. First run `ghostex show help`: if it doesn't mention
`gx-questions`, this Ghostex can't take answers from a page, so ask in the
chat instead.

A question page is the same skeleton (keep the marker) with
`<gx-questions>` in place of `<gx-page>`. Every variant carries `data-question`
and `data-choice` (first bullet below), which gives it a Pick button:

```html
<gx-questions submit="Send to the agent">
  <header>
    <span class="gx-eyebrow">Settings</span>
    <h1>Settings redesign</h1>
    <p class="gx-lede">Two layouts for the page, then the header.</p>
  </header>
  <gx-wide>
    <section data-question="Q1" data-choice="tabs"><h3>A. Tabs</h3><!-- mockup A --></section>
    <section data-question="Q1" data-choice="one page"><h3>B. One long page</h3><!-- mockup B --></section>
  </gx-wide>
  <gx-question tag="Q1" title="Which layout should I build?">
    <p>Why it matters, in a sentence or two.</p>
    <gx-choice value="tabs" recommended>A. Tabs<small>One page per area.</small></gx-choice>
    <gx-choice value="one page">B. One long page</gx-choice>
  </gx-question>
  <section data-question="Q2" data-choice="Compact"><h3>A. Compact</h3><!-- header mockup --></section>
  <section data-question="Q2" data-choice="Large title"><h3>B. Large title</h3><!-- header mockup --></section>
  <gx-question tag="Q2" title="Which header style?">
    <gx-choice>Compact</gx-choice>
    <gx-choice>Large title</gx-choice>
  </gx-question>
  <gx-question title="Anything else?" type="text"></gx-question>
</gx-questions>
```

- **Tie every variant to its choice, so each mockup has a Pick button at the
  right end of its title and the reader never has to scroll down to choose.**
  Wrap the variant's title and its mockup in one element with `data-question`
  (the question's `name`, title or tag) and `data-choice` (the choice's
  value, label or letter), for example
  `<section data-question="Q1" data-choice="tabs"><h3>A. Tabs</h3>…</section>`
  (as in the example above). The kit puts a Pick button by that title, shows
  the variant picked there and in the question (clicking either clears both).
  It adds no note box under the mockup (the reader's notes go in the
  question's own note box), so never tell the reader they can write a note
  under each mockup. Older Ghostex versions ignore the attributes; the
  question still works.
  - A question whose variants you did not tie still gets Pick buttons when the
    kit can read them: a heading above the question (after the previous
    question) that starts with a choice's letter (`A. Tabs`, `Option B:`) or
    is exactly a choice's label gets that choice's Pick button. Never rely on
    it: a heading that matches nothing, or two choices, or the same choice as
    another heading, gets no button, because a wrong Pick is worse than none.
  - When a question sits under mockups and no Pick could be tied,
    `ghostex visual check` and `ghostex show` say so, naming the question
    (a warning; the page still works). Add the attributes and check again.
- Put the whole page in one `<gx-questions>`. It lays out like `<gx-page>`:
  the column, with `<gx-wide>` only for mockups wider than 720px (the
  wide Settings mockups above; the small header mockups stay in the column).
  Never put a mockup inside a question.
- Each question sits directly below the visual it asks about: a mockup, then
  its question, then the next mockup and its question, so the reader answers
  while the picture is still on screen instead of scrolling back up. When one
  question compares several variants, put them side by side (or stacked)
  immediately above that question, with nothing in between, and keep them
  compact enough to be seen together with it. Side-by-side variants go in a
  `.gx-grid` (inside `<gx-wide>` when they are wide), so they stack on a
  phone. Draw each variant once, in the
  page's theme; the reader checks the other theme with the switch. Never
  collect all the mockups first and all the questions at the end.
- `<gx-question>`: `title`; `tag`, an optional short label such as an issue
  id; `name`, the answer's key (the title when there is none); `multiple`
  for several picks; `type="text"` for a text box instead of choices. Content
  before its choices, such as a `<p>`, explains the question.
- `<gx-choice>`: its text is the choice and a `<small>` inside is a one-line
  detail. `value` is what you receive (the text without the `<small>` when
  there is none). `recommended` adds a Recommended badge.
- List a question's choices in exactly the order its variants appear above
  it: the first variant on the page is the first choice, and a variant
  lettered "A." is choice a. Never move the recommended choice to the top;
  mark it with `recommended` where it already sits. A question whose choices
  run C, B, A under mockups A, B, C makes the reader pick the wrong one.
- Write only that markup. The kit letters the choices, ends every choice
  question with "Other / none of these" and a note box, and adds the send
  button. Nothing is ever preselected (the reader can also click a pick again
  to clear it), and a question left blank arrives as `"skipped"` (`null` in older
  Ghostex versions). Don't write your
  own form, button, "Other" choice or note box. Newer Ghostex versions also add
  a Skip all button; a skipped question arrives as `"skipped"`.
- Once sent, the page is locked; opening it again shows the answers and an
  Answered note.
- Paste the block exactly as `ghostex show` printed it: for a question page it
  lists the page's tags (`"questions": ["Q1", "Q2"]`), which is how the chat
  counts the page as unanswered and links its tags.
- After pasting the card, end your turn. Don't poll or wait in a loop: the
  answers arrive as the next user message, which the chat shows as an answers
  card while the page's card reads Answered. A note arrives under the
  question's key plus ` (note)`:

  ```text
  <ghostex-page-answers page="7ea5c5be1f2d68ebc9e40f790db0ef29" title="Settings redesign">
  {
    "Which layout should I build?": "tabs",
    "Which layout should I build? (note)": "keep search at the top",
    "Which platforms first?": ["macOS", "Other / none of these"],
    "Which platforms first? (note)": "Linux too",
    "Anything else?": null
  }
  </ghostex-page-answers>
  ```

For an answer the kit can't ask for, such as a ranking or a spot on a
mockup, a page can send its own: `ghostex.submit(answers)` posts any JSON
under 64 KiB and returns a promise that rejects with an `Error` saying why it
wasn't sent. While Ghostex can't be reached (a dropped phone link, a restart)
newer versions keep trying instead of rejecting and call an optional second
argument, `ghostex.submit(answers, onRetry)`, with a message to show each
time; `ghostex.answered` and `ghostex.answers` tell a reopened page what
was sent; an element with `data-gx-answered` is where the Answered note goes.
A page takes one set of answers either way.

What the reader picks and types is kept until they send, also when they close
and reopen the page: the kit's picks and the value of every named `input`,
`textarea` and `select` are saved and filled back in on their own. For other
state (a ranking, marks on a mockup), call `ghostex.saveDraft(value)` with any
JSON as it changes and read it back from `ghostex.draft` when the page loads;
check `ghostex.saveDraft` exists first, since older Ghostex versions lack it.
