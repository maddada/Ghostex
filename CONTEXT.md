# Ghostex

Ghostex is a native macOS workarea for project, session, terminal, browser, editor, and Project Board workflows backed by gxserver for shared state.

## Language

**Native Presentation Projection**:
The pure local projection from gxserver presentation and already-projected macOS-local pane facts into final sidebar groups, including normal project groups and Quick/Chats. It owns combined sidebar group/session ids. Local-first hidden projects and sessions enter as data sets; remote gxserver presentation stays outside this module. The projection does not mutate gxserver snapshots, local-first overlays, pane chrome, or publish state.
_Avoid_: presentation sync, sidebar bridge, gxserver reducer
