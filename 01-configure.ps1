# Auto-detect paths and generate/update config.psd1
# Run this first to verify your environment is ready.

param(
    [string]$LlamaCppDir  # path to llama.cpp source. If omitted, defaults to .\build
)

$configPath = "$PSScriptRoot\config.psd1"

# ── Detection functions ──────────────────────────────────────────────

function Find-VsDevShell {
    # Search known VS installation roots, newest first
    $roots = @(
        "${env:ProgramFiles}\Microsoft Visual Studio"
        "${env:ProgramFiles(x86)}\Microsoft Visual Studio"
    )
    foreach ($root in $roots) {
        if (-not (Test-Path $root)) { continue }
        # Sort version folders descending (18, 17, 2022, 2019...)
        $versions = Get-ChildItem $root -Directory | Sort-Object Name -Descending
        foreach ($ver in $versions) {
            $editions = @("Enterprise", "Professional", "Community", "BuildTools")
            foreach ($ed in $editions) {
                $script = Join-Path $ver.FullName "$ed\Common7\Tools\Launch-VsDevShell.ps1"
                if (Test-Path $script) { return $script }
            }
        }
    }
    # Fallback: vswhere
    $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path $vswhere) {
        $installPath = & $vswhere -latest -property installationPath 2>$null
        if ($installPath) {
            $script = Join-Path $installPath "Common7\Tools\Launch-VsDevShell.ps1"
            if (Test-Path $script) { return $script }
        }
    }
    return $null
}

function Find-OpenSSL {
    $candidates = @(
        "${env:ProgramFiles}\OpenSSL-Win64"
        "${env:ProgramW6432}\OpenSSL-Win64"
        "C:\OpenSSL-Win64"
    )
    foreach ($p in $candidates) {
        if (Test-Path "$p\include\openssl\ssl.h") { return $p }
    }
    # Check if openssl.exe is in PATH
    $exe = Get-Command openssl -ErrorAction SilentlyContinue
    if ($exe) {
        $dir = Split-Path (Split-Path $exe.Source)
        if (Test-Path "$dir\include\openssl\ssl.h") { return $dir }
    }
    return $null
}

function Find-ROCm {
    # Check HIP_PATH env first
    if ($env:HIP_PATH -and (Test-Path "$env:HIP_PATH\bin\hipcc.exe")) {
        return $env:HIP_PATH
    }
    $base = "${env:ProgramFiles}\AMD\ROCm"
    if (-not (Test-Path $base)) { return $null }
    # Pick the latest version folder
    $latest = Get-ChildItem $base -Directory | Sort-Object Name -Descending | Select-Object -First 1
    if ($latest -and (Test-Path "$($latest.FullName)\bin")) {
        return $latest.FullName
    }
    return $null
}


function Find-CacheDir {
    # Prefer LLAMA_CACHE env, then common locations
    if ($env:LLAMA_CACHE -and (Test-Path $env:LLAMA_CACHE)) {
        return $env:LLAMA_CACHE
    }
    $candidates = @(
        "E:\llama.cpp\models"
        "D:\llama.cpp\models"
        "$env:USERPROFILE\.cache\llama.cpp"
    )
    foreach ($p in $candidates) {
        if (Test-Path $p) { return $p }
    }
    return $null
}

function Find-Tool([string]$Name) {
    $cmd = Get-Command $Name -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    # Fallback: check HIP_PATH\bin (ROCm ships its own clang, cmake, etc.)
    if ($detected.HipPath) {
        $hipBin = Join-Path $detected.HipPath "bin\$Name.exe"
        if (Test-Path $hipBin) { return $hipBin }
    }
    return $null
}

# ── Run detection ────────────────────────────────────────────────────

Write-Host ""
Write-Host "  llama.cpp-framework — Environment Check" -ForegroundColor Cyan
Write-Host "  ========================================" -ForegroundColor Cyan
Write-Host ""

$detected = [ordered]@{}
$gaps     = @()

# --- Paths ---

$val = Find-VsDevShell
$detected.VsDevShell = $val
if ($val) { Write-Host "  [OK] VsDevShell     : $val" -ForegroundColor Green }
else      { Write-Host "  [!!] VsDevShell     : NOT FOUND" -ForegroundColor Red; $gaps += "VsDevShell — Install Visual Studio with C++ workload" }

# Activate VS Dev Shell early so tool detection (cmake, clang, ninja) works
if ($val) {
    Write-Host ""
    Write-Host "  Activating VS Developer Shell..." -ForegroundColor DarkGray
    $prevDir = Get-Location
    & $val -Arch amd64
    Set-Location $prevDir
}

$val = Find-OpenSSL
$detected.OpenSSLDir = $val
if ($val) { Write-Host "  [OK] OpenSSLDir     : $val" -ForegroundColor Green }
else      { Write-Host "  [!!] OpenSSLDir     : NOT FOUND" -ForegroundColor Red; $gaps += "OpenSSLDir — Run: winget install OpenSSL" }

$val = Find-ROCm
$detected.HipPath = $val
if ($val) { Write-Host "  [OK] HipPath        : $val" -ForegroundColor Green }
else      { Write-Host "  [--] HipPath        : not found (optional, needed for HIP/ROCm)" -ForegroundColor Yellow }

# LlamaCppDir: CLI param → default .\build
if (-not $LlamaCppDir) {
    $LlamaCppDir = "$PSScriptRoot\build"
}
$val = (Resolve-Path $LlamaCppDir -ErrorAction SilentlyContinue)?.Path ?? $LlamaCppDir
$detected.LlamaCppDir = $val
Write-Host "  [OK] LlamaCppDir    : $val" -ForegroundColor Green

$val = Find-CacheDir
$detected.CacheDir = $val
if ($val) { Write-Host "  [OK] CacheDir       : $val" -ForegroundColor Green }
else      { Write-Host "  [--] CacheDir       : not found (will use default LLAMA_CACHE)" -ForegroundColor Yellow }

Write-Host ""

# --- Tools (detected AFTER VS Dev Shell activation) ---

Write-Host "  Tools" -ForegroundColor Cyan
Write-Host "  -----" -ForegroundColor Cyan

$tools = [ordered]@{
    cmake = "CMake — https://cmake.org/download/"
    ninja = "Ninja — winget install Ninja-build.Ninja"
    clang = "Clang — install via Visual Studio or LLVM"
    git   = "Git — https://git-scm.com/"
}

foreach ($tool in $tools.Keys) {
    $found = Find-Tool $tool
    if ($found) { Write-Host "  [OK] $($tool.PadRight(14)): $found" -ForegroundColor Green }
    else        { Write-Host "  [!!] $($tool.PadRight(14)): NOT FOUND" -ForegroundColor Red; $gaps += "$tool — $($tools[$tool])" }
}

# Check CUDA (nvcc)
$nvcc = Find-Tool "nvcc"
if ($nvcc) { Write-Host "  [OK] nvcc (CUDA)    : $nvcc" -ForegroundColor Green }
else       { Write-Host "  [--] nvcc (CUDA)    : not found (optional, needed for CUDA)" -ForegroundColor Yellow }

Write-Host ""

# ── Summary ──────────────────────────────────────────────────────────

if ($gaps.Count -gt 0) {
    Write-Host "  Gaps found ($($gaps.Count)):" -ForegroundColor Red
    foreach ($g in $gaps) {
        Write-Host "    - $g" -ForegroundColor Red
    }
    Write-Host ""
}
else {
    Write-Host "  All required dependencies found!" -ForegroundColor Green
    Write-Host ""
}

# ── Write config.psd1 ────────────────────────────────────────────────

# Build config as an ordered list of lines (avoids interpolation/escaping issues)
$lines = [System.Collections.Generic.List[string]]::new()
$lines.Add('@{')

# Helper: format a value for .psd1
function Fmt($val) {
    if ($val -is [bool])   { if ($val) { return '$true' } else { return '$false' } }
    if ($val -is [int])    { return "$val" }
    if ($val -is [string]) { return "'$($val -replace "'", "''" )'" }
    return "'$val'"
}

# Paths: use detected values, fall back to placeholder
$paths = [ordered]@{
    LlamaCppDir = @{ Val = $detected.LlamaCppDir; Placeholder = 'TODO: path to llama.cpp' }
    OpenSSLDir  = @{ Val = $detected.OpenSSLDir;  Placeholder = 'TODO: path to OpenSSL-Win64' }
    HipPath     = @{ Val = $detected.HipPath;     Placeholder = 'TODO: path to ROCm' }
    VsDevShell  = @{ Val = $detected.VsDevShell;  Placeholder = 'TODO: path to Launch-VsDevShell.ps1' }
    CacheDir    = @{ Val = $detected.CacheDir;    Placeholder = 'TODO: path to model cache' }
}

$lines.Add('    # Paths')
foreach ($key in $paths.Keys) {
    $v = if ($paths[$key].Val) { $paths[$key].Val } else { $paths[$key].Placeholder }
    $lines.Add("    $($key.PadRight(12)) = $(Fmt $v)")
}

# Non-path settings with defaults
$settings = [ordered]@{
    GpuTargets  = "gfx900;gfx906;gfx908;gfx90a;gfx942;gfx950;gfx1030;gfx1100;gfx1101;gfx1102;gfx1200;gfx1201"
    BuildType   = "Release"
    CCompiler   = "clang"
    CxxCompiler = "clang"
    MarchFlags  = "-march=x86-64-v3"
    BuildJobs   = [int]0
    Model       = "unsloth/Qwen3.5-35B-A3B-GGUF:UD-Q4_K_XL"
    Port        = [int]8080
    CtxSize     = [int]65536
    GpuLayers   = [int]99
    Parallel    = [int]1
    CacheTypeK  = "q8_0"
    CacheTypeV  = "q8_0"
    FlashAttn   = $true
    Jinja       = $true
}

# Preserve existing non-path settings if config already exists
if (Test-Path $configPath) {
    try {
        $existing = Import-PowerShellDataFile $configPath
        foreach ($key in @($settings.Keys)) {
            if ($existing.ContainsKey($key)) {
                $settings[$key] = $existing[$key]
            }
        }
    } catch {
        Write-Host "  [--] Could not read existing config, using defaults" -ForegroundColor Yellow
    }
}

$lines.Add('')
$lines.Add('    # Build settings')
foreach ($key in @('GpuTargets','BuildType','CCompiler','CxxCompiler','MarchFlags','BuildJobs')) {
    $lines.Add("    $($key.PadRight(12)) = $(Fmt $settings[$key])")
}

$lines.Add('')
$lines.Add('    # Runtime settings')
foreach ($key in @('Model','Port','CtxSize','GpuLayers','Parallel','CacheTypeK','CacheTypeV','FlashAttn','Jinja')) {
    $lines.Add("    $($key.PadRight(12)) = $(Fmt $settings[$key])")
}

$lines.Add('}')

$content = $lines -join "`r`n"
Set-Content -Path $configPath -Value $content -Encoding UTF8
Write-Host "  config.psd1 written to: $configPath" -ForegroundColor Green

# Show any TODO placeholders
$todos = (Select-String -Path $configPath -Pattern "TODO:" -SimpleMatch)
if ($todos) {
    Write-Host ""
    Write-Host "  Fill in these placeholders in config.psd1:" -ForegroundColor Yellow
    foreach ($t in $todos) {
        Write-Host "    $($t.Line.Trim())" -ForegroundColor Yellow
    }
}

Write-Host ""
