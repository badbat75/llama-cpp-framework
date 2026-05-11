# Dev-time launcher: runs llama-server from .\build\llama.cpp-cmake\bin without installing.
# Pass -WithWebUI to also launch Open WebUI from .\build\webui-venv in the background.
#
# This is a thin wrapper around resources\run-model.ps1 (and start-webui.ps1) —
# the heavy lifting (config loading, arg building, logging) lives there, so the
# installed runtime and the dev runtime can't drift.

[CmdletBinding()]
param(
    [switch]$WithWebUI
)

$ErrorActionPreference = 'Stop'

. "$PSScriptRoot\common.ps1"  # ROCm DLLs on PATH; no VS shell needed at runtime

$resourcesDir = Join-Path $PSScriptRoot "resources"
$serverExe    = Join-Path $PSScriptRoot "build\llama.cpp-cmake\bin\llama-server.exe"
if (-not (Test-Path $serverExe)) {
    throw "llama-server.exe not found at $serverExe. Run 02-build.ps1 first."
}

# Surface the cache dir so config-model.ps1's default points at the right place
# when this is the first launch.
if ($cfg.CacheDir) { $env:LLAMA_CACHE = $cfg.CacheDir }

# ── Optional: Open WebUI in the background ──────────────────────────
$webuiProc = $null
if ($WithWebUI) {
    $webuiExe = Join-Path $PSScriptRoot "build\webui-venv\Scripts\open-webui.exe"
    if (-not (Test-Path $webuiExe)) {
        Write-Host "Open WebUI not built at $webuiExe — skipping (run 02-build-webui.ps1)." -ForegroundColor Yellow
    } else {
        $webuiScript = Join-Path $resourcesDir "start-webui.ps1"
        $webuiProc = Start-Process pwsh -PassThru -WindowStyle Minimized -ArgumentList @(
            '-NoProfile', '-ExecutionPolicy', 'Bypass',
            '-File', $webuiScript,
            '-WebUIExe', $webuiExe
        )
    }
}

# ── llama-server (foreground; blocks until exit) ────────────────────
try {
    & (Join-Path $resourcesDir "run-model.ps1") -ServerExe $serverExe
} finally {
    if ($webuiProc -and -not $webuiProc.HasExited) {
        Write-Host "Stopping Open WebUI..." -ForegroundColor Cyan
        # /T kills the whole process tree (pwsh wrapper + open-webui.exe + uvicorn workers).
        & taskkill.exe /F /T /PID $webuiProc.Id 2>$null | Out-Null
    }
}
