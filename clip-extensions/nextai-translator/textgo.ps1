if (-not $selection) {
    exit 0
}

$body = [System.Text.Encoding]::UTF8.GetBytes($selection)

Invoke-WebRequest `
  -Uri "http://127.0.0.1:62007/" `
  -Method POST `
  -Body $body `
  -UseBasicParsing | Out-Null

""
