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
    # Check HIP_PATH env first — process env, then machine env (a console opened
    # before 00-install-prerequisites.ps1 set the machine vars has a stale copy).
    foreach ($hp in @($env:HIP_PATH, [Environment]::GetEnvironmentVariable('HIP_PATH', 'Machine'))) {
        if ($hp -and (Test-Path "$hp\bin\hipcc.exe")) { return $hp }
    }
    # Legacy HIP SDK installs (pre-TheRock).
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


# GPU targets are DERIVED from the installed ROCm dist, not hand-maintained:
# the archs the dist ships Windows BLAS kernels for are exactly the ones worth
# (and safe) building. Enumeration + policy, so a dist bump only needs a
# re-run of this script:
#  - TheRock dist: .kpack\blas_lib_gfx*.kpack is the authoritative per-arch
#    kernel list (the 7.1-era TensileLibrary_lazy_*.dat files survive as a
#    legacy subset — only used as fallback for a legacy HIP SDK install).
#  - policy: drop CDNA/Vega (gfx9xx) — kpacks exist for MI cards but there is
#    no Windows driver for them (and gfx906's kernels are gone from TheRock).
#  - safety net: always keep the Ryzen iGPU archs in the fatbin even if a
#    future dist drops their kernels: the driver-bundled HIP runtime
#    (amdhip64_7.dll) fails kernel-module load on EVERY visible device when
#    one visible device has no matching code object — a visible iGPU would
#    otherwise break the dGPU with "device kernel image is invalid".
$gpuTargetsIgpuSafety = @('gfx1035', 'gfx1036', 'gfx1103', 'gfx1152', 'gfx1153')
# Snapshot of TheRock 7.14.0 coverage — used only when no ROCm dist is present
# at configure time (the HIP build cannot succeed then anyway, but the config
# stays complete and editable).
$gpuTargetsFallback = 'gfx1010;gfx1011;gfx1012;gfx1030;gfx1031;gfx1032;gfx1033;gfx1034;gfx1035;gfx1036;gfx1100;gfx1101;gfx1102;gfx1103;gfx1150;gfx1151;gfx1152;gfx1153;gfx1200;gfx1201'

function Get-GpuTargets([string]$HipPath) {
    if (-not $HipPath) { return $null }
    $archs = @(Get-ChildItem (Join-Path $HipPath '.kpack') -Filter 'blas_lib_gfx*.kpack' -ErrorAction SilentlyContinue |
        ForEach-Object { $_.BaseName -replace '^blas_lib_' })
    if (-not $archs) {
        $archs = @(Get-ChildItem (Join-Path $HipPath 'bin\rocblas\library') -Filter 'TensileLibrary_lazy_gfx*.dat' -ErrorAction SilentlyContinue |
            ForEach-Object { $_.BaseName -replace '^TensileLibrary_lazy_' })
    }
    if (-not $archs) { return $null }
    $archs = @($archs | Where-Object { $_ -notmatch '^gfx9' })
    return (($archs + $gpuTargetsIgpuSafety) | Sort-Object -Unique) -join ';'
}

function Find-Tool([string]$Name) {
    $cmd = Get-Command $Name -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    # Fallback: ROCm ships its own toolchain — the legacy HIP SDK under bin\,
    # the TheRock dist under lib\llvm\bin (clang, lld, llvm-*).
    if ($detected.HipPath) {
        foreach ($sub in @('bin', 'lib\llvm\bin')) {
            $exe = Join-Path $detected.HipPath "$sub\$Name.exe"
            if (Test-Path $exe) { return $exe }
        }
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

$gpuTargets = Get-GpuTargets $detected.HipPath
if ($gpuTargets) {
    Write-Host "  [OK] GpuTargets     : $(($gpuTargets -split ';').Count) archs from the installed ROCm dist" -ForegroundColor Green
    Write-Host "                        $gpuTargets" -ForegroundColor DarkGray
} else {
    $gpuTargets = $gpuTargetsFallback
    Write-Host "  [--] GpuTargets     : no kernel list found in the dist — using the 7.14.0 snapshot" -ForegroundColor Yellow
}

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
$buildLines.Add('    # GpuTargets: DERIVED from the installed ROCm dist by 01-configure.ps1 —')
$buildLines.Add('    # the archs the dist ships Windows BLAS kernels for (.kpack\blas_lib_gfx*.')
$buildLines.Add('    # kpack; legacy SDK fallback: TensileLibrary_lazy_gfx*.dat), minus CDNA/')
$buildLines.Add('    # Vega gfx9xx (no Windows driver for MI cards), plus the Ryzen iGPU archs')
$buildLines.Add('    # even when kernels are absent (the driver HIP runtime fails module load')
$buildLines.Add('    # on EVERY visible device when one visible device has no code object).')
$buildLines.Add('    # Re-run 01-configure.ps1 after a ROCm dist bump to refresh. Hand-edit')
$buildLines.Add('    # only to prune (~56 MiB of ggml-hip.dll per arch) — and never prune the')
$buildLines.Add('    # iGPU archs (gfx1035/1036/1103/1152/1153).')
$buildLines.Add("    GpuTargets  = '$gpuTargets'")
$buildLines.Add("    BuildType   = 'Release'")
# Full path to the resolved clang (bare 'clang' as a last resort): with the
# TheRock dist the compiler lives in HIP_PATH\lib\llvm\bin, which is not on a
# stale console's PATH — a bare name in the config then fails at cmake time.
$clangPath = Find-Tool 'clang'
if (-not $clangPath) { $clangPath = 'clang' }
$buildLines.Add("    CCompiler   = $(Fmt $clangPath)")
$buildLines.Add("    CxxCompiler = $(Fmt $clangPath)")
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
