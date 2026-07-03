# Clip Extensions

Clip extensions let a third-party clip tool push your selected text into TextGO,
which then runs your bound rules on it. This is useful in apps where TextGO's
native selection capture is blocked or unreliable, or if you already use a clip
tool and want TextGO as one of its actions.

It is **off by default** and reachable only by local processes running as you.

Currently supported: **SnipDo** (Windows). PopClip (macOS) parity is planned.

## How it works

When enabled, TextGO runs a local, owner-only listener. The clip tool's TextGO
action sends the selected text to that listener, and TextGO re-emits it as the
**Clip Extension** trigger — the same pipeline used by keyboard and mouse
triggers. On Windows the listener is a named pipe (`\\.\pipe\textgo`) protected
by an owner-only security descriptor, so a web page cannot reach it.

## Setup (Windows / SnipDo)

1. In TextGO, open **Settings → General → Behavior & Permissions** and turn on
   **Clip Extension Ingest**.
2. On the **Shortcuts** page, click **Register Shortcut → Clip Extension**, then
   bind one or more rules to it (a text type → an action), just like any other
   trigger.
3. Install the SnipDo package: download `TextGO.pbar` from the
   [releases page](https://github.com/C5H12O5/TextGO/releases) and open it with
   SnipDo (or drag it into SnipDo's package list).
4. Select text in any app, open the SnipDo bar, and click **TextGO**. The text
   flows into TextGO and your bound rule runs.

If TextGO isn't running, the SnipDo action tries to launch it via the `textgo://`
URL and retries once.

## Security

- The listener never binds unless you enable it; disabling drops it.
- It is reachable only by processes running as your user — a browser `fetch()`
  cannot open the named pipe.
- External text only ever triggers rules **you** bound to Clip Extension. If
  nothing is bound, nothing runs.
- Residual risk: while enabled, any process running as you can also send text to
  TextGO. Leave it off when you don't need it.

## Package source

The SnipDo package lives in `clip-extensions/snipdo/` (`textgo.json`,
`textgo.ps1`, `icon.png`) and is zipped into `TextGO.pbar` on release.
