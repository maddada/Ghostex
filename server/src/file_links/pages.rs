//! The two pages a link shows instead of the raw file: rendered Markdown and the Excalidraw editor.

use std::{fs, path::Path};

const EXCALIDRAW_VERSION: &str = "0.18.1";
const REACT_VERSION: &str = "19.2.4";

fn file_name(file: &Path) -> String {
    file.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// GitHub-flavoured Markdown with the file's own HTML kept, front matter hidden, and Mermaid
/// blocks drawn in the browser. Relative links and images resolve beside the file.
pub(super) fn markdown(file: &Path) -> Option<String> {
    let source = fs::read_to_string(file).ok()?;
    let mut options = markdown::Options::gfm();
    options.parse.constructs.frontmatter = true;
    options.compile.allow_dangerous_html = true;
    options.compile.allow_dangerous_protocol = true;
    let body = markdown::to_html_with_options(&source, &options).ok()?;
    Some(format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>{MARKDOWN_STYLE}</style>
</head>
<body>
<main class="markdown-body">
{body}
</main>
<script type="module">
const blocks = [...document.querySelectorAll('pre > code.language-mermaid')];
if (blocks.length) {{
  const {{ default: mermaid }} = await import('https://esm.sh/mermaid@11');
  const dark = matchMedia('(prefers-color-scheme: dark)').matches;
  mermaid.initialize({{ startOnLoad: false, theme: dark ? 'dark' : 'default' }});
  for (const code of blocks) {{
    const host = document.createElement('div');
    host.className = 'mermaid';
    host.textContent = code.textContent;
    code.parentElement.replaceWith(host);
  }}
  await mermaid.run({{ querySelector: '.mermaid' }});
}}
</script>
</body>
</html>
"#,
        title = escape(&file_name(file)),
    ))
}

/// The same Excalidraw release the Files view runs, loaded from esm.sh. It reads the drawing
/// with `?raw=1` and saves it back with `PUT` one second after the last change, like the Files
/// view's own autosave.
///
/// CDXC:Docs 2026-11-14 WHY:
/// Excalidraw shows X/Twitter, Reddit and GitHub Gist links as a `srcdoc` frame that runs those
/// sites' widget scripts with this page's own origin, which is gxserver's link origin that can read
/// and save the link's files. Those links stay link cards, as in the Files view's editor
/// (apps/desktop/views/manage/preview/excalidraw-editor.tsx `validateManageExcalidrawEmbed`);
/// YouTube, Vimeo, Figma and the other embeds Excalidraw allows load from their own sites as before
/// (`undefined` keeps Excalidraw's own list).
pub(super) fn drawing(file: &Path) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<link rel="stylesheet" href="https://esm.sh/@excalidraw/excalidraw@{EXCALIDRAW_VERSION}/dist/prod/index.css">
<style>html, body, #root {{ height: 100%; margin: 0; }} body {{ background: #121212; color: #ddd; font: 13px system-ui, sans-serif; }} .message {{ padding: 24px; }}</style>
<script>window.EXCALIDRAW_ASSET_PATH = "https://esm.sh/@excalidraw/excalidraw@{EXCALIDRAW_VERSION}/dist/prod/";</script>
<script type="importmap">{{"imports": {{
  "react": "https://esm.sh/react@{REACT_VERSION}",
  "react/": "https://esm.sh/react@{REACT_VERSION}/",
  "react-dom": "https://esm.sh/react-dom@{REACT_VERSION}",
  "react-dom/": "https://esm.sh/react-dom@{REACT_VERSION}/"
}}}}</script>
</head>
<body>
<div id="root"></div>
<script type="module">
import {{ createElement }} from 'react';
import {{ createRoot }} from 'react-dom/client';
import {{ Excalidraw }} from 'https://esm.sh/@excalidraw/excalidraw@{EXCALIDRAW_VERSION}?external=react,react-dom';

const root = document.getElementById('root');
const fail = (text) => {{ root.innerHTML = ''; const p = document.createElement('p'); p.className = 'message'; p.textContent = text; root.append(p); }};
const response = await fetch(location.pathname + '?raw=1', {{ cache: 'no-store' }});
if (!response.ok) {{ fail('This drawing is no longer available. Open it again from Ghostex.'); throw new Error('unavailable'); }}
const text = (await response.text()).trim();
let data;
try {{ data = text ? JSON.parse(text) : {{}}; }} catch {{ fail('This drawing is not valid Excalidraw JSON.'); throw new Error('invalid'); }}
const previous = data.appState && typeof data.appState === 'object' ? data.appState : {{}};
const serialize = (elements, appState, files) => {{
  const saved = {{ ...previous, scrollX: appState.scrollX, scrollY: appState.scrollY, theme: appState.theme, viewBackgroundColor: appState.viewBackgroundColor, zoom: {{ value: appState.zoom?.value ?? 1 }} }};
  delete saved.collaborators;
  return JSON.stringify({{ appState: saved, elements, files, source: data.source ?? 'https://excalidraw.com', type: 'excalidraw', version: data.version ?? 2 }});
}};
const writtenIntoPage = ['twitter.com', 'x.com', 'reddit.com', 'gist.github.com'];
const validateEmbeddable = (link) => {{
  let host;
  try {{ host = new URL(String(link).trim()).hostname.toLowerCase(); }} catch {{ return undefined; }}
  return writtenIntoPage.some((site) => host === site || host.endsWith('.' + site)) ? false : undefined;
}};
let baseline;
let timer;
const save = (content) => {{
  clearTimeout(timer);
  timer = setTimeout(() => {{
    fetch(location.pathname, {{ method: 'PUT', body: content, headers: {{ 'content-type': 'application/json' }} }})
      .then((answer) => {{ if (!answer.ok) fail('Saving failed. Open the drawing again from Ghostex.'); }});
  }}, 1000);
}};
createRoot(root).render(createElement(Excalidraw, {{
  validateEmbeddable,
  initialData: {{
    appState: {{ viewBackgroundColor: '#ffffff', theme: 'dark', ...previous, collaborators: new Map() }},
    elements: Array.isArray(data.elements) ? data.elements : [],
    files: data.files && typeof data.files === 'object' ? data.files : {{}},
  }},
  onChange: (elements, appState, files) => {{
    const content = serialize(elements, appState, files);
    // Excalidraw reports the loaded scene once; that is the baseline, not an edit.
    if (baseline === undefined) {{ baseline = content; return; }}
    if (content === baseline) return;
    baseline = content;
    save(content);
  }},
}}));
</script>
</body>
</html>
"#,
        title = escape(&file_name(file)),
    )
}

const MARKDOWN_STYLE: &str = r#"
:root { color-scheme: light dark; --text: #1f2328; --muted: #59636e; --border: #d1d9e0; --code: #f6f8fa; --link: #0969da; --page: #ffffff; }
@media (prefers-color-scheme: dark) { :root { --text: #e6e6e6; --muted: #9a9a9a; --border: #2e2e2e; --code: #1d1d1d; --link: #6ea8fe; --page: #121212; } }
html { background: var(--page); }
body { margin: 0; color: var(--text); font: 16px/1.6 -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif; }
.markdown-body { box-sizing: border-box; max-width: 860px; margin: 0 auto; padding: 40px 32px 96px; }
h1, h2, h3, h4, h5, h6 { line-height: 1.25; margin: 1.6em 0 0.6em; }
h1 { font-size: 2em; padding-bottom: 0.3em; border-bottom: 1px solid var(--border); }
h2 { font-size: 1.5em; padding-bottom: 0.3em; border-bottom: 1px solid var(--border); }
a { color: var(--link); }
img, video { max-width: 100%; }
code { font: 0.875em ui-monospace, SFMono-Regular, Menlo, monospace; background: var(--code); padding: 0.15em 0.35em; border-radius: 5px; }
pre { background: var(--code); padding: 14px 16px; border-radius: 8px; overflow: auto; }
pre code { background: none; padding: 0; }
blockquote { margin: 0; padding: 0 1em; color: var(--muted); border-left: 3px solid var(--border); }
table { border-collapse: collapse; display: block; overflow: auto; }
th, td { border: 1px solid var(--border); padding: 6px 12px; }
hr { border: 0; border-top: 1px solid var(--border); margin: 2em 0; }
li > input[type="checkbox"] { margin-right: 0.4em; }
.mermaid { display: flex; justify-content: center; margin: 1em 0; }
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_drawing_page_keeps_sites_written_into_the_page_as_link_cards() {
        let page = drawing(Path::new("/project/plan.excalidraw"));
        assert!(
            page.contains("validateEmbeddable,"),
            "the editor gets the rule"
        );
        for site in [
            "'twitter.com'",
            "'x.com'",
            "'reddit.com'",
            "'gist.github.com'",
        ] {
            assert!(page.contains(site), "{site} stays a link card");
        }
        // The page's braces come out single: it is valid JavaScript, not the format string.
        assert!(!page.contains("{{"), "doubled braces left in the page");
    }
}
