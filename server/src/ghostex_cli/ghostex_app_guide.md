## Ghostex Apps: tools the user keeps

When the user asks for an app, a dashboard, a tracker, a timer or a
presentation they will come back to, build a **Ghostex App**: one
self-contained HTML file published with `ghostex app show app.html`. It opens
like a Live page, keeps its own data, and the user can install it as its own
app with its own icon. Use a Live page (`ghostex show`) instead for one-off
reports, mockups and questions.

Ghostex Apps is a Beta extension; these commands answer only while the user
has it on in Settings > Extensions.

### Skeleton

Start from the Live page skeleton and layout (`ghostex visual guide pages`:
the v2 marker, `<gx-page>`, the theme variables, phones and large text), and
add the app's own head tags:

```html
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="ghostex-page" content="v2">
<title>Morning brief</title>
<link rel="icon" href="icon.svg">
<meta name="ghostex-app-permissions" content="send">
<style>/* the app's own look */</style>
</head>
<body>
<gx-page>…</gx-page>
<script>/* the app */</script>
</body>
</html>
```

- `<title>` is the app's name in the Dock, the taskbar and Settings: short and
  specific ("Morning brief", "Release board"), never "App" or "Dashboard".
- No `<gx-questions>` and no `ghostex.submit`: an app talks back with
  `ghostex.app.send` (below).
- **Updates: always publish the same file again.** The app follows its file:
  `ghostex app show` on the same path makes the next version of the same app,
  and the open app moves to it. A copy under another name is a new app.

### Icon

Every app needs an icon you draw: a square SVG beside the HTML, named in the
head with `<link rel="icon" href="icon.svg">` (a square PNG of at least
256 px works too). `ghostex app show` refuses an app without one.

- A rounded square filling the whole canvas (`viewBox="0 0 512 512"`, corner
  radius about 23%), in the app's own colours, with one bold symbol that says
  what the app is (a sun for a morning brief, a plane for flight watch). No
  words or letters, nothing thin: it must read at 16 px.
- Shapes, paths, gradients only: no scripts, event attributes, embedded or
  linked images, `<use>`, or outside addresses. At most 64 KiB.

### What the app can do: `window.ghostex.app`

Every call returns a Promise. `ghostex.app.available` is false when the app was
opened outside Ghostex (a plain link, a browser); then every call rejects, so
keep the app usable without them.

- `ghostex.app.data.get(key)`, `.set(key, value)`, `.remove(key)`,
  `.keys()`, `.all()`: the app's own saved data, any JSON value per key, kept
  across restarts and updates. At most 1 MiB and 256 keys. The page itself has
  no `localStorage` (it is sandboxed), so save here. **Never store secrets**
  (passwords, API keys, tokens) in it.
- `ghostex.app.send(text)`: sends a message to the chat that made the app (up
  to 8 KiB, at most 20 per 10 minutes), for "log this", "add a meeting",
  "refresh the numbers". Needs the `send` permission.
- `ghostex.app.permission(name)`: `"allowed"`, `"denied"`, `"ask"` (the user
  has not decided yet) or `"undeclared"`.

Powers are asked, never assumed. Name every power the app may use in
`<meta name="ghostex-app-permissions" content="…">` (space separated:
`send`, `notifications`, `ontop`, `menubar`, `shortcut`, `connectors`); one
not named there is always refused. The user decides each one for the app;
until then a call rejects with `error.permission === "ask"` and Ghostex asks
the user, so show a short line such as "Allow sending in Ghostex to log this"
and let the user try again. A refused call has `error.permission === "denied"`:
say what is off and keep working without it. Only declare what the app really
uses; an update that adds a power asks the user again for that one.

`notifications`, `ontop`, `menubar`, `shortcut` and `connectors` are not
available yet in this Beta; declare them only when the user asked for that.

### Messages from the app

What the app sends reaches you as a message from the user:

```
<ghostex-app-message app="<app id>" title="Morning brief">
Log this dose
</ghostex-app-message>
```

Treat it as the user's request made through the app. When it asks you to
change the app, edit the same file and run `ghostex app show` on it again.

### Commands

- `ghostex app show <app.html> [--title <name>] [--json]`: publishes the app
  (or its next version) and prints the block to put in your reply, where the
  chat shows it as a card that opens the app.
- `ghostex app list [--json]`: the apps on this computer, their versions and
  what each may do.
- `ghostex app guide`: this guide.
