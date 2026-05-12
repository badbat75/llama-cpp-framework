# Runtime entry point for the installed llama.cpp distribution. Launched by the
# "llama.cpp" Start Menu shortcut. Starts llama-server in *router mode* against
# a user-owned %LOCALAPPDATA%\llama.cpp\config\presets.ini.
#
# Reads:
#   %LOCALAPPDATA%\llama.cpp\config\server.ini    — router-wide params
#   %LOCALAPPDATA%\llama.cpp\config\presets.ini   — per-model presets (one [section] each)
#
# presets.ini is the source of truth — hand-edit it freely; config-model.ps1
# updates one section at a time while preserving the rest of the file.
#
# -ServerExe is an override for pointing at an alternate llama-server build
# (e.g. .\build\llama.cpp-cmake\bin\llama-server.exe from an in-tree build).

[CmdletBinding()]
param(
    [string]$ServerExe = (Join-Path $PSScriptRoot "bin\llama-server.exe")
)

$ErrorActionPreference = 'Stop'
$installDir = $PSScriptRoot

. (Join-Path $installDir "common-functions.ps1")

$configDir   = Join-Path $env:LOCALAPPDATA "llama.cpp\config"
$serverPath  = Join-Path $configDir "server.ini"
$presetsPath = Join-Path $configDir "presets.ini"

# ── server.ini ───────────────────────────────────────────────────────
if (-not (Test-Path $serverPath)) {
    & (Join-Path $installDir "config-server.ps1")
    if (-not (Test-Path $serverPath)) { throw "server.ini was not created. Aborting." }
}
$srvRaw = Read-ServerIni -Path $serverPath

# Coerce strings → typed values.
function ConvertTo-IntOrNull  { param($v) if ([string]::IsNullOrWhiteSpace("$v")) { return $null }; $p=0; if ([int]::TryParse("$v", [ref]$p)) { return $p } else { return $null } }
function ConvertTo-BoolOrNull { param($v) $s = "$v".Trim().ToLowerInvariant(); if ($s -in @('true','yes','on','1','$true'))  { return $true }; if ($s -in @('false','no','off','0','$false')) { return $false }; return $null }

$srv = @{
    Port         = ConvertTo-IntOrNull  $srvRaw['Port']
    Hostname     = $srvRaw['Hostname']
    Mlock        = ConvertTo-BoolOrNull $srvRaw['Mlock']
    Threads      = ConvertTo-IntOrNull  $srvRaw['Threads']
    ThreadsBatch = ConvertTo-IntOrNull  $srvRaw['ThreadsBatch']
    CacheReuse   = ConvertTo-IntOrNull  $srvRaw['CacheReuse']
    ModelsMax    = ConvertTo-IntOrNull  $srvRaw['ModelsMax']
    ModelsDir    = $srvRaw['ModelsDir']
}

# ── Need at least one configured preset ──────────────────────────────
$hasPresets = (Test-Path $presetsPath) -and `
              ([regex]::IsMatch((Get-Content -Path $presetsPath -Raw -Encoding UTF8), '(?m)^\['))
if (-not $hasPresets) {
    Write-Host "No model presets found — launching config-model.ps1..." -ForegroundColor Cyan
    & (Join-Path $installDir "config-model.ps1")
    $hasPresets = (Test-Path $presetsPath) -and `
                  ([regex]::IsMatch((Get-Content -Path $presetsPath -Raw -Encoding UTF8), '(?m)^\['))
    if (-not $hasPresets) { throw "No model presets configured. Aborting." }
}

# LLAMA_CACHE points -hf downloads at the user's ModelsDir so they land alongside
# the user's other .gguf files instead of in the default %USERPROFILE%\.cache.
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

# ── Router CLI args ──────────────────────────────────────────────────
# host/port/threads/mlock/cache-reuse stay as CLI flags on the router; model
# instances inherit them when the router spawns them.
$hostname  = if ($srv.Hostname)              { $srv.Hostname }  else { 'localhost' }
$port      = if ($null -ne $srv.Port)        { $srv.Port }      else { 8080 }
$modelsMax = if ($null -ne $srv.ModelsMax)   { $srv.ModelsMax } else { 1 }

$serverArgs = @(
    '--models-preset',   $presetsPath
    '--models-max',      $modelsMax
    '--port',            $port
    '--host',            $hostname
    '--webui-mcp-proxy'
)

if ($srv.Mlock)                { $serverArgs += '--mlock' }
if ($null -ne $srv.CacheReuse) { $serverArgs += '--cache-reuse', $srv.CacheReuse }

$cpuCores     = [Environment]::ProcessorCount
$threads      = if ($null -ne $srv.Threads)      { $srv.Threads }      else { [Math]::Max(1, [Math]::Floor($cpuCores * 0.5)) }
$threadsBatch = if ($null -ne $srv.ThreadsBatch) { $srv.ThreadsBatch } else { [Math]::Max(1, [Math]::Floor($cpuCores * 0.75)) }
$serverArgs += '-t', $threads
$serverArgs += '--threads-batch', $threadsBatch

Write-Host "Presets file:       $presetsPath  (max simultaneous: $modelsMax)" -ForegroundColor DarkGray
Write-Host "Starting llama-server (router) on ${hostname}:$port..." -ForegroundColor Cyan
Write-Host "Log:                $logPath" -ForegroundColor DarkGray
& $ServerExe @serverArgs *>> $logPath
