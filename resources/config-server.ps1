# llama-server runtime configuration.
#
# Writes %LOCALAPPDATA%\llama.cpp\config\server.ini. Prompts for every
# server-side parameter (defaulting to the current value or a sane hardcoded
# default; Enter accepts the default, `-` explicitly unsets an optional
# field). Preserves ModelsDir set by config-model.ps1.
#
# NSIS calls this with -NonInteractive plus the values collected on the
# install-time custom page (Port, Hostname, Threads, ThreadsBatch, ModelsDir);
# other params fall back to existing values or hardcoded defaults. Per-model
# params (CtxSize, GpuLayers, Parallel, CacheTypeK/V, FlashAttn, sampling)
# live in presets.ini, the file consumed by llama-server's --models-preset.

[CmdletBinding()]
param(
    [int]$Port,
    [string]$Hostname,
    [Nullable[int]]$Threads,
    [Nullable[bool]]$Mlock,
    [Nullable[int]]$CacheReuse,
    [Nullable[int]]$ThreadsBatch,
    [Nullable[int]]$ModelsMax,
    [string]$ModelsDir,
    [switch]$NonInteractive,
    # When set, dump the existing [Server] section as a UTF-16 LE INI file at
    # the given path and exit without writing anything else. Used by the
    # installer's .onInit so GetPrivateProfileStringW can surface the user's
    # previous values as install-page defaults.
    [string]$DumpIni
)

$ErrorActionPreference = 'Stop'

. (Join-Path $PSScriptRoot "common-functions.ps1")

$configDir   = Join-Path $env:LOCALAPPDATA "llama.cpp\config"
$serverPath  = Join-Path $configDir "server.ini"

$cur = Read-ServerIni -Path $serverPath

# ── -DumpIni: emit the [Server] section as UTF-16 for NSIS, then exit ──

if ($DumpIni) {
    if ($cur.Count -gt 0) {
        $lines = @(
            '[Server]'
            "Port=$($cur.Port)"
            "Hostname=$($cur.Hostname)"
            "Threads=$($cur.Threads)"
            "ThreadsBatch=$($cur.ThreadsBatch)"
            "ModelsMax=$($cur.ModelsMax)"
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

# Coerce string values from the INI / migration to typed defaults.
function ConvertTo-IntOrNull  { param($v) if ([string]::IsNullOrWhiteSpace("$v")) { return $null }; $p=0; if ([int]::TryParse("$v", [ref]$p)) { return $p } else { return $null } }
function ConvertTo-BoolOrNull { param($v) $s = "$v".Trim().ToLowerInvariant(); if ($s -in @('true','yes','on','1','$true'))  { return $true }; if ($s -in @('false','no','off','0','$false')) { return $false }; return $null }

# ── Resolve initial values ───────────────────────────────────────────

# Defaults match the runtime fallback in resources\run-model.ps1:
# Threads = floor(cores * 0.5), ThreadsBatch = floor(cores * 0.75); both floored to 1.
$cores = [Environment]::ProcessorCount
$defaultThreads      = [Math]::Max(1, [Math]::Floor($cores * 0.5))
$defaultThreadsBatch = [Math]::Max(1, [Math]::Floor($cores * 0.75))

$curPort         = ConvertTo-IntOrNull  $cur['Port']
$curThreads      = ConvertTo-IntOrNull  $cur['Threads']
$curCacheReuse   = ConvertTo-IntOrNull  $cur['CacheReuse']
$curThreadsBatch = ConvertTo-IntOrNull  $cur['ThreadsBatch']
$curModelsMax    = ConvertTo-IntOrNull  $cur['ModelsMax']
$curMlock        = ConvertTo-BoolOrNull $cur['Mlock']

$portVal       = if ($PSBoundParameters.ContainsKey('Port')       -and $Port -gt 0) { $Port }       elseif ($null -ne $curPort)    { $curPort }       else { 8080 }
$hostVal       = if ($PSBoundParameters.ContainsKey('Hostname')   -and $Hostname)   { $Hostname }   elseif ($cur['Hostname'])      { $cur['Hostname'] } else { 'localhost' }
$mlockVal      = if ($PSBoundParameters.ContainsKey('Mlock')      -and $null -ne $Mlock)        { [bool]$Mlock }        elseif ($null -ne $curMlock)        { $curMlock }        else { $true }
$threadsVal    = if ($PSBoundParameters.ContainsKey('Threads')    -and $null -ne $Threads)      { [int]$Threads }       elseif ($null -ne $curThreads)      { $curThreads }      else { $defaultThreads }
$cacheReuseVal = if ($PSBoundParameters.ContainsKey('CacheReuse') -and $null -ne $CacheReuse)   { [int]$CacheReuse }    elseif ($null -ne $curCacheReuse)   { $curCacheReuse }   else { 256 }
$threadsBatchVal = if ($PSBoundParameters.ContainsKey('ThreadsBatch') -and $null -ne $ThreadsBatch) { [int]$ThreadsBatch } elseif ($null -ne $curThreadsBatch) { $curThreadsBatch } else { $defaultThreadsBatch }
# ModelsMax: 0 = unlimited; 1 = strict one-at-a-time (default — typical single-GPU setup).
$modelsMaxVal    = if ($PSBoundParameters.ContainsKey('ModelsMax') -and $null -ne $ModelsMax)   { [int]$ModelsMax }     elseif ($null -ne $curModelsMax)    { $curModelsMax }    else { 1 }
# ModelsDir falls back: -ModelsDir param → server.ini → %USERPROFILE%\.llama.cpp\models (matches NSIS default).
$modelsDirVal  = if ($PSBoundParameters.ContainsKey('ModelsDir') -and $ModelsDir) { $ModelsDir } `
                 elseif ($cur['ModelsDir']) { $cur['ModelsDir'] } `
                 else { Join-Path $env:USERPROFILE ".llama.cpp\models" }

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
    $modelsMaxVal = Read-IntDefault "Max models loaded simultaneously (--models-max; 0 = unlimited)" $modelsMaxVal -Min 0 -Max 64

    $reply = Read-Host "Models folder (where your .gguf files are stored) [$modelsDirVal]"
    if ($reply) { $modelsDirVal = $reply }
}

# ── Render ───────────────────────────────────────────────────────────

if ($modelsDirVal -and -not (Test-Path -LiteralPath $modelsDirVal)) {
    New-Item -ItemType Directory -Path $modelsDirVal -Force | Out-Null
}

$mlockLit = if ($mlockVal) { 'true' } else { 'false' }

$threadsLine = if ($null -ne $threadsVal) {
    "Threads = $threadsVal"
} else {
    '; Threads = 12  ; optional override; auto-detected if commented'
}

$cacheReuseLine = if ($null -ne $cacheReuseVal) {
    "CacheReuse = $cacheReuseVal"
} else {
    '; CacheReuse = 256  ; minimum chunk size for prompt cache reuse (--cache-reuse)'
}

$threadsBatchLine = if ($null -ne $threadsBatchVal) {
    "ThreadsBatch = $threadsBatchVal"
} else {
    '; ThreadsBatch = 12  ; optional override; auto-detected if commented'
}

# ModelsMax: keep the field out of the file when it matches the runtime
# default (1 = strict one-at-a-time). Surface it as a commented example so the
# user can discover it. Emit a live value only when the user actually changed it.
$modelsMaxLine = if ($modelsMaxVal -eq 1) {
    '; ModelsMax = 2  ; uncomment to allow N models resident at once (0 = unlimited; runtime default if unset: 1)'
} else {
    "ModelsMax = $modelsMaxVal"
}

$content = @"
; Generated by config-server.ps1 on $(Get-Date -Format 'yyyy-MM-dd HH:mm')
;
; llama-server runtime configuration (machine-wide).
; Per-model knobs live in presets.ini, the file consumed by --models-preset.
; Re-run via the "Configure llama-server" Start Menu shortcut.

[Server]
Port = $portVal
Hostname = $hostVal
Mlock = $mlockLit
$threadsLine
$cacheReuseLine
$threadsBatchLine
$modelsMaxLine

; ModelsDir: where .gguf files live; config-model.ps1 scans this when
; building presets.ini, and run-model.ps1 also exports it as LLAMA_CACHE
; so -hf downloads land alongside your local .gguf files.
ModelsDir = $modelsDirVal
"@

[System.IO.File]::WriteAllText($serverPath, $content, [System.Text.UTF8Encoding]::new($false))

if (-not $NonInteractive) {
    Write-Host ""
    Write-Host "Configuration written to:" -ForegroundColor Green
    Write-Host "  $serverPath"
    Write-Host ""
}
