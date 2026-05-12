# Runtime entry point for the installed llama.cpp distribution.
# Launched by the "llama.cpp" Start Menu shortcut. Starts llama-server only —
# Open WebUI has its own dedicated shortcut ("Start Open WebUI").
#
# Reads two split configs:
#   %LOCALAPPDATA%\llama.cpp\config\server.psd1            — runtime params
#   %LOCALAPPDATA%\llama.cpp\config\models\<active>.psd1   — model + sampling
#
# server.psd1 is written by NSIS at install. ActiveModel is empty until the
# user runs config-model.ps1 (lazily invoked here on first launch).
#
# -ServerExe lets the dev-mode launcher (04-run.ps1) point at the in-tree build.

[CmdletBinding()]
param(
    [string]$ServerExe = (Join-Path $PSScriptRoot "bin\llama-server.exe")
)

$ErrorActionPreference = 'Stop'
$installDir = $PSScriptRoot

$configDir  = Join-Path $env:LOCALAPPDATA "llama.cpp\config"
$serverPath = Join-Path $configDir "server.psd1"
$modelsRoot = Join-Path $configDir "models"

# ── server.psd1 ──────────────────────────────────────────────────────
if (-not (Test-Path $serverPath)) {
    & (Join-Path $installDir "config-server.ps1")
    if (-not (Test-Path $serverPath)) { throw "server.psd1 was not created. Aborting." }
}
$srv = Import-PowerShellDataFile -Path $serverPath

# ── Active model — prompt via config-model.ps1 if unset / missing ──────
if (-not $srv.ActiveModel) {
    & (Join-Path $installDir "config-model.ps1")
    $srv = Import-PowerShellDataFile -Path $serverPath
    if (-not $srv.ActiveModel) { throw "No active model configured. Aborting." }
}
$modelCfgPath = Join-Path $modelsRoot "$($srv.ActiveModel).psd1"
if (-not (Test-Path $modelCfgPath)) {
    Write-Host "Active model '$($srv.ActiveModel)' has no config at $modelCfgPath" -ForegroundColor Yellow
    & (Join-Path $installDir "config-model.ps1")
    $srv = Import-PowerShellDataFile -Path $serverPath
    $modelCfgPath = Join-Path $modelsRoot "$($srv.ActiveModel).psd1"
    if (-not (Test-Path $modelCfgPath)) { throw "Model config still missing. Aborting." }
}
$mdl = Import-PowerShellDataFile -Path $modelCfgPath

if ($srv.ModelsDir) { $env:LLAMA_CACHE = $srv.ModelsDir }

# ── Locate llama-server ─────────────────────────────────────────────
if (-not (Test-Path $ServerExe)) {
    throw "llama-server.exe not found at $ServerExe."
}

# ── CWD to writable per-user dir ────────────────────────────────────
$dataDir = Join-Path $env:LOCALAPPDATA "llama.cpp"
New-Item -ItemType Directory -Path $dataDir -Force | Out-Null
Set-Location $dataDir

# ── Log file (tee'd from llama-server stdout+stderr) ────────────────
$logsDir = Join-Path $env:LOCALAPPDATA "llama.cpp\logs"
New-Item -ItemType Directory -Path $logsDir -Force | Out-Null
$logPath = Join-Path $logsDir "llama-server.log"
"=== Started $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss') ===" | Out-File -FilePath $logPath -Append -Encoding UTF8

# ── Build server arguments ──────────────────────────────────────────
$modelArgs = if (Test-Path -LiteralPath $mdl.Model) {
    @("-m", $mdl.Model)
} else {
    @("-hf", $mdl.Model)
}

$hostname = if ($null -ne $srv.Hostname) { $srv.Hostname } else { "localhost" }

$serverArgs = $modelArgs + @(
    "--cache-type-k", $mdl.CacheTypeK
    "--cache-type-v", $mdl.CacheTypeV
    "-np", $mdl.Parallel
    "-ngl", $mdl.GpuLayers
    "--ctx-size", $mdl.CtxSize
    "--port", $srv.Port
    "--host", $hostname
)

if ($null -ne $mdl.BatchSize) { $serverArgs += "--batch-size", $mdl.BatchSize }
if ($null -ne $mdl.UbatchSize) { $serverArgs += "--ubatch-size", $mdl.UbatchSize }

if ($mdl.FlashAttn) { $serverArgs += "-fa", "on" }
if ($mdl.Jinja)     { $serverArgs += "--jinja" }
if ($srv.Mlock)     { $serverArgs += "--mlock" }
if ($null -ne $srv.CacheReuse) { $serverArgs += "--cache-reuse", $srv.CacheReuse }

if ($mdl.ReasoningFormat) { $serverArgs += "--reasoning-format", $mdl.ReasoningFormat }

if ($null -ne $mdl.NCpuMoe) { $serverArgs += "--n-cpu-moe", $mdl.NCpuMoe }

if ($null -ne $mdl.Temp)               { $serverArgs += "--temp", $mdl.Temp }
if ($null -ne $mdl.TopK)               { $serverArgs += "--top-k", $mdl.TopK }
if ($null -ne $mdl.TopP)               { $serverArgs += "--top-p", $mdl.TopP }
if ($null -ne $mdl.MinP)               { $serverArgs += "--min-p", $mdl.MinP }
if ($null -ne $mdl.RepeatPenalty)      { $serverArgs += "--repeat-penalty", $mdl.RepeatPenalty }
if ($null -ne $mdl.PresencePenalty)    { $serverArgs += "--presence-penalty", $mdl.PresencePenalty }
if ($null -ne $mdl.ChatTemplateKwargs) { $serverArgs += "--chat-template-kwargs", $mdl.ChatTemplateKwargs }

$cpuCores = [Environment]::ProcessorCount
$threads = if ($null -ne $srv.Threads) {
    $srv.Threads
} else {
    [Math]::Max(1, [Math]::Floor($cpuCores * 0.5))
}
$threadsBatch = if ($null -ne $srv.ThreadsBatch) {
    $srv.ThreadsBatch
} else {
    [Math]::Max(1, [Math]::Floor($cpuCores * 0.75))
}
$serverArgs += "-t", $threads
$serverArgs += "--threads-batch", $threadsBatch

Write-Host "Active model: $($srv.ActiveModel)" -ForegroundColor DarkGray
Write-Host "Starting llama-server on ${hostname}:$($srv.Port)..." -ForegroundColor Cyan
Write-Host "Log: $logPath" -ForegroundColor DarkGray
& $ServerExe @serverArgs *>> $logPath
