# zorite

The Markdown editor crates the native Docs view (`apps/desktop/src/app/native_docs/`) is built on:
`zorite-editor`, `zorite-markdown`, `gpui-bidi` and `ratex-gpui`, all MIT, from
[packetThrower/zorite](https://github.com/packetThrower/zorite). Each crate keeps its own LICENSE.

They are vendored rather than depended on from git because they carry small Docs changes, each
marked `Local change` in the code (selection access, the Docs heading scale and colours, the 1.7
line height, find colours, table-cell slack, note highlights, and cached line starts, without
which a long document took about half a second per frame). Upstream compiles against
Ghostex's GPUI after dropping the extra clip-bounds argument from two `paint_image` calls.

The Files view's parity pass with the Docs page it replaced (2026-09-28) added more, also marked
`Local change`: frontmatter, merge-conflict and `<details>` blocks (`local_block_line` in
`element.rs`, with room above a conflict for the host's buttons); numbered footnotes that jump
between reference and definition; `<kbd>` keycaps, `:shortcode:` emoji (`emoji.rs`), hidden
autolink brackets, underlined links that open on a click, or on Cmd/Ctrl-click while the caret is in one
(`revealed_links_need_modifier`), and
empty-text image links; `[~]`/`[-]` task states and the Docs checkbox; the Docs heading scale,
quote bar, uppercase alert labels with a tinted body, and code cards whose fence rows carry the
language and copy chips; a host view per Mermaid block (`set_mermaid_view_provider`) and
`:::mermaid` blocks; a heading-fold API for a host-drawn fold lane (`heading_folds`); switches
for the block-drag grip and the hover chevrons; and table controls (`caret_table`,
`table_header_cells`, `sort_table`, TSV copy and cell clearing of a cell-range selection).

The Zorite application is GPL and none of its code is here.
