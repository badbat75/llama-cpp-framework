# llama-server runtime configuration.
#
# Writes %LOCALAPPDATA%\llama.cpp\config\server.psd1. Prompts for every
# server-side parameter (defaulting to the current value or a sane hardcoded
# default; Enter accepts the default, `-` explicitly unsets an optional
# field). Preserves ModelsDir / ActiveModel set by config-model.ps1.
#
# NSIS calls this with -NonInteractive plus the values collected on the
# install-time custom page (Port, Hostname); other params fall back to
# existing values or hardcoded defaults. Model-dependent params (CtxSize,
# GpuLayers, Parallel, CacheTypeK/V, FlashAttn) live in models\<id>.psd1.
#
[CmdletBinding()]
param(
    [int]$Port,
    [string]$Hostname,
    [Nullable[int]]$Threads,
    [Nullable[bool]]$Mlock,
    [Nullable[int]]$CacheReuse,
    [Nullable[int]]$ThreadsBatch,
    [string]$ModelsDir,
    [switch]$NonInteractive,
    # When set, dump the existing server.psd1 fields as a [Server] INI section
    # appended to this file (UTF-16 LE so NSIS Unicode's ReadINIStr can parse
    # it) and exit without writing anything else. Used by the installer's
    # .onInit to surface the user's previous values as install-page defaults.
    [string]$DumpIni
)

$ErrorActionPreference = 'Stop'

$configDir  = Join-Path $env:LOCALAPPDATA "llama.cpp\config"
$serverPath = Join-Path $configDir "server.psd1"

$cur = @{}
if (Test-Path $serverPath) {
    try { $cur = Import-PowerShellDataFile -Path $serverPath } catch { }
}

if ($DumpIni) {
    if ($cur.Count -gt 0) {
        $lines = @(
            '[Server]'
            "Port=$($cur.Port)"
            "Hostname=$($cur.Hostname)"
            "Threads=$($cur.Threads)"
            "ThreadsBatch=$($cur.ThreadsBatch)"
            "ModelsDir=$($cur.ModelsDir)"
        )
        Add-Content -LiteralPath $DumpIni -Value $lines -Encoding Unicode
    }
    return
}

New-Item -ItemType Directory -Path $configDir -Force | Out-Null

# ── Helpers ──────────────────────────────────────────────────────────

function Read-IntDefault {
    param([string]$Prompt, $Default, [int]$Min = 0, [int]$Max = [int]::MaxValue, [switch]$AllowUnset)
    while ($true) {
        $shown = if ($null -eq $Default) { 'unset' } else { "$Default" }
        $reply = Read-Host "$Prompt [$shown]"
        if (-not $reply) { return $Default }
        if ($AllowUnset -and $reply -eq '-') { return $null }
        [int]$parsed = 0
        if ([int]::TryParse($reply, [ref]$parsed) -and $parsed -ge $Min -and $parsed -le $Max) {
            return $parsed
        }
        Write-Host "  Invalid value." -ForegroundColor Yellow
    }
}

function Read-BoolDefault {
    param([string]$Prompt, [bool]$Default)
    while ($true) {
        $shown = if ($Default) { 'Y/n' } else { 'y/N' }
        $reply = Read-Host "$Prompt [$shown]"
        if (-not $reply) { return $Default }
        if ($reply -match '^[yY]') { return $true }
        if ($reply -match '^[nN]') { return $false }
        Write-Host "  Invalid (y/n)." -ForegroundColor Yellow
    }
}

# ── Resolve initial values ───────────────────────────────────────────

# Defaults match the runtime fallback in resources\run-model.ps1:
# Threads = floor(cores * 0.5), ThreadsBatch = floor(cores * 0.75); both floored to 1.
$cores = [Environment]::ProcessorCount
$defaultThreads      = [Math]::Max(1, [Math]::Floor($cores * 0.5))
$defaultThreadsBatch = [Math]::Max(1, [Math]::Floor($cores * 0.75))

$portVal       = if ($PSBoundParameters.ContainsKey('Port')       -and $Port -gt 0) { $Port }       elseif ($cur.Port)       { $cur.Port }       else { 8080 }
$hostVal       = if ($PSBoundParameters.ContainsKey('Hostname')   -and $Hostname)   { $Hostname }   elseif ($cur.Hostname)   { $cur.Hostname }   else { 'localhost' }
$mlockVal      = if ($PSBoundParameters.ContainsKey('Mlock')     -and $null -ne $Mlock)     { [bool]$Mlock }     elseif ($null -ne $cur.Mlock)     { [bool]$cur.Mlock }     else { $true }
$threadsVal    = if ($PSBoundParameters.ContainsKey('Threads')   -and $null -ne $Threads)   { [int]$Threads }    elseif ($null -ne $cur.Threads)   { [int]$cur.Threads }   else { $defaultThreads }
$cacheReuseVal = if ($PSBoundParameters.ContainsKey('CacheReuse') -and $null -ne $CacheReuse) { [int]$CacheReuse } elseif ($null -ne $cur.CacheReuse) { [int]$cur.CacheReuse } else { 256 }
$threadsBatchVal = if ($PSBoundParameters.ContainsKey('ThreadsBatch') -and $null -ne $ThreadsBatch) { [int]$ThreadsBatch } elseif ($null -ne $cur.ThreadsBatch) { [int]$cur.ThreadsBatch } else { $defaultThreadsBatch }

# ── Interactive prompts ──────────────────────────────────────────────

if (-not $NonInteractive) {
    Write-Host ""
    Write-Host "── llama-server configuration ──" -ForegroundColor Cyan
    Write-Host "Press Enter to accept the default; type '-' to unset an optional field." -ForegroundColor DarkGray
    Write-Host ""

    $portVal = Read-IntDefault "Port" $portVal 1 65535

    Write-Host ""
    Write-Host "Network exposure:" -ForegroundColor Cyan
    Write-Host "  [1] localhost only  (only this machine)"
    Write-Host "  [2] all interfaces  (LAN-reachable)"
    $defaultBind = if ($hostVal -eq '0.0.0.0') { 2 } else { 1 }
    $bindReply = Read-IntDefault "Bind to" $defaultBind 1 2
    $hostVal = if ($bindReply -eq 2) { '0.0.0.0' } else { 'localhost' }

    $mlockVal     = Read-BoolDefault "Mlock (lock model in RAM)" $mlockVal
    $threadsVal   = Read-IntDefault  "CPU threads (auto-detected if unset)" $threadsVal -Min 1 -Max 256 -AllowUnset
    $cacheReuseVal = Read-IntDefault  "Cache reuse (min chunk size, --cache-reuse)" $cacheReuseVal -Min 0 -AllowUnset
    $threadsBatchVal = Read-IntDefault "Batch threads (--threads-batch)" $threadsBatchVal -Min 1 -AllowUnset
}

# ── Render ───────────────────────────────────────────────────────────

$activeModel = if ($cur.ActiveModel) { $cur.ActiveModel } else { '' }
$modelsDir   = if ($PSBoundParameters.ContainsKey('ModelsDir') -and $ModelsDir) { $ModelsDir } `
               elseif ($cur.ModelsDir) { $cur.ModelsDir } `
               else { '' }
if ($modelsDir -and -not (Test-Path -LiteralPath $modelsDir)) {
    New-Item -ItemType Directory -Path $modelsDir -Force | Out-Null
}

$mlockLit    = if ($mlockVal)     { '$true' } else { '$false' }
$hostEsc     = $hostVal     -replace "'", "''"
$activeEsc   = $activeModel -replace "'", "''"
$modelsEsc   = $modelsDir   -replace "'", "''"

$threadsLine = if ($null -ne $threadsVal) {
    "    Threads        = $threadsVal"
} else {
    '    # Threads      = 12  # optional override; auto-detected if unset'
}

$cacheReuseLine = if ($null -ne $cacheReuseVal) {
    "    CacheReuse        = $cacheReuseVal"
} else {
    ''
}

$threadsBatchLine = if ($null -ne $threadsBatchVal) {
    "    ThreadsBatch      = $threadsBatchVal"
} else {
    ''
}

$content = @"
@{
    # Generated by config-server.ps1 on $(Get-Date -Format 'yyyy-MM-dd HH:mm')

    # ── llama-server runtime (machine-wide; model-specific knobs in models\<id>.psd1) ──
    Port           = $portVal
    Hostname       = '$hostEsc'
    Mlock          = $mlockLit
$threadsLine
$cacheReuseLine
$threadsBatchLine

    # ── Models registry (populated by config-model.ps1) ──────────────────
    ModelsDir      = '$modelsEsc'
    ActiveModel    = '$activeEsc'
}
"@

Set-Content -Path $serverPath -Value $content -Encoding utf8NoBOM

if (-not $NonInteractive) {
    Write-Host ""
    Write-Host "Configuration written to:" -ForegroundColor Green
    Write-Host "  $serverPath"
    Write-Host ""
}
