# zorite

The Markdown editor crates the native Docs view (`apps/desktop/src/app/native_docs/`) is built on:
`zorite-editor`, `zorite-markdown`, `gpui-bidi` and `ratex-gpui`, all MIT, from
[packetThrower/zorite](https://github.com/packetThrower/zorite). Each crate keeps its own LICENSE.

They are vendored rather than depended on from git because they carry small Docs changes, each
marked `Local change` in the code (selection access, the Docs heading scale and colours, the 1.7
line height, find colours, table-cell slack, note highlights, and cached line starts, without
which a long document took about half a second per frame). Upstream compiles against
Ghostex's GPUI after dropping the extra clip-bounds argument from two `paint_image` calls.

The Zorite application is GPL and none of its code is here.
