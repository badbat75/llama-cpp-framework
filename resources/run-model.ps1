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
trap {
    Write-Host "`n[✕] ERROR: $($_.Exception.Message)" -ForegroundColor Red
    Write-Host "`nPress any key to close..." -ForegroundColor DarkYellow
    $null = [System.Console]::ReadKey($true)
    break
}

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

# ── GPU info (OS-level — llama-server also prints this on startup) ───
$gpuName  = ''
$gpuVRAM  = ''
try {
    $nvidiaSmi = Get-Command nvidia-smi -ErrorAction SilentlyContinue
    if ($nvidiaSmi) {
        $smiOut = & $nvidiaSmi.Source --query-gpu=name,memory.total --format=csv,noheader,nounits 2>$null
        if ($smiOut) {
            $parts = $smiOut.Trim().Split(',')
            $gpuName = $parts[0].Trim()
            $gpuVRAM = "{0} MiB" -f $parts[1].Trim()
        }
    }
    if (-not $gpuName) {
        $card = Get-CimInstance Win32_VideoController | Sort-Object AdapterRAM -Descending | Select-Object -First 1
        if ($card) { $gpuName = $card.Name.Trim() }
    }
    if ($gpuName -and -not $gpuVRAM) {
        $regKey = Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}\0*' -ErrorAction SilentlyContinue |
            Where-Object { $_.DriverDesc -and $gpuName -match [regex]::Escape($_.DriverDesc) } |
            Select-Object -First 1
        if ($regKey -and $regKey.HardwareInformation.qwMemorySize) {
            $gpuVRAM = "{0} MiB" -f [Math]::Round([long]$regKey.HardwareInformation.qwMemorySize / 1MB)
        }
    }
    if ($gpuName -and -not $gpuVRAM) {
        $card = Get-CimInstance Win32_VideoController | Where-Object { $_.Name -eq $gpuName } | Select-Object -First 1
        if ($card -and $card.AdapterRAM) {
            $gpuVRAM = "{0} MiB" -f [Math]::Round($card.AdapterRAM / 1MB)
        }
    }
} catch {}

# ── Router CLI args ──────────────────────────────────────────────────
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

# ── Banner ───────────────────────────────────────────────────────────
$m = (& $ServerExe --version 2>&1) -join "`n" | Select-String 'version:\s+(\S+)'
$verRaw = if ($m) { $m.Matches[0].Groups[1].Value } else { 'unknown' }
Write-Host ""

$BANNER_W = 80
$bannerRows = [System.Collections.ArrayList]@(
    ,@('llama.cpp', "v$verRaw")
    ,@('Presets', $presetsPath)
    ,@('Log',     $logPath)
)
if ($gpuName) { $bannerRows.Insert(0, @('GPU', "$gpuName  ($gpuVRAM)")) }

foreach ($r in $bannerRows) {
    $needed = 4 + 14 + $r[1].Length
    if ($needed -gt $BANNER_W) { $BANNER_W = $needed }
}

function Write-BannerRow { param([string]$Label, [string]$Value)
    $rowText  = ("{0,-14}" -f $Label) + $Value
    $padding  = " " * ($BANNER_W - 4 - $rowText.Length)
    Write-Host ("║ $($rowText)$padding ║") -ForegroundColor DarkGray
}

Write-Host ("╔" + ("═" * ($BANNER_W - 2)) + "╗") -ForegroundColor DarkGray
foreach ($r in $bannerRows) { Write-BannerRow $r[0] $r[1] }
Write-Host ("╚" + ("═" * ($BANNER_W - 2)) + "╝") -ForegroundColor DarkGray

# ── Launch info ──────────────────────────────────────────────────────
Write-Host ""
Write-Host "[*] Starting llama-server (router)..."             -ForegroundColor Cyan
$urlHost = if ($hostname -eq '0.0.0.0') { [System.Net.Dns]::GetHostName() } else { $hostname }
Write-Host ("    url:                  http://{0}:{1}" -f $urlHost, $port) -ForegroundColor Gray
Write-Host ("    command:              & `"$ServerExe`" $($serverArgs -join ' ')") -ForegroundColor Gray
Write-Host ("    threads:              {0}" -f $threads) -ForegroundColor Gray
Write-Host ("    batch:                {0}" -f $threadsBatch) -ForegroundColor Gray
Write-Host ("    max-models:           {0}" -f $modelsMax) -ForegroundColor Gray

$presetSections = [regex]::Matches((Get-Content -Path $presetsPath -Raw -Encoding UTF8), '(?m)^\[([^\]]+)\]') | ForEach-Object { $_.Groups[1].Value }
if ($presetSections) {
    Write-Host "    models:" -ForegroundColor Gray
    foreach ($m in $presetSections) {
        Write-Host "      - $m" -ForegroundColor DarkGray
    }
}

# ── Launch server (foreground, Ctrl+C to stop) ──────────────────────
Write-Host ""
Write-Host "[*] Ctrl+C to stop" -ForegroundColor DarkYellow
Write-Host ""

try {
    & $ServerExe @serverArgs *>> $logPath
} finally {
    Write-Host ""
    Write-Host "[✓] Server stopped." -ForegroundColor Green
}
