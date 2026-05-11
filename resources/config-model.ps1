# Per-model configuration for llama.cpp.
#
# Lists .gguf files in the configured ModelsDir, marks already-configured
# models with `*`, lets the user pick one, prompts for every per-model
# parameter (defaulting to the current value or a sane hardcoded default —
# Enter accepts the default, `-` explicitly unsets an optional field), and
# writes %LOCALAPPDATA%\llama.cpp\config\models\<id>.psd1. ActiveModel and
# ModelsDir are then updated in server.psd1 so the runtime knows which to load.
#
# Re-runnable: pick a different model to switch active, or pick the same one
# to refresh its parameters. Configured models are marked with `*` in the list.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$configDir   = Join-Path $env:LOCALAPPDATA "llama.cpp\config"
$serverPath  = Join-Path $configDir "server.psd1"
$modelsDir   = Join-Path $configDir "models"

if (-not (Test-Path $serverPath)) {
    Write-Host ""
    Write-Host "  No server.psd1 at $serverPath" -ForegroundColor Yellow
    Write-Host "  Run config-server.ps1 first to set up llama-server." -ForegroundColor Yellow
    Write-Host ""
    return
}
New-Item -ItemType Directory -Path $modelsDir -Force | Out-Null
$serverCfg = Import-PowerShellDataFile -Path $serverPath

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

# Targeted PSD1 field replace (preserves comments and other fields). Inserts
# before the closing `}` if the field is missing.
function Set-PsdField {
    param([string]$Path, [string]$Field, [string]$Literal)
    $content = Get-Content -Path $Path -Raw -Encoding UTF8
    $pattern = "(?m)^(\s*)$([regex]::Escape($Field))\s*=.*$"
    $newLine = "    $Field = $Literal"
    if ($content -match $pattern) {
        $content = [regex]::Replace($content, $pattern, $newLine)
    } else {
        $content = $content -replace "(?ms)\}\s*$", "$newLine`r`n}"
    }
    Set-Content -Path $Path -Value $content -Encoding utf8NoBOM
}

Write-Host ""
Write-Host "── llama.cpp model configuration ──" -ForegroundColor Cyan
Write-Host ""

# 1) Models directory
$defaultModelsDir = if ($serverCfg.ModelsDir) { $serverCfg.ModelsDir } else { Join-Path $env:USERPROFILE ".cache\llama.cpp" }
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

# 3) Display list with `*` for already-configured models
Write-Host ""
Write-Host "Available models:" -ForegroundColor Cyan
for ($i = 0; $i -lt $models.Count; $i++) {
    $sizeGb  = [math]::Round($models[$i].Length / 1GB, 2)
    $relPath = $models[$i].FullName.Substring($ggufDir.Length).TrimStart('\', '/')
    $id      = Get-ModelId $models[$i].FullName
    $marker  = if (Test-Path (Join-Path $modelsDir "$id.psd1")) { '*' } else { ' ' }
    Write-Host ("  [{0,2}]{1} {2}  ({3} GB)" -f ($i + 1), $marker, $relPath, $sizeGb)
}
Write-Host ""
Write-Host "  (* = already configured)" -ForegroundColor DarkGray

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

$modelId      = Get-ModelId $selected.FullName
$modelCfgPath = Join-Path $modelsDir "$modelId.psd1"
$cur = if (Test-Path $modelCfgPath) { Import-PowerShellDataFile -Path $modelCfgPath } else { @{} }

# 5) Prompt for all per-model parameters (Enter = default, `-` = unset)
Write-Host ""
Write-Host "Selected: $($selected.FullName)" -ForegroundColor Green
Write-Host "Press Enter to accept the default; type '-' to unset an optional field." -ForegroundColor DarkGray
Write-Host ""

$ctxSize           = Read-IntDefault    "Context size (tokens, --ctx-size)" $(if ($cur.CtxSize) { $cur.CtxSize } else { 32768 }) -Min 512 -Max 8388608
$gpuLayers         = Read-IntDefault    "GPU layers (99 = all, --n-gpu-layers)" $(if ($null -ne $cur.GpuLayers) { $cur.GpuLayers } else { 99 }) -Min 0 -Max 999
$parallel          = Read-IntDefault    "Parallel decoding seqs (-np)" $(if ($cur.Parallel) { $cur.Parallel } else { 4 }) -Min 1 -Max 64
$batchSize          = Read-IntDefault    "Batch size (--batch-size)" $(if ($cur.BatchSize) { $cur.BatchSize } else { 512 }) -Min 1 -Max 8192
$ubatchSize         = Read-IntDefault    "Ubatch size (--ubatch-size)" $(if ($cur.UbatchSize) { $cur.UbatchSize } else { 512 }) -Min 1 -Max 8192
$cacheTypeK         = Read-EnumDefault   "K-cache quantization (--cache-type-k)" $(if ($cur.CacheTypeK) { $cur.CacheTypeK } else { 'q8_0' }) @('f32','f16','q8_0','q5_0','q4_0','q4_1')
$cacheTypeV         = Read-EnumDefault   "V-cache quantization (--cache-type-v)" $(if ($cur.CacheTypeV) { $cur.CacheTypeV } else { 'q8_0' }) @('f32','f16','q8_0','q5_0','q4_0','q4_1')
$flashAttn          = Read-BoolDefault   "Flash Attention (-fa)" $(if ($null -ne $cur.FlashAttn) { $cur.FlashAttn } else { $true })
$jinja             = Read-BoolDefault   "Use embedded chat template (--jinja)" $(if ($null -ne $cur.Jinja) { $cur.Jinja } else { $true })
$nCpuMoe           = Read-IntDefault    "Expert layers on CPU (MoE only, --n-cpu-moe)" $cur.NCpuMoe -Min 0 -Max 999 -AllowUnset
$temp              = Read-FloatDefault  "Sampling temperature (--temp)" $cur.Temp -AllowUnset
$topK              = Read-IntDefault    "Top-K (--top-k)" $cur.TopK -Min 0 -Max 1000 -AllowUnset
$topP              = Read-FloatDefault  "Top-P (--top-p)" $cur.TopP -AllowUnset
$repeatPenalty     = Read-FloatDefault  "Repeat penalty (--repeat-penalty)" $cur.RepeatPenalty -AllowUnset
$presencePenalty   = Read-FloatDefault  "Presence penalty (--presence-penalty)" $cur.PresencePenalty -AllowUnset
$chatTemplateKwargs= Read-StringDefault "Chat template kwargs (JSON, --chat-template-kwargs)" $cur.ChatTemplateKwargs -AllowUnset

# 6) Render psd1 — set fields are emitted as live; unset optional fields are
#    emitted as commented examples so the user can spot them later.
function Format-Field {
    param([string]$Name, $Value, [string]$Default, [int]$NamePad = 18)
    $padded = $Name.PadRight($NamePad)
    if ($null -eq $Value) {
        return "    # $padded = $Default"
    }
    return "    $padded = $Value"
}

$modelPathEsc = $selected.FullName -replace "'", "''"
$jinjaLit     = if ($jinja)     { '$true' } else { '$false' }
$flashLit     = if ($flashAttn) { '$true' } else { '$false' }
$cacheKEsc    = $cacheTypeK -replace "'", "''"
$cacheVEsc    = $cacheTypeV -replace "'", "''"
$ctkEsc       = if ($chatTemplateKwargs) { ($chatTemplateKwargs -replace "'", "''") } else { $null }
$ctkLit       = if ($ctkEsc) { "'$ctkEsc'" } else { $null }

$body = @()
$body += "    # Generated by config-model.ps1 on $(Get-Date -Format 'yyyy-MM-dd HH:mm')"
$body += ''
$body += "    # Model: local path (-m) or HF spec ('user/repo[:tag]', triggers -hf)"
$body += "    Model              = '$modelPathEsc'"
$body += ''
$body += '    # ── Resource / context (model-dependent: ctx max, VRAM cost) ───────'
$body += "    CtxSize            = $ctxSize"
$body += "    GpuLayers          = $gpuLayers"
$body += "    Parallel           = $parallel"
$body += (Format-Field 'BatchSize'          $batchSize          '512')
$body += (Format-Field 'UbatchSize'         $ubatchSize         '512')
$body += ''
$body += "    CacheTypeK         = '$cacheKEsc'"
$body += "    CacheTypeV         = '$cacheVEsc'"
$body += "    FlashAttn          = $flashLit"
$body += ''
$body += "    # Use the chat template embedded in the GGUF"
$body += "    Jinja              = $jinjaLit"
$body += ''
$body += '    # ── MoE-only: expert layers kept on CPU (--n-cpu-moe) ──────────────'
$body += (Format-Field 'NCpuMoe' $nCpuMoe '0')
$body += ''
$body += '    # ── Sampling overrides ─────────────────────────────────────────────'
$body += (Format-Field 'Temp'            $temp            '0.7')
$body += (Format-Field 'TopK'            $topK            '20')
$body += (Format-Field 'TopP'            $topP            '0.95')
$body += (Format-Field 'RepeatPenalty'   $repeatPenalty   '1.05')
$body += (Format-Field 'PresencePenalty' $presencePenalty '1.5')
$body += ''
$body += '    # ── Chat template kwargs (e.g. enable_thinking for Qwen3) ──────────'
$body += (Format-Field 'ChatTemplateKwargs' $ctkLit "'{`"enable_thinking`":true}'")

$content = "@{`r`n" + ($body -join "`r`n") + "`r`n}`r`n"
Set-Content -Path $modelCfgPath -Value $content -Encoding utf8NoBOM

# 7) Update server.psd1 pointers
$ggufDirEsc = $ggufDir -replace "'", "''"
Set-PsdField $serverPath 'ModelsDir'   "'$ggufDirEsc'"
Set-PsdField $serverPath 'ActiveModel' "'$modelId'"

Write-Host ""
Write-Host "Active model: $modelId" -ForegroundColor Green
Write-Host "  Config: $modelCfgPath"
Write-Host "  Edit it directly to tweak values without re-running this script."
Write-Host ""
