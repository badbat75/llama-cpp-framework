# Per-model preset builder for llama.cpp router mode.
#
# Operates directly on %LOCALAPPDATA%\llama.cpp\config\presets.ini — the file
# consumed by `llama-server --models-preset`. Each [section] in the INI is one
# preset; the section name is the id clients pass as the OpenAI "model" field.
#
# This script edits exactly one section at a time. Other sections in the file —
# including any custom keys you've hand-added, comments, and ordering — are
# preserved byte-for-byte. The section being edited is rewritten in full from
# the wizard's answers, so any custom keys IN THAT SECTION will be lost; if you
# want exotic flags on a model, set them up via the wizard first, then add the
# exotic keys by hand and don't re-run the wizard for that model.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

. (Join-Path $PSScriptRoot "common-functions.ps1")

$configDir   = Join-Path $env:LOCALAPPDATA "llama.cpp\config"
$serverPath  = Join-Path $configDir "server.ini"
$presetsPath = Join-Path $configDir "presets.ini"

if (-not (Test-Path $serverPath)) {
    Write-Host ""
    Write-Host "  No server.ini at $serverPath" -ForegroundColor Yellow
    Write-Host "  Run config-server.ps1 first to set up llama-server." -ForegroundColor Yellow
    Write-Host ""
    return
}
New-Item -ItemType Directory -Path $configDir -Force | Out-Null
$serverCfg = Read-ServerIni -Path $serverPath

# ── Prompt helpers (Enter = default; `-` = unset for optional fields) ──

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
        Write-Host "  Invalid value (expected $Min-$Max$(if ($AllowUnset) { ' or `-` to unset' }))." -ForegroundColor Yellow
    }
}

function Read-FloatDefault {
    param([string]$Prompt, $Default, [switch]$AllowUnset)
    while ($true) {
        $shown = if ($null -eq $Default) { 'unset' } else { "$Default" }
        $reply = Read-Host "$Prompt [$shown]"
        if (-not $reply) { return $Default }
        if ($AllowUnset -and $reply -eq '-') { return $null }
        [double]$parsed = 0
        if ([double]::TryParse($reply, [System.Globalization.NumberStyles]::Float, [System.Globalization.CultureInfo]::InvariantCulture, [ref]$parsed)) {
            return $parsed
        }
        Write-Host "  Invalid number." -ForegroundColor Yellow
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

function Read-StringDefault {
    param([string]$Prompt, $Default, [switch]$AllowUnset)
    $shown = if (-not $Default) { 'unset' } else { $Default }
    $reply = Read-Host "$Prompt [$shown]"
    if (-not $reply) { return $Default }
    if ($AllowUnset -and $reply -eq '-') { return $null }
    return $reply
}

function Read-EnumDefault {
    param([string]$Prompt, [string]$Default, [string[]]$Choices)
    $list = $Choices -join '/'
    while ($true) {
        $reply = Read-Host "$Prompt ($list) [$Default]"
        if (-not $reply) { return $Default }
        if ($Choices -contains $reply) { return $reply }
        Write-Host "  Invalid (one of: $list)." -ForegroundColor Yellow
    }
}

# Stable filesystem-safe ID per model: basename without extension, without
# multi-shard suffix, with non-alphanumerics collapsed to underscores.
function Get-ModelId {
    param([string]$ModelPath)
    $base = [System.IO.Path]::GetFileNameWithoutExtension($ModelPath)
    $base = $base -replace '-\d{5}-of-\d{5}$', ''
    return ($base -replace '[^a-zA-Z0-9._-]+', '_')
}

# ── INI section parsing ──────────────────────────────────────────────
# Returns each section as { Id; Text } where Text is the verbatim slice from
# `[id]` up to (but not including) the next section header. Used both to
# detect which models are already configured (for the `*` marker) and to
# pre-fill prompt defaults from the active section.

function Get-IniSections {
    param([string]$Path)
    if (-not (Test-Path $Path)) { return @() }
    $text = Get-Content -Path $Path -Raw -Encoding UTF8
    if (-not $text) { return @() }
    $sections = @()
    foreach ($m in [regex]::Matches($text, '(?m)^\[(?<id>[^\]\r\n]+)\][\s\S]*?(?=^\[|\z)')) {
        $sections += [pscustomobject]@{
            Id    = $m.Groups['id'].Value.Trim()
            Text  = $m.Value
            Index = $m.Index
            Length = $m.Length
        }
    }
    return $sections
}

function Get-IniSectionKeys {
    param([pscustomobject]$Section)
    $result = @{}
    if (-not $Section) { return $result }
    foreach ($line in ($Section.Text -split "(?:\r\n|\n)")) {
        $t = $line.Trim()
        if ($t -eq '' -or $t.StartsWith(';') -or $t.StartsWith('#') -or $t.StartsWith('[')) { continue }
        if ($t -match '^([^=]+?)\s*=\s*(.*)$') {
            $result[$Matches[1].Trim()] = $Matches[2].Trim()
        }
    }
    return $result
}

Write-Host ""
Write-Host "── llama.cpp model preset ──" -ForegroundColor Cyan
Write-Host ""

# 1) Models directory
$defaultModelsDir = if ($serverCfg['ModelsDir']) { $serverCfg['ModelsDir'] } else { Join-Path $env:USERPROFILE ".cache\llama.cpp" }
$ggufDir = $null
while (-not $ggufDir) {
    $reply = Read-Host "Models directory [$defaultModelsDir]"
    if (-not $reply) { $reply = $defaultModelsDir }
    if (Test-Path $reply -PathType Container) {
        $ggufDir = (Resolve-Path $reply).Path
    } else {
        Write-Host "  Directory not found. Try again or Ctrl+C to abort." -ForegroundColor Yellow
    }
}

# 2) Scan .gguf files (recursive). For multi-shard models, only show the first
#    shard — llama-server auto-loads the rest.
Write-Host ""
Write-Host "Scanning $ggufDir for .gguf models..." -ForegroundColor DarkGray
$models = Get-ChildItem -Path $ggufDir -Filter "*.gguf" -Recurse -File `
    | Where-Object { $_.Name -notmatch '-\d{5}-of-\d{5}\.gguf$' -or $_.Name -match '-00001-of-\d{5}\.gguf$' } `
    | Sort-Object FullName

if ($models.Count -eq 0) {
    throw "No .gguf files found under $ggufDir."
}

# 3) Read existing presets so already-configured models get a `*` marker
$sections = @(Get-IniSections -Path $presetsPath)
$configuredIds = @($sections | ForEach-Object { $_.Id })

Write-Host ""
Write-Host "Available models:" -ForegroundColor Cyan
for ($i = 0; $i -lt $models.Count; $i++) {
    $sizeGb  = [math]::Round($models[$i].Length / 1GB, 2)
    $relPath = $models[$i].FullName.Substring($ggufDir.Length).TrimStart('\', '/')
    $id      = Get-ModelId $models[$i].FullName
    $marker  = if ($configuredIds -contains $id) { '*' } else { ' ' }
    Write-Host ("  [{0,2}]{1} {2}  ({3} GB)" -f ($i + 1), $marker, $relPath, $sizeGb)
}
Write-Host ""
Write-Host "  (* = already a preset section in presets.ini)" -ForegroundColor DarkGray

# 4) Selection
$selected = $null
while (-not $selected) {
    $reply = Read-Host "`nSelect model [1-$($models.Count)]"
    [int]$idx = 0
    if ([int]::TryParse($reply, [ref]$idx) -and $idx -ge 1 -and $idx -le $models.Count) {
        $selected = $models[$idx - 1]
    } else {
        Write-Host "  Invalid selection." -ForegroundColor Yellow
    }
}

$modelId = Get-ModelId $selected.FullName
$existingSection = $sections | Where-Object { $_.Id -eq $modelId } | Select-Object -First 1
$cur = Get-IniSectionKeys -Section $existingSection

# Convert string defaults read from INI into typed values for the prompts.
function ConvertTo-IntOrNull   { param($v) if ($v) { $p=0; if ([int]::TryParse($v, [ref]$p))    { return $p } } return $null }
function ConvertTo-FloatOrNull { param($v) if ($v) { $p=[double]0; if ([double]::TryParse($v, [System.Globalization.NumberStyles]::Float, [System.Globalization.CultureInfo]::InvariantCulture, [ref]$p)) { return $p } } return $null }
function ConvertTo-BoolOrNull  { param($v) if ($v -eq 'true') { $true } elseif ($v -eq 'false') { $false } else { $null } }
function ConvertTo-FlashOrNull { param($v) if ($v -eq 'true') { $true } elseif ($v -eq 'false') { $false } else { $null } }

$curCtx          = ConvertTo-IntOrNull   $cur['ctx-size']
$curGpuLayers    = ConvertTo-IntOrNull   $cur['n-gpu-layers']
$curParallel     = ConvertTo-IntOrNull   $cur['parallel']
$curBatchSize    = ConvertTo-IntOrNull   $cur['batch-size']
$curUbatchSize   = ConvertTo-IntOrNull   $cur['ubatch-size']
$curCacheK       = if ($cur.ContainsKey('cache-type-k')) { $cur['cache-type-k'] } else { $null }
$curCacheV       = if ($cur.ContainsKey('cache-type-v')) { $cur['cache-type-v'] } else { $null }
$curFlash        = ConvertTo-FlashOrNull $cur['flash-attn']
$curJinja        = ConvertTo-BoolOrNull  $cur['jinja']
$curReasoning    = if ($cur.ContainsKey('reasoning-format')) { $cur['reasoning-format'] } else { $null }
$curNCpuMoe      = ConvertTo-IntOrNull   $cur['n-cpu-moe']
$curTemp         = ConvertTo-FloatOrNull $cur['temp']
$curTopK         = ConvertTo-IntOrNull   $cur['top-k']
$curTopP         = ConvertTo-FloatOrNull $cur['top-p']
$curMinP         = ConvertTo-FloatOrNull $cur['min-p']
$curRepeatPen    = ConvertTo-FloatOrNull $cur['repeat-penalty']
$curPresencePen  = ConvertTo-FloatOrNull $cur['presence-penalty']
$curChatKwargs   = if ($cur.ContainsKey('chat-template-kwargs')) { $cur['chat-template-kwargs'] } else { $null }

# 5) Prompt for all per-model parameters
Write-Host ""
Write-Host "Selected: $($selected.FullName)" -ForegroundColor Green
Write-Host "Preset id (sent as `"model`" in API requests): $modelId" -ForegroundColor Green
Write-Host "Press Enter to accept the default; type '-' to unset an optional field." -ForegroundColor DarkGray
Write-Host ""

$ctxSize           = Read-IntDefault    "Context size (tokens, --ctx-size)" $(if ($curCtx) { $curCtx } else { 32768 }) -Min 512 -Max 8388608
$gpuLayers         = Read-IntDefault    "GPU layers (99 = all, --n-gpu-layers)" $(if ($null -ne $curGpuLayers) { $curGpuLayers } else { 99 }) -Min 0 -Max 999
$parallel          = Read-IntDefault    "Parallel decoding seqs (-np)" $(if ($curParallel) { $curParallel } else { 4 }) -Min 1 -Max 64
$batchSize         = Read-IntDefault    "Batch size (--batch-size)" $(if ($curBatchSize) { $curBatchSize } else { 512 }) -Min 1 -Max 8192
$ubatchSize        = Read-IntDefault    "Ubatch size (--ubatch-size)" $(if ($curUbatchSize) { $curUbatchSize } else { 512 }) -Min 1 -Max 8192
$cacheTypeK        = Read-EnumDefault   "K-cache quantization (--cache-type-k)" $(if ($curCacheK) { $curCacheK } else { 'q8_0' }) @('f32','f16','q8_0','q5_0','q4_0','q4_1')
$cacheTypeV        = Read-EnumDefault   "V-cache quantization (--cache-type-v)" $(if ($curCacheV) { $curCacheV } else { 'q8_0' }) @('f32','f16','q8_0','q5_0','q4_0','q4_1')
$flashAttn         = Read-BoolDefault   "Flash Attention (-fa)" $(if ($null -ne $curFlash) { $curFlash } else { $true })
$jinja             = Read-BoolDefault   "Use embedded chat template (--jinja)" $(if ($null -ne $curJinja) { $curJinja } else { $true })
$reasoningFormat   = Read-EnumDefault   "Reasoning format (--reasoning-format)" $(if ($curReasoning) { $curReasoning } else { 'auto' }) @('auto','none','deepseek')
$nCpuMoe           = Read-IntDefault    "Expert layers on CPU (MoE only, --n-cpu-moe)" $curNCpuMoe -Min 0 -Max 999 -AllowUnset
$temp              = Read-FloatDefault  "Sampling temperature (--temp)" $curTemp -AllowUnset
$topK              = Read-IntDefault    "Top-K (--top-k)" $curTopK -Min 0 -Max 1000 -AllowUnset
$topP              = Read-FloatDefault  "Top-P (--top-p)" $curTopP -AllowUnset
$minP              = Read-FloatDefault  "Min-P (--min-p, Qwen wants 0.0; llama.cpp default is 0.05)" $curMinP -AllowUnset
$repeatPenalty     = Read-FloatDefault  "Repeat penalty (--repeat-penalty)" $curRepeatPen -AllowUnset
$presencePenalty   = Read-FloatDefault  "Presence penalty (--presence-penalty)" $curPresencePen -AllowUnset
$chatTemplateKwargs= Read-StringDefault "Chat template kwargs (JSON, --chat-template-kwargs)" $curChatKwargs -AllowUnset

# 6) Build the new section text. Set values are emitted as live keys; unset
#    optional values are emitted as commented placeholders so the user can
#    discover them later.
function Emit-Setting {
    param([System.Text.StringBuilder]$Sb, [string]$Key, $Value, [string]$Example = $null)
    if ($null -eq $Value -or ($Value -is [string] -and $Value -eq '')) {
        if ($Example) { [void]$Sb.AppendLine("; $Key = $Example") }
        return
    }
    [void]$Sb.AppendLine("$Key = $Value")
}
function Emit-Bool {
    param([System.Text.StringBuilder]$Sb, [string]$Key, $Value)
    if ($null -eq $Value) { return }
    [void]$Sb.AppendLine("$Key = $(if ($Value) { 'true' } else { 'false' })")
}

$sb = [System.Text.StringBuilder]::new()
[void]$sb.AppendLine("[$modelId]")
[void]$sb.AppendLine("; Generated by config-model.ps1 on $(Get-Date -Format 'yyyy-MM-dd HH:mm').")
[void]$sb.AppendLine('; Re-running the wizard rewrites this section; hand-edits to OTHER sections')
[void]$sb.AppendLine('; in this file are preserved. To add exotic llama.cpp flags, edit by hand and')
[void]$sb.AppendLine('; do not re-run the wizard for this model.')
[void]$sb.AppendLine('')
[void]$sb.AppendLine('; Model: local path (-m). For Hugging Face downloads use `hf-repo = user/repo[:tag]` instead.')
[void]$sb.AppendLine("model = $($selected.FullName)")
[void]$sb.AppendLine('')
[void]$sb.AppendLine('; Resource / context (model-dependent: ctx max, VRAM cost)')
Emit-Setting $sb 'ctx-size'     $ctxSize
Emit-Setting $sb 'n-gpu-layers' $gpuLayers
Emit-Setting $sb 'parallel'     $parallel
Emit-Setting $sb 'batch-size'   $batchSize
Emit-Setting $sb 'ubatch-size'  $ubatchSize
[void]$sb.AppendLine('')
[void]$sb.AppendLine('; KV cache')
Emit-Setting $sb 'cache-type-k' $cacheTypeK
Emit-Setting $sb 'cache-type-v' $cacheTypeV
Emit-Bool $sb 'flash-attn' $flashAttn
[void]$sb.AppendLine('')
[void]$sb.AppendLine('; Chat template embedded in the GGUF')
Emit-Bool $sb 'jinja' $jinja
[void]$sb.AppendLine('')
[void]$sb.AppendLine('; Reasoning format: auto | none | deepseek')
Emit-Setting $sb 'reasoning-format' $reasoningFormat
[void]$sb.AppendLine('')
[void]$sb.AppendLine('; MoE-only: expert layers kept on CPU')
Emit-Setting $sb 'n-cpu-moe' $nCpuMoe '0'
[void]$sb.AppendLine('')
[void]$sb.AppendLine('; Sampling overrides')
Emit-Setting $sb 'temp'             $temp           '0.7'
Emit-Setting $sb 'top-k'            $topK           '20'
Emit-Setting $sb 'top-p'            $topP           '0.95'
Emit-Setting $sb 'min-p'            $minP           '0.0'
Emit-Setting $sb 'repeat-penalty'   $repeatPenalty  '1.05'
Emit-Setting $sb 'presence-penalty' $presencePenalty '1.5'
[void]$sb.AppendLine('')
[void]$sb.AppendLine('; Chat template kwargs (e.g. enable_thinking for Qwen3)')
Emit-Setting $sb 'chat-template-kwargs' $chatTemplateKwargs '{"enable_thinking":true}'

$newSection = $sb.ToString().TrimEnd("`r", "`n") + "`r`n"

# 7) Section-preserving write: replace existing [modelId] section, or append
#    a new one, leaving the rest of the file untouched.
$existingText = if (Test-Path $presetsPath) { Get-Content -Path $presetsPath -Raw -Encoding UTF8 } else { '' }
if (-not $existingText) { $existingText = '' }

$escapedId = [regex]::Escape($modelId)
$existingMatch = [regex]::Match($existingText, "(?m)^\[$escapedId\][\s\S]*?(?=^\[|\z)")
if ($existingMatch.Success) {
    $before = $existingText.Substring(0, $existingMatch.Index)
    $after  = $existingText.Substring($existingMatch.Index + $existingMatch.Length)
    # If another section follows, insert a blank-line separator before it.
    $sep    = if ($after -ne '') { "`r`n" } else { '' }
    $newText = $before + $newSection + $sep + $after
} else {
    if ($existingText.Length -gt 0) {
        $existingText = $existingText.TrimEnd("`r", "`n") + "`r`n`r`n"
    }
    $newText = $existingText + $newSection
}

[System.IO.File]::WriteAllText($presetsPath, $newText, [System.Text.UTF8Encoding]::new($false))

# 8) Update server.ini's ModelsDir pointer
Set-ServerIniField -Path $serverPath -Key 'ModelsDir' -Value $ggufDir

Write-Host ""
if ($existingMatch.Success) {
    Write-Host "Updated preset: $modelId" -ForegroundColor Green
} else {
    Write-Host "Added preset: $modelId" -ForegroundColor Green
}
Write-Host "  File: $presetsPath"
Write-Host "  Edit it directly to tweak values, add exotic flags, or remove a preset."
Write-Host "  Clients select this preset by passing  '`"model`": `"$modelId`"'  in API requests."
Write-Host ""
