# Standalone launcher for Open WebUI.
# Reads webui.psd1 for its own bind/port, and server.psd1 for the llama-server
# endpoint to point at (probes whether llama-server is already running). If
# llama-server isn't up, Open WebUI still starts (the user can launch it
# separately or pick a different OpenAI-compatible backend in Admin Settings).
#
# -WebUIExe lets the dev launcher point at the in-tree webui-venv.

[CmdletBinding()]
param(
    [string]$WebUIExe
)

$ErrorActionPreference = 'Stop'
$installDir = $PSScriptRoot

$configDir  = Join-Path $env:LOCALAPPDATA "llama.cpp\config"
$serverPath = Join-Path $configDir "server.psd1"
$webuiPath  = Join-Path $configDir "webui.psd1"

# ── webui.psd1 ───────────────────────────────────────────────────────
if (-not (Test-Path $webuiPath)) {
    & (Join-Path $installDir "config-webui.ps1")
    if (-not (Test-Path $webuiPath)) { throw "webui.psd1 was not created. Aborting." }
}
$wui = Import-PowerShellDataFile -Path $webuiPath
$wuiHost = if ($null -ne $wui.Hostname) { $wui.Hostname } else { 'localhost' }
$wuiPort = if ($null -ne $wui.Port)     { $wui.Port }     else { 3000 }

# ── server.psd1 (only used to discover the llama-server endpoint) ────
$srv = if (Test-Path $serverPath) { Import-PowerShellDataFile -Path $serverPath } else { @{} }
$srvHost = if ($null -ne $srv.Hostname) { $srv.Hostname } else { 'localhost' }
$srvPort = if ($null -ne $srv.Port)     { $srv.Port }     else { 8080 }

# ── Locate Open WebUI ───────────────────────────────────────────────
if (-not $WebUIExe) {
    $webuiDir = $null
    $regPath  = "HKLM:\Software\llama.cpp"
    if (Test-Path $regPath) {
        $webuiDir = (Get-ItemProperty $regPath).WebUIDir
    }
    if (-not $webuiDir) { $webuiDir = "${env:ProgramFiles}\Open WebUI" }
    $WebUIExe = Join-Path $webuiDir "Scripts\open-webui.exe"
}
if (-not (Test-Path $WebUIExe)) {
    throw "Open WebUI not installed at $WebUIExe. Re-run the installer and select the 'Open WebUI' component."
}

# ── CWD + DATA_DIR for writable per-user data ───────────────────────
# WebUI writes .webui_secret_key to CWD on first run; webui.db, vector_db/,
# uploads/, etc. go to DATA_DIR. Without DATA_DIR, Open WebUI falls back to a
# path inside Program Files (read-only) and ChromaDB crashes with
# "attempt to write a readonly database".
$rootDir      = Join-Path $env:LOCALAPPDATA "llama.cpp"
$webuiDataDir = Join-Path $rootDir "data"
New-Item -ItemType Directory -Path $rootDir -Force | Out-Null
New-Item -ItemType Directory -Path $webuiDataDir -Force | Out-Null
Set-Location $rootDir
$env:DATA_DIR = $webuiDataDir

# ── Probe whether llama-server is already running ───────────────────
# 0.0.0.0 in config means "bind to all interfaces" — probe localhost.
$probeHost = if ($srvHost -eq '0.0.0.0') { 'localhost' } else { $srvHost }
$llamaRunning = $false
try {
    $client = [System.Net.Sockets.TcpClient]::new()
    $iar = $client.BeginConnect($probeHost, $srvPort, $null, $null)
    if ($iar.AsyncWaitHandle.WaitOne(500)) {
        $client.EndConnect($iar)
        $llamaRunning = $client.Connected
    }
    $client.Close()
} catch { $llamaRunning = $false }

if ($llamaRunning) {
    Write-Host "Detected llama-server on ${probeHost}:${srvPort} — Open WebUI will use it." -ForegroundColor Green
} else {
    Write-Host "llama-server is not running on ${probeHost}:${srvPort}." -ForegroundColor Yellow
    Write-Host "Starting Open WebUI standalone — launch llama-server separately to enable chat." -ForegroundColor DarkGray
}

# Seed the OpenAI-compatible endpoint to llama-server's address (first run only;
# afterwards Admin Settings persist in webui.db and override these env vars).
$env:OPENAI_API_BASE_URL = "http://${probeHost}:${srvPort}/v1"
$env:OPENAI_API_KEY      = "none"

# Open WebUI reads HOST from env to set the uvicorn bind interface.
$env:HOST = $wuiHost
# Unbuffer Python output so the log catches up in real time
$env:PYTHONUNBUFFERED = '1'
# Force UTF-8 for stdio: Open WebUI prints a banner with box-drawing chars
# that cp1252 (Windows default when stdout is redirected to a file) can't
# encode, crashing the boot with UnicodeEncodeError.
$env:PYTHONIOENCODING = 'utf-8'

# ── Log file (tee'd from open-webui stdout+stderr) ──────────────────
$logsDir = Join-Path $env:LOCALAPPDATA "llama.cpp\logs"
New-Item -ItemType Directory -Path $logsDir -Force | Out-Null
$logPath = Join-Path $logsDir "open-webui.log"
"=== Started $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss') ===" | Out-File -FilePath $logPath -Append -Encoding UTF8

$displayHost = if ($wuiHost -eq '0.0.0.0') { 'localhost' } else { $wuiHost }
Write-Host "Starting Open WebUI on ${wuiHost}:${wuiPort}..." -ForegroundColor Cyan
Write-Host "Open WebUI: http://${displayHost}:${wuiPort}" -ForegroundColor Green
Write-Host "Log: $logPath" -ForegroundColor DarkGray
& $WebUIExe serve --port $wuiPort *>> $logPath
