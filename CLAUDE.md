# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

TextGO is a cross-platform (macOS/Windows) desktop tool that recognizes the type of selected text and runs custom actions on it, triggered by hotkeys, double-click, or drag-select. Stack: **Tauri v2** (Rust backend) + **SvelteKit / Svelte 5 runes** frontend, TailwindCSS v4 + daisyui, Paraglide (inlang) i18n.

**Package manager is pnpm** (required — see `pnpm-workspace.yaml`, which sets `minimumReleaseAge` and hoists `@codemirror/*`). Do not use npm/yarn.

## Commands

```bash
pnpm install
pnpm tauri dev                     # run the full app (Rust + frontend)
RUST_LOG=debug pnpm tauri dev      # with backend debug logs (Unix)
pnpm tauri build                   # build platform installer

pnpm dev / pnpm build              # frontend only (vite), rarely needed alone
pnpm check                         # svelte-kit sync + svelte-check (type check)
pnpm lint                          # prettier --check + eslint
pnpm format                        # prettier --write

pnpm sync-versions                 # sync version across manifests (scripts/sync-versions.ts)
pnpm scan-packages                 # regenerate LICENSES.md
```

Rust lives in `src-tauri/`; lint/format it with `cargo fmt` and `cargo clippy --manifest-path ./src-tauri/Cargo.toml -- -D warnings` (clippy warnings are errors).

There is **no test suite** in this repo — do not assume `pnpm test` exists.

**Pre-commit hooks** (`.pre-commit-config.yaml`) enforce: prettier on `src`, `cargo fmt`, `cargo clippy -D warnings`, and **commitizen** — commit messages must follow Conventional Commits (`feat|fix|...`).

## Architecture

### Three windows, one SPA
SSR is disabled; `adapter-static` builds a single SPA (`svelte.config.js`, `+layout.ts` sets `ssr = false`). That one build backs three Tauri webviews (see `src-tauri/tauri.conf.json`):
- **main** — settings/config UI, routes under `src/routes/(main)/` (general, model, mouse, prompt, regexp, script, searcher, display; plus `shortcuts/`, `histories/`).
- **popup** — `src/routes/popup/`, the instant-result window.
- **toolbar** — `src/routes/toolbar/`, the interactive action toolbar. On macOS it's a native `tauri-nspanel` panel (defined in `lib.rs`).

Windows coordinate through Tauri events: `show-popup`/`hide-popup`, `show-toolbar`/`hide-toolbar`, `toolbar-entered`/`toolbar-exited`.

### Rust backend (`src-tauri/src/`)
- `lib.rs` — app setup + all global mutable state (`LazyLock<Mutex<…>>` / `Atomic*`: shortcut state, Enigo keyboard, clipboard, selection cache). Spawns the global input listener via `rdev`.
- `commands/` — Tauri `#[command]` fns, one module per concern (clipboard, executor, identifier, keyboard, permission, selection, shortcut, tray, typer, window), re-exported through `commands.rs` and registered in `lib.rs`'s `generate_handler!`. Frontend calls these via `invoke(...)`.
- `handlers/` — keyboard/mouse global event handlers, plus `clip.rs` (opt-in, off-by-default owner-only named-pipe listener on Windows that re-emits pushed text as the `ClipExtension` pseudo-shortcut; see `CLIP-EXTENSIONS.md`).
- `platform/` — `macos.rs` / `windows.rs` platform-specific implementations behind `platform.rs`.

Script actions run through backend commands `execute_python` / `execute_javascript` / `execute_shell` / `execute_powershell`.

### Frontend pipeline (`src/lib/`)
A trigger fires → **match** the text to a rule → **execute** the rule's action:
- `matcher.ts` — `matchOne` / `matchAll` classify text against `Rule`s. Defines the built-in text-type catalogs (`TEXT_CASES`, `NATURAL_CASES`, `PROGRAMMING_CASES`, …).
- `executor.ts` — `execute()` runs a rule's action; defines built-in action catalogs (`GENERAL_ACTIONS`, `CONVERT_ACTIONS`, `PROCESS_ACTIONS`).
- `detector.ts` — programming-language detection (`@vscode/vscode-languagedetection`, loaded via a `require` shim).
- `classifier.ts` — user-trainable TensorFlow.js text classifier.
- `evaluator.ts` — in-process JS eval of user code (`evalSync`/`evalAsync`).
- `llm/` — one file per provider (`ollama`, `lmstudio`, `openrouter`, `openai`, `anthropic`, `google`, `xai`) over the `base.ts` interface, dispatched by `index.ts`.

Rules bind a shortcut + text `case` + `action`. Action/case IDs are namespaced by MARK-prefix constants in `constants.ts`: `model-`, `regexp-`, `script-`, `prompt-`, `searcher-`. When adding a new action or type source, follow the existing MARK convention.

### State & i18n
- `stores.svelte.ts` — persisted reactive state backed by the Tauri Store plugin (`.settings.dat`); sensitive fields go through `encrypt`/`decrypt` (`utils.ts`).
- `states.svelte.ts` — transient reactive UI state (e.g. `Loading` class).
- i18n: messages in `messages/{en,zh-CN}.json` (baseLocale `en`), compiled by Paraglide into `src/lib/paraglide/`. Import strings as `import { m } from '$lib/paraglide/messages'`. Add keys to both locale files.

## Conventions
- Svelte 5 runes mode is on (`compilerOptions.runes = true`) — use `$state`/`$derived`/`$effect`, not stores-as-globals.
- Prettier + eslint config are authoritative for style; run `pnpm format` before committing.
- User-facing strings must go through Paraglide `m.*`, never hardcoded.
