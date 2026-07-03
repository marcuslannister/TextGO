# Clip Extensions — Design Doc

- **Status:** Reviewed — decisions locked, ready to implement (Windows-first)
- **Date:** 2026-07-02
- **Author:** marcuslannister
- **Scope:** TextGO only. No changes to any other project.
- **Decision:** Option A, **named pipe owner-ACL (Windows) / unix socket 0600 (macOS)**,
  off by default. Windows ships first (Phase 1); macOS parity is an independent Phase 2.
  See §5, §6, §10.

---

## 1. Problem & context

Some users select text inside apps or contexts where TextGO's native global
capture (rdev + Enigo + clipboard) is unreliable or blocked, and some users
already live inside a third-party "clip" tool (PopClip on macOS, SnipDo on
Windows) and want TextGO as one more action in that tool's selection popup.

The reference for this pattern is nextai-translator's `CLIP-EXTENSIONS.md`.
There, the app has **no** native selection capture, so PopClip/SnipDo are its
*only* reliable text source. Its mechanism:

- **PopClip (macOS):** `Config.plist` + `nextai-translator.sh` →
  `curl -d "$POPCLIP_TEXT" --unix-socket /tmp/openai-translator.sock`.
  If the app isn't running: `open -g -a`, then retry. Shipped as `.popclipextz` (zip).
- **SnipDo (Windows):** `nextai-translator.json` + `.ps1` →
  `curl 127.0.0.1:62007 -Method POST -Body <utf8 text>`. Shipped as `.pbar` (zip).

i.e. the app runs a small **local ingest listener** that receives POSTed
selection text, plus **shipped tool packages** users install into PopClip/SnipDo.

This doc proposes bringing the same capability to TextGO.

## 2. Goals / non-goals

**Goals**

- Let PopClip (macOS) and SnipDo (Windows) push selected text into TextGO.
- Route that text through TextGO's **existing** match → execute pipeline so all
  current action types (builtin / script / prompt / searcher), toolbar, and popup
  work with zero downstream changes.
- Ship installable PopClip/SnipDo packages from TextGO's own releases.
- Opt-in and safe by default.

**Non-goals**

- No changes to the existing single-item JSON "extension" system
  (`dumpExtension`, `/extensions` gallery) — see §3, these are different concepts.
- No Linux support (TextGO is macOS/Windows only).
- No browser extension.
- No auth model beyond a local trust boundary + off-by-default (see §7).

## 3. Suitability analysis

### 3.1 "Extension" means two different things here

TextGO already uses the word **extension** for single-item JSON action
definitions (`dumpExtension` in `helpers.ts`; export/import/install on each
settings page; the `/extensions` web gallery). That system **defines actions**.
Clip extensions **ingest external text**. They share only the word.

> **Decision:** Do **not** shoehorn clip ingest into the JSON action-extension
> system. It is the wrong layer.

### 3.2 The right vehicle already exists: the pseudo-shortcut pipeline

Rust already emits a `shortcut` event and the frontend already dispatches it:

```
Rust  app.emit("shortcut", { shortcut, selection })
  → shortcut.ts: handleShortcutEvent(shortcut, selection)
  → matcher.ts:  matchAll / matchOne(selection, rules)
  → executor.ts: execute(rule) → toolbar / popup / replace
```

TextGO already treats non-key triggers as shortcut strings
(`MouseClick+MouseMove`, `MouseClick+MouseClick`, `LongPress` in `constants.ts`).
Clip ingest becomes **one more pseudo-shortcut** (`ClipExtension`): the listener
emits the same `shortcut` event with the received text, and everything
downstream is unchanged. Users bind rules to `ClipExtension` exactly like any
other trigger.

**Shared injection point (identical for every transport option):**

```rust
// mirrors the existing pattern in handlers/keyboard.rs:30-38
tauri::async_runtime::spawn(async move {
    let event_data = serde_json::json!({
        "shortcut": "ClipExtension",
        "selection": text
    });
    let _ = app_handle.emit("shortcut", event_data);
});
```

> **Note (see §6):** `ClipExtension` is a *pseudo*-shortcut, not an OS accelerator.
> It must be carved out of the registrar path the same way the mouse pseudo-shortcuts
> are, or binding a clip rule throws. This is a required implementation detail the
> earlier draft missed.

### 3.3 Premise-collapse risk (state it plainly)

**This plan assumes there is real value in an external clip-tool trigger given
TextGO already has native selection capture.** nextai *needed* clip tools; TextGO
mostly does not. If that assumption is weak, the feature is low-value.

Where it still pays off: (a) apps/contexts where TextGO's global hook is
blocked/unreliable; (b) users already in PopClip/SnipDo. Verdict: worth building,
but **scoped and gated** — not a headline feature. **Resolved at review: build it,
Windows-first** (§10.4).

### 3.4 Security surface (load-bearing constraint)

nextai's socket only translates. **TextGO's pipeline can run
`execute_shell` / `execute_python` / etc.** So a clip ingest that reaches the
pipeline is a path from "external input" to "script execution with
attacker-chosen text." This is the constraint that decides the transport and the
gating (see §5, §7).

## 4. Options

Both options end at the same §3.2 injection point. They differ only in **how
external text reaches the emit**.

### Option A — local ingest listener (unix socket / named pipe) — RECOMMENDED

```
PopClip (.sh)                             SnipDo (.ps1)
   │  curl --unix-socket /tmp/textgo.sock    │  NamedPipeClientStream '.','textgo','Out'
   │                            (macOS)       │  write text; flush; close      (Windows)
   ▼                                          ▼
TextGO Rust — NEW module handlers/clip.rs (spawned in setup, gated by setting)
   #[cfg(unix)]    tokio UnixListener  @ /tmp/textgo.sock   (0600 perms)
   #[cfg(windows)] tokio named_pipe    @ \\.\pipe\textgo    (owner-only ACL)
   accept → read body → text
   ▼
   app.emit("shortcut", { shortcut:"ClipExtension", selection:text })
   ▼
(unchanged frontend pipeline)
```

App-not-running: PopClip/SnipDo script does a launch + retry
(`open -g -a TextGO; sleep 2; retry` on macOS; equivalent on Windows) — nextai's pattern.

Illustrative Rust (not final):

```rust
// handlers/clip.rs  — ~120–160 LOC
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        #[cfg(unix)]    accept_unix(app, "/tmp/textgo.sock").await;  // UnixListener, chmod 0600
        #[cfg(windows)] accept_pipe(app, r"\\.\pipe\textgo").await;  // owner-only SD (below)
    });
}

// Windows: named pipe with a PROTECTED owner-only security descriptor.
//   SDDL "D:P(A;;GA;;;OW)" → generic-all to the pipe's owner (= the user running
//   TextGO); everyone else denied. Built via
//   ConvertStringSecurityDescriptorToSecurityDescriptorW (windows crate,
//   Win32_Security features) and passed to tokio
//   named_pipe::ServerOptions::create_with_security_attributes_raw.
//   One-instance-ahead accept loop (a named-pipe server needs a fresh instance
//   per client). Each connection: read body to EOF → text → emit the received
//   text directly (do NOT re-call get_selection). A browser fetch() cannot open
//   a named pipe, so there is no web-reachable path — contrast the TCP note in
//   §4.1 and Option B.

pub fn stop() { /* drop listener; unix: unlink socket file */ }

// commands/clip.rs — enable/disable, registered in generate_handler!
#[tauri::command] pub fn set_clip_extension_enabled(app: AppHandle, enabled: bool) { /* .. */ }
```

**Cost:** ~120–160 new Rust LOC · 1 new module · **2 Cargo changes**
(`tokio` needs the `net` feature — currently only `process, rt-multi-thread`; the
already-present `windows` dep needs `Win32_Security` + `Win32_Security_Authorization`
for the security descriptor) · a lifecycle command + `generate_handler!` entry.
No new crate.

### Option B — reuse the existing `textgo://` deep-link

TextGO already registers scheme `textgo`, has single-instance forwarding
(`deep-link` feature on), and a live `on_open_url` handler at `lib.rs:343`.
B adds a branch to it.

```
PopClip (.sh) / SnipDo (.ps1)
   │  open "textgo://clip?text=<percent-encoded>"          (macOS)
   │  Start-Process "textgo://clip?text=<percent-encoded>" (Windows)
   ▼
OS URL dispatch → single-instance forwards to running TextGO (or launches it)
   ▼
TextGO Rust — EXTEND existing on_open_url (lib.rs:343-350)
   detect "/clip" → percent-decode text
   ▼
   app.emit("shortcut", { shortcut:"ClipExtension", selection:text })
   ▼
(unchanged frontend pipeline)
```

Illustrative Rust (the whole change is inside the existing handler):

```rust
app.deep_link().on_open_url(move |event| {
    if let Some(url) = event.urls().first() {
        if url.host_str() == Some("clip") {                                   // NEW
            if let Some(text) = url.query_pairs().find(|(k,_)| k=="text").map(|(_,v)| v.into_owned()) {
                let _ = app_handle.emit("shortcut",
                    json!({ "shortcut":"ClipExtension", "selection": text })); // NEW ~6 lines
                return;
            }
        }
        let url = url.as_str().strip_prefix("textgo:/").unwrap_or(url.as_str());
        navigate_to(app_handle.clone(), url.to_string());                     // unchanged
    }
});
```

**Cost:** ~15–30 changed Rust LOC · no new module · no new Cargo dep · no
listener lifecycle, no port, no socket file. **Rejected** — see §5.

### 4.1 Head-to-head

| Dimension | A — unix socket / named pipe | B — deep-link |
|---|---|---|
| New/changed Rust | ~120–160 LOC, new module | ~15–30 LOC, in-place |
| New Cargo change | `tokio` `net` + `windows` `Win32_Security*` (existing dep) | none |
| Reuses existing infra | emit only | emit + scheme + single-instance + `on_open_url` |
| Lifecycle to manage | listener start/stop, socket cleanup | none |
| **Large / multiline text** | ✅ raw body, unlimited | ⚠️ URL length + percent-encoding limits |
| **Reachability (security)** | ✅ OS-enforced owner-only — unix socket 0600 (macOS) and named-pipe owner ACL (Windows) are both unreachable by a browser `fetch()` | ❌ web-reachable (`location='textgo://clip?text=…'`) |
| Safe for a script-exec pipeline | yes: off-by-default + owner-only boundary | hazard: a webpage could feed text to a bound script action |
| App-not-running | script launches + retries | OS auto-launches via scheme |
| Parity with the reference doc | macOS exact; Windows hardened (nextai used tokenless TCP — unsafe here, §5) | different mechanism |
| Hardening needed | off-by-default; owner-only socket/pipe (no token needed) | needs a per-install token in the URL to block web invocation |

## 5. Recommendation: Option A — unix socket 0600 (macOS) / named pipe owner-ACL (Windows), off by default

B is ~20 lines and reuses everything, but two things make it the wrong default
**for TextGO specifically**:

1. **Web-reachability + script execution.** A deep-link is invokable by any web
   page, so enabling clip-via-deep-link lets a malicious site push chosen text
   into whatever action the user bound to `ClipExtension`. Option A's listeners
   are reachable only by same-user local processes — the correct trust boundary
   for a script-capable pipeline.
2. **Large selections.** Clip tools routinely send whole paragraphs;
   percent-encoded URLs get fragile/truncated, a POST body does not.

**Transport within A — why NOT loopback TCP on Windows (correcting the earlier draft).**
nextai's Windows path is tokenless `curl 127.0.0.1:62007`. That is safe for nextai
because its socket only translates. **It is not safe for TextGO**, because the same
pipeline can run `execute_shell` / `execute_python`, and a loopback TCP port has a
web-reachable *side effect*: a page in the user's browser can
`fetch('http://127.0.0.1:<port>', { method: 'POST', body: selection })` — CORS blocks the
page from reading the *response*, but the request body is still delivered, the listener
emits it, and the user's bound action runs. So Windows uses a **named pipe with a
protected owner-only ACL** instead: a browser `fetch()` cannot open a named pipe at all,
and the OS enforces the same-user boundary — the true equivalent of the macOS 0600 socket,
with no shared secret to leak. (A per-install token would also block the web path, but it
is a disk secret; the pipe ACL is stronger and needs no token. Token stays optional future
hardening only, §7.)

## 6. Plan (Option A) — two independently mergeable phases

**Priority: Windows first (Phase 1).** macOS parity (Phase 2) can land later; if it never
does, Phase 1 still works end-to-end. Each phase leaves the app in a usable state.

### Phase 1 — Windows

**Backend (`src-tauri/`)**

- `handlers/clip.rs` (new): `#[cfg(windows)]` named-pipe server at `\\.\pipe\textgo`,
  created with a protected owner-only security descriptor (SDDL `D:P(A;;GA;;;OW)`, built via
  `ConvertStringSecurityDescriptorToSecurityDescriptorW` and passed to tokio
  `named_pipe::ServerOptions::create_with_security_attributes_raw`). One-instance-ahead
  accept loop; read body to EOF → emit `shortcut` with `ClipExtension`, emitting the received
  text directly (do NOT re-call `get_selection`). Graceful stop.
- `commands/clip.rs` (new): `set_clip_extension_enabled(enabled)`; re-export via
  `commands.rs`; register in `lib.rs` `generate_handler!`.
- `lib.rs`: start the listener in `setup_app` when the persisted setting is on.
- `Cargo.toml`: add `net` to `tokio` features; add `Win32_Security` +
  `Win32_Security_Authorization` to the existing `windows` dep. No new crate.

**Frontend (`src/`)**

- `constants.ts`: `export const CLIP_SHORTCUT = 'ClipExtension';`
- **[REQUIRED] `shortcut.ts` registrar carve-out.** `ClipExtension` is not a mouse
  shortcut, so today `register()` would pass it to `register_shortcut` (`shortcut.ts:148`) /
  `unregister_shortcut` (`:184`), which the global-shortcut plugin parses as an OS
  accelerator and rejects → binding a clip rule throws. Add
  `isPseudoShortcut(s) = isMouseShortcut(s) || s === CLIP_SHORTCUT` and use it at **both**
  registrar guards. **Do not** make `isMouseShortcut('ClipExtension')` return true — that
  would wrongly trigger the empty-selection `get_selection` refetch (`:99-105`) and
  mouse-cursor toolbar positioning.
- `helpers.ts`: add a `formatShortcut` case for `ClipExtension`; do NOT extend
  `isMouseShortcut`.
- `stores.svelte.ts`: `clipExtensionEnabled` persisted, default **false**,
  `onchange` → `invoke('set_clip_extension_enabled', { enabled })`.
- Shortcuts UI: surface `ClipExtension` as a bindable trigger (alongside the mouse
  pseudo-shortcuts).
- Settings toggle on the **general** page (NOT the "Extensions" nav — that name is the
  JSON action-extension system, §3.1) with a clear "any local app running as you can send
  text to TextGO while this is on" warning.
- i18n: add keys to `messages/{en,zh-CN}.json` (both locales).

**Package (`clip-extensions/snipdo/` in the TextGO repo)**

- `textgo.json`, `textgo.ps1` (.NET `NamedPipeClientStream '.','textgo','Out'` → write the
  selection → flush → close), `icon.png`. Identifier
  `top.xylitol.textgo.clip-extensions.snipdo`.
- **Confirm when authoring:** SnipDo's exact text-passing mechanism (env var vs stdin vs a
  `{text}` placeholder in the `.pbar` config). The transport is agnostic to it.

**CI / distribution**

- **[REQUIRED]** `.github/workflows/release.yml` today runs ONLY `tauri-apps/tauri-action@v0`
  (`release.yml:74`), which uploads Tauri bundles, not arbitrary zips. Add an explicit step to
  zip the SnipDo package into `.pbar` and attach it (`gh release upload`, or
  `softprops/action-gh-release` with `files:`). "Mirror nextai's `release.yaml`" is not enough
  on its own. The updater already targets `github.com/C5H12O5/TextGO/releases`.

**Docs**

- `CLIP-EXTENSIONS.md` with SnipDo install steps; one-line architecture note in `CLAUDE.md`.

### Phase 2 — macOS parity (independent)

- `handlers/clip.rs`: add the `#[cfg(unix)]` `UnixListener` branch at `/tmp/textgo.sock`,
  chmod 0600; stop unlinks the socket file.
- `clip-extensions/popclip/`: `Config.plist`, `textgo.sh`
  (`curl --unix-socket /tmp/textgo.sock`; `open -g -a` launch+retry fallback), `icon.png`,
  `textgo.png`. PopClip passes text as `$POPCLIP_TEXT`. Identifier
  `top.xylitol.textgo.clip-extensions.popclip`.
- `release.yml`: add the macOS `.popclipextz` zip + upload step.
- Docs: extend `CLIP-EXTENSIONS.md` with PopClip steps.

## 7. Security model

- **Off by default.** Listener never binds unless the user enables it.
- **OS-enforced owner-only boundary:** unix socket (0600) on macOS; named pipe with a
  protected owner-only ACL on Windows. Neither is reachable by a browser `fetch()` (contrast
  Option B, and the tokenless-TCP hazard in §5). This is the reason A beats B *and* the reason
  Windows uses a named pipe rather than loopback TCP.
- **Route through user-defined rules only.** External text triggers whatever the
  user bound to `ClipExtension`; no implicit/hidden script execution. If nothing
  is bound, nothing runs.
- **Disable = inert.** On disable/exit: stop accepting, drop the listener; on macOS remove
  the socket file.
- **Residual risk:** another process *running as the same user* can POST while enabled
  (identical on both platforms — a same-user process can open a 0600 socket or an owner-ACL
  pipe). Accepted for an opt-in feature; documented in the settings warning. Optional future
  hardening: a per-install token the package script includes, layered on top of the
  pipe/socket.

## 8. Verification (manual — repo has no test suite)

1. Enable the setting → confirm the listener binds (Windows:
   `Get-ChildItem \\.\pipe\ | ? Name -eq textgo`; macOS: socket file present); disable →
   confirm it is gone.
2. Windows (Phase 1): install the SnipDo package, select text, invoke → `shortcut` event
   fires with `ClipExtension` + text → bound rule runs (toolbar/popup as configured).
3. macOS (Phase 2): same via PopClip.
4. Large multi-KB / multiline selection survives intact (the reason for A).
5. With the setting off, no listener / connection refused.
6. **Registrar:** bind then unbind a rule to `ClipExtension` → no OS-accelerator error
   (proves the §6 carve-out).
7. **Negative (web-reachability):** from the browser devtools console,
   `fetch('http://127.0.0.1', { method: 'POST', body: 'x' })` and a second-user process both
   fail to reach the listener.

## 9. Rollback

- Feature is a single persisted boolean + a listener. Off = fully inert.
- No persisted external state beyond the one setting; the socket file (macOS) is removed on
  disable/exit and a named pipe (Windows) vanishes with the process. Reverting the code
  removes the module, the command, the constant, and the packages with no data migration.

## 10. Open questions — RESOLVED at review (2026-07-02)

1. **Transport:** ✅ **Option A.** Windows uses a **named pipe** (owner-only ACL), not
   loopback TCP — TCP has a web-reachable side effect into a script-exec pipeline (§5).
   macOS uses the unix socket 0600.
2. **Windows port:** ✅ **N/A.** A named pipe has no port; `\\.\pipe\textgo` is the endpoint.
3. **Settings placement:** ✅ **General page**, not the "Extensions" nav (that name is the
   JSON action-extension system, §3.1).
4. **Worth it given native capture (§3.3):** ✅ **Yes — build it, Windows-first.** Kept as a
   gated, off-by-default secondary trigger, not a headline. Windows is the priority target;
   macOS parity follows as an independent phase (§6).
