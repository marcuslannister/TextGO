param(
    [string]$PLAIN_TEXT
)

# SnipDo passes the selected text as $PLAIN_TEXT. Forward it to TextGO's
# clip-extension listener over its owner-only named pipe (\\.\pipe\textgo);
# the listener re-emits it as the "ClipExtension" trigger.
#
# Enable "Clip Extension Ingest" in TextGO settings first, and bind a rule to
# the Clip Extension trigger on the Shortcuts page.

function Send-TextGO([string]$text) {
    $pipe = New-Object System.IO.Pipes.NamedPipeClientStream('.', 'textgo', [System.IO.Pipes.PipeDirection]::Out)
    try {
        $pipe.Connect(2000)
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($text)
        $pipe.Write($bytes, 0, $bytes.Length)
        $pipe.Flush()
    } finally {
        $pipe.Dispose()
    }
}

try {
    Send-TextGO $PLAIN_TEXT
} catch {
    # TextGO not running (or clip ingest disabled): launch it, then retry once.
    Start-Process "textgo://" -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 1500
    try { Send-TextGO $PLAIN_TEXT } catch {}
}
