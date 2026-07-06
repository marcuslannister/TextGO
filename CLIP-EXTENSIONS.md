# Clip Extensions

This package forwards selected text from a clip action to NextAI Translator.
It is useful when TextGO can capture the selection but you want the selected text
handled by NextAI Translator instead.

Currently supported: **NextAI Translator** on Windows.

## How it works

The PowerShell script reads TextGO's `$selection`, encodes it as UTF-8, and
POSTs it to NextAI Translator's local listener:

```powershell
http://127.0.0.1:62007/
```

NextAI Translator must already be running. The local endpoint decides which
NextAI action to run based on the app's current/default action.

## Setup

1. Start NextAI Translator.
2. In TextGO, create a PowerShell script action using the script from
   `clip-extensions/nextai-translator/textgo.ps1`.
3. Bind that script to the shortcut or rule you want.
4. Select text and run the TextGO action. The selection is sent to NextAI
   Translator.

## Notes

- The script does not choose translate vs polish. Set **Default Action** in
  NextAI Translator, or select the action in NextAI before using the TextGO
  script.
- If NextAI Translator is not running, the request fails silently.
- The package source lives in `clip-extensions/nextai-translator/`
  (`textgo.json`, `textgo.ps1`, `icon.png`).
