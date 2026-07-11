#requires -Version 7
# Auto-detect paths and generate/update config-build.psd1.
# Run this first to verify your environment is ready.
#
# config-build.psd1 holds build-time settings only (paths, GPU targets, compiler
# flags). Runtime / per-model settings live under %LOCALAPPDATA%\llama.cpp\config\
# and are written by llama-cpp-config on first launch.

param(
    [string]$LlamaCppDir  # path to llama.cpp source. If omitted, defaults to .\build\llama.cpp
)

# ── Detection functions ──────────────────────────────────────────────

function Find-VsDevShell {
    # vswhere first: it knows real product versions, so "newest" is correct
    # across the year-dir → major-version-dir naming switch (2022 vs 18).
    $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path $vswhere) {
        $installPath = & $vswhere -latest -property installationPath 2>$null
        if ($installPath) {
            $script = Join-Path $installPath "Common7\Tools\Launch-VsDevShell.ps1"
            if (Test-Path $script) { return $script }
        }
    }
    # Fallback (no vswhere): scan known VS installation roots, newest first.
    # Folder names mix years (2019, 2022) and major versions (18, …), and a
    # plain string sort puts "2022" above "18" — map years to their major
    # version (2022→17, 2019→16, 2017→15) so the comparison is numeric.
    $roots = @(
        "${env:ProgramFiles}\Microsoft Visual Studio"
        "${env:ProgramFiles(x86)}\Microsoft Visual Studio"
    )
    foreach ($root in $roots) {
        if (-not (Test-Path $root)) { continue }
        $versions = Get-ChildItem $root -Directory | Sort-Object -Descending {
            # NB: inside switch blocks $_ is the switch VALUE (the name string).
            switch ($_.Name) {
                '2022' { 17 } '2019' { 16 } '2017' { 15 }
                default { $v = 0; [void][int]::TryParse($_, [ref]$v); $v }
            }
        }
        foreach ($ver in $versions) {
            $editions = @("Enterprise", "Professional", "Community", "BuildTools")
            foreach ($ed in $editions) {
                $script = Join-Path $ver.FullName "$ed\Common7\Tools\Launch-VsDevShell.ps1"
                if (Test-Path $script) { return $script }
            }
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
    # Pick the latest version folder — numerically ([version]), not as strings:
    # a string sort would rank "7.1" above a future "10.0".
    $latest = Get-ChildItem $base -Directory |
        Sort-Object -Descending { $v = $null; if ([version]::TryParse($_.Name, [ref]$v)) { $v } else { [version]'0.0' } } |
        Select-Object -First 1
    if ($latest -and (Test-Path "$($latest.FullName)\bin")) {
        return $latest.FullName
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

# Activate VS Dev Shell early so tool detection (cmake, clang, ninja) works.
# This duplicates common.ps1's Enable-VsDevShell on purpose: common.ps1 throws
# before build\config-build.psd1 exists, which is exactly when this script runs.
# Keep the vswhere PATH fixup in step with it.
if ($val) {
    Write-Host ""
    Write-Host "  Activating VS Developer Shell..." -ForegroundColor DarkGray
    # vswhere.exe lives in the VS Installer dir, which isn't on PATH by default;
    # without it the spawned VsDevCmd prints "'vswhere.exe' is not recognized".
    $pf86 = ${env:ProgramFiles(x86)}
    if (-not $pf86) { $pf86 = 'C:\Program Files (x86)' }
    $vsInstaller = Join-Path $pf86 'Microsoft Visual Studio\Installer'
    if ((Test-Path (Join-Path $vsInstaller 'vswhere.exe')) -and ($env:PATH -notlike "*$vsInstaller*")) {
        $env:PATH = "$vsInstaller;$env:PATH"
    }
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

# LlamaCppDir (source clone): CLI param → default .\build\llama.cpp
if (-not $LlamaCppDir) {
    $LlamaCppDir = "$PSScriptRoot\build\llama.cpp"
}
$val = (Resolve-Path $LlamaCppDir -ErrorAction SilentlyContinue)?.Path ?? $LlamaCppDir
$detected.LlamaCppDir = $val
Write-Host "  [OK] LlamaCppDir    : $val" -ForegroundColor Green

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

# ── Write config-build.psd1 ──────────────────────────────────────────

function Fmt($val) {
    if ($val -is [bool])   { if ($val) { return '$true' } else { return '$false' } }
    if ($val -is [int])    { return "$val" }
    if ($val -is [double]) { return "$val" }
    if ($val -is [string]) { return "'$($val -replace "'", "''" )'" }
    return "'$val'"
}

$buildLines = [System.Collections.Generic.List[string]]::new()
$buildLines.Add('@{')
$buildLines.Add('    # Paths')
$buildLines.Add("    LlamaCppDir  = $(Fmt $detected.LlamaCppDir)")
$buildLines.Add("    OpenSSLDir   = $(Fmt $detected.OpenSSLDir)")
$buildLines.Add("    HipPath      = $(Fmt $detected.HipPath)")
$buildLines.Add("    VsDevShell   = $(Fmt $detected.VsDevShell)")
$buildLines.Add('')
$buildLines.Add('    # Build settings')
$buildLines.Add('    # GpuTargets = the archs ROCm 7.1 can actually serve on Windows, plus the iGPUs')
$buildLines.Add('    # that merely need to EXIST in the fatbin. Two groups, both required:')
$buildLines.Add('    #  * inference-capable — rocBLAS ships Tensile kernels for exactly these:')
$buildLines.Add('    #    gfx906 (Radeon VII), gfx1030 (RDNA2), gfx110x (RDNA3), gfx115x (Strix/Strix')
$buildLines.Add('    #    Halo), gfx120x (RDNA4). CDNA/Vega (gfx900/908/90a/942/950) is deliberately')
$buildLines.Add('    #    ABSENT: no Windows rocBLAS kernels and no Windows driver for MI cards, so')
$buildLines.Add('    #    building them only cost compile time and ~56 MiB of ggml-hip.dll each.')
$buildLines.Add('    #  * code-object-only (gfx1035/1036/1103/1152/1153, Ryzen iGPUs) — useless for')
$buildLines.Add('    #    inference, but the driver-bundled HIP runtime (amdhip64_7.dll 10.0.3679)')
$buildLines.Add('    #    fails kernel-module load on EVERY visible device when one of them has no')
$buildLines.Add('    #    matching code object: a visible iGPU otherwise breaks the dGPU with')
$buildLines.Add('    #    "device kernel image is invalid". Do not prune them.')
$buildLines.Add("    GpuTargets  = 'gfx906;gfx1030;gfx1035;gfx1036;gfx1100;gfx1101;gfx1102;gfx1103;gfx1150;gfx1151;gfx1152;gfx1153;gfx1200;gfx1201'")
$buildLines.Add("    BuildType   = 'Release'")
$buildLines.Add("    CCompiler   = 'clang'")
$buildLines.Add("    CxxCompiler = 'clang'")
$buildLines.Add("    MarchFlags  = '-march=x86-64-v3'")
$buildLines.Add('    # Parallel build jobs: 3/4 of logical cores, leaving headroom for')
$buildLines.Add('    # interactive use during a long CUDA/HIP compile (0 = use all cores).')
$buildLines.Add("    BuildJobs   = [int]$([math]::Max(1, [math]::Floor([Environment]::ProcessorCount * 3 / 4)))")
$buildLines.Add('}')
$buildDir = Join-Path $PSScriptRoot 'build'
New-Item -ItemType Directory -Path $buildDir -Force | Out-Null
$buildLines | Set-Content -Path (Join-Path $buildDir 'config-build.psd1') -Encoding utf8NoBOM

Write-Host "  build\config-build.psd1 written." -ForegroundColor Green
Write-Host "  Runtime / per-model settings are written on first launch (or by the installer)." -ForegroundColor DarkGray
Write-Host ""

# Non-zero exit when required tools are missing, so callers can script on it.
# The config file is still written above to ease fixing the gaps incrementally.
if ($gaps.Count -gt 0) { exit 1 }
