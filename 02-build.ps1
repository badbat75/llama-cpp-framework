#requires -Version 7
# Build llama.cpp with CUDA + Vulkan + HIP (ROCm) support,
# then build llama-cpp-config (Rust GUI + CLI configurator).

. "$PSScriptRoot\common.ps1"  # loads $cfg, adds ROCm to PATH
Enable-VsDevShell

$ErrorActionPreference = 'Stop'

Write-Host ""
Write-Host "  === llama.cpp (CMake) ===" -ForegroundColor Yellow

# Clone llama.cpp if missing, otherwise fetch. Either way we build from the latest
# release TAG (bNNNN) rather than master HEAD: pinning to a tag gives a clean
# `git describe` (e.g. `b9867`, not `b9867-1-g…`) that names the bundled build and
# the installer package, and ships a tagged commit instead of an arbitrary master tip.
if (-not (Test-Path "$($cfg.LlamaCppDir)\CMakeLists.txt")) {
    Write-Host "llama.cpp not found at $($cfg.LlamaCppDir), cloning..." -ForegroundColor Yellow
    git clone https://github.com/ggerganov/llama.cpp $cfg.LlamaCppDir
    if ($LASTEXITCODE -ne 0) { throw "git clone failed" }
} else {
    Write-Host "Fetching latest llama.cpp (branches + tags)..." -ForegroundColor Cyan
    git -C $cfg.LlamaCppDir fetch origin --tags
    if ($LASTEXITCODE -ne 0) { throw "git fetch failed" }
}

# Newest release tag reachable from origin/master (the highest bNNNN); detach the
# working tree onto it so the build — and `git describe` in 03-package.ps1 — see a
# clean tagged release.
$llamaTag = (git -C $cfg.LlamaCppDir describe --tags --abbrev=0 origin/master 2>$null | Select-Object -First 1)
if (-not $llamaTag) { throw "could not resolve latest llama.cpp release tag from origin/master" }
$llamaTag = $llamaTag.Trim()
Write-Host "Checking out llama.cpp release tag: $llamaTag" -ForegroundColor Cyan
git -C $cfg.LlamaCppDir checkout --detach $llamaTag
if ($LASTEXITCODE -ne 0) { throw "git checkout $llamaTag failed" }

$buildDir = Join-Path $PSScriptRoot "build\llama.cpp-cmake"
New-Item -ItemType Directory -Path $buildDir -Force | Out-Null

# A CMakeCache.txt written by a previous toolchain pins the old compiler path
# (and the HIP paths derived from it): when the ROCm dist moves — e.g. a
# TheRock version bump relocating HIP_PATH — the cached entries poison the
# reconfigure. Detect the compiler mismatch and start clean. Only comparable
# when the config carries a full path (01-configure writes one when it can).
$cacheFile = Join-Path $buildDir 'CMakeCache.txt'
if ((Test-Path $cacheFile) -and [System.IO.Path]::IsPathRooted($cfg.CCompiler)) {
    $cachedCc = Select-String -Path $cacheFile -Pattern '^CMAKE_C_COMPILER:[^=]+=(.+)$' |
        Select-Object -First 1 | ForEach-Object { $_.Matches[0].Groups[1].Value }
    if ($cachedCc -and (($cachedCc -replace '/', '\') -ne ($cfg.CCompiler -replace '/', '\'))) {
        Write-Host "Cached compiler changed:" -ForegroundColor Yellow
        Write-Host "  $cachedCc -> $($cfg.CCompiler)" -ForegroundColor Yellow
        Write-Host "  wiping $buildDir for a clean configure" -ForegroundColor Yellow
        Remove-Item $buildDir -Recurse -Force
        New-Item -ItemType Directory -Path $buildDir -Force | Out-Null
    }
}
$opensslPath = $cfg.OpenSSLDir -replace '\\', '/'

# ── Vulkan SDK: resolve the newest installed version ──────────────
# The LunarG SDK installs into a VERSIONED dir (C:\VulkanSDK\<x.y.z.w>) and an
# upgrade removes the old tree, so every absolute path into it dies with it.
# Two things then keep pointing at the dead version:
#   - VULKAN_SDK in a console opened before the upgrade (and CMake's FindVulkan
#     searches that env var first — it is also what ggml-vulkan appends to
#     CMAKE_PREFIX_PATH for find_package(SPIRV-Headers));
#   - the CMakeCache entries FindVulkan wrote (Vulkan_INCLUDE_DIR /
#     Vulkan_LIBRARY / Vulkan_GLSLC_EXECUTABLE / Vulkan_GLSLANG_VALIDATOR_
#     EXECUTABLE), which CMake reuses WITHOUT checking the files still exist —
#     unlike a config-package <pkg>_DIR, which is re-searched when it vanishes.
# So the fix is to scan the PARENT dir and pin the highest version ourselves:
# set VULKAN_SDK for this process and pass the four cached paths explicitly, so
# they override whatever the cache holds. Deliberately NOT a build-dir wipe like
# the compiler-mismatch check above: only the Vulkan backend needs recompiling,
# and a wipe would cost a full CUDA + HIP rebuild.
function Find-VulkanSdk {
    # The parent of whatever VULKAN_SDK names (process env, then machine env —
    # a stale console has an old value but still the right root), then the
    # default install roots.
    $roots = @()
    foreach ($sdk in @($env:VULKAN_SDK, [Environment]::GetEnvironmentVariable('VULKAN_SDK', 'Machine'))) {
        if ($sdk) { $roots += (Split-Path $sdk -Parent) }
    }
    $roots += @('C:\VulkanSDK', "${env:ProgramFiles}\VulkanSDK", "${env:ProgramFiles(x86)}\VulkanSDK")

    $best = $null
    $bestVer = $null
    foreach ($root in ($roots | Where-Object { $_ } | Select-Object -Unique)) {
        if (-not (Test-Path $root)) { continue }
        foreach ($d in (Get-ChildItem $root -Directory -ErrorAction SilentlyContinue)) {
            # Compare numerically ([version]): a string sort ranks 1.4.9 above
            # 1.4.10. Skip anything the build cannot actually consume — the
            # import lib and the shader compiler ggml-vulkan invokes.
            $v = $null
            if (-not [version]::TryParse($d.Name, [ref]$v)) { continue }
            if (-not (Test-Path (Join-Path $d.FullName 'Lib\vulkan-1.lib'))) { continue }
            if (-not (Test-Path (Join-Path $d.FullName 'Bin\glslc.exe'))) { continue }
            if (-not $bestVer -or $v -gt $bestVer) { $bestVer = $v; $best = $d.FullName }
        }
    }
    return $best
}

$vulkanArgs = @()
$vulkanSdk = Find-VulkanSdk
if ($vulkanSdk) {
    Write-Host "Vulkan SDK: $vulkanSdk" -ForegroundColor Cyan
    if ($env:VULKAN_SDK -ne $vulkanSdk) {
        Write-Host "  (VULKAN_SDK was '$env:VULKAN_SDK' — overridden for this build)" -ForegroundColor DarkGray
    }
    $env:VULKAN_SDK = $vulkanSdk
    $vk = $vulkanSdk -replace '\\', '/'
    $vulkanArgs += "-DVulkan_INCLUDE_DIR:PATH=$vk/Include"
    $vulkanArgs += "-DVulkan_LIBRARY:FILEPATH=$vk/Lib/vulkan-1.lib"
    $vulkanArgs += "-DVulkan_GLSLC_EXECUTABLE:FILEPATH=$vk/Bin/glslc.exe"
    # Found unconditionally by FindVulkan (legacy var), so a stale cached path
    # would survive the same way — refresh it too, when the SDK still ships it.
    if (Test-Path "$vulkanSdk\Bin\glslangValidator.exe") {
        $vulkanArgs += "-DVulkan_GLSLANG_VALIDATOR_EXECUTABLE:FILEPATH=$vk/Bin/glslangValidator.exe"
    }
} else {
    # Non-standard layout (unversioned dir, custom root): leave FindVulkan to its
    # own search rather than guessing — it fails loudly enough if nothing is there.
    Write-Host "Vulkan SDK: no versioned install found — leaving detection to CMake" -ForegroundColor Yellow
}

# ── sccache: use local cache if available ─────────────────────────
$sccachePath = Get-Command sccache -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source
if ($sccachePath) {
    $sccacheDir = Join-Path $PSScriptRoot "build\.sccache"
    New-Item -ItemType Directory -Path $sccacheDir -Force | Out-Null
    $env:SCCACHE_DIR = $sccacheDir
    $env:SCCACHE_CACHE_SIZE = "10G"
    $env:SCCACHE_IDLE_TIMEOUT = "0"
    $env:SCCACHE_MAX_FRAME_LENGTH = "104857600"  # 100MB — GPU multi-arch objects are large
    # Client-side mode (sccache 0.17+): cache lookup + compile run in the client process,
    # the daemon serves only shared state — removes the per-compile server round-trip.
    # Older sccache ignores the var. sccache itself ignores it under SCCACHE_ERROR_LOG
    # or distributed compilation (neither used here); stats stay server-side, so
    # --show-stats below still aggregates the whole build.
    $env:SCCACHE_CLIENT_SIDE = "1"
    Write-Host "sccache: $sccachePath (cache: $sccacheDir)" -ForegroundColor Cyan
} else {
    Write-Host "sccache not found — building without compiler cache" -ForegroundColor DarkGray
}

$cmakeArgs = @(
    "-S", $cfg.LlamaCppDir
    "-B", $buildDir
    "-G", "Ninja"
    "-DGGML_NATIVE=OFF"
    "-DGGML_BACKEND_DL=ON"
    "-DGGML_CPU_ALL_VARIANTS=ON"
    "-DGGML_CUDA=ON"
    "-DGGML_VULKAN=ON"
    "-DGGML_HIP=ON"
    "-DGPU_TARGETS=$($cfg.GpuTargets)"
    # ROCm's hip-config-amd.cmake derives the --offload-arch flags from
    # GPU_BUILD_TARGETS, which it seeds from GPU_TARGETS with a `set(... CACHE ...)`
    # — a no-op once the entry exists. Without -U, editing GpuTargets silently does
    # nothing on an existing build dir: the cached arch list wins and Ninja sees
    # unchanged command lines. Clearing it re-derives the list every configure.
    "-UGPU_BUILD_TARGETS"
    "-DCMAKE_BUILD_TYPE=$($cfg.BuildType)"
    "-DCMAKE_C_COMPILER=$($cfg.CCompiler)"
    "-DCMAKE_CXX_COMPILER=$($cfg.CxxCompiler)"
    "-DCMAKE_C_FLAGS=$($cfg.MarchFlags) -w"
    "-DOPENSSL_ROOT_DIR:PATH=$opensslPath"
    "-DCMAKE_CUDA_FLAGS=-w"
)

# Pinned Vulkan SDK paths (see Find-VulkanSdk above) — empty when no versioned
# install was found, in which case FindVulkan does its own search.
$cmakeArgs += $vulkanArgs

# ── HIP workaround for MSVC 14.51 (VS 18) <cmath> include order ──
# The stock __clang_hip_runtime_wrapper.h includes <cmath> before the HIP
# device math headers, causing MSVC's _CLANG_BUILTIN2 constexpr overloads
# (implicitly __host__ __device__) to conflict with __device__ declarations
# in __clang_cuda_math_forward_declares.h and __clang_hip_cmath.h.
# First hit with ROCm 7.1; re-verified 2026-07-16 against TheRock ROCm 7.14
# (AMD clang 23) + MSVC 14.51.36231: the stock headers still trip the same
# isgreater/_CLANG_BUILTIN2 conflict and the patched wrapper still compiles
# clean — so this stays until MSVC or ROCm fix the overload clash upstream.
# A patched wrapper reverses the include order; we suppress the stock one via
# -D__CLANG_HIP_RUNTIME_WRAPPER_H__ and force-include the patched copy. The
# copy is a modified snapshot of the toolchain's OWN header, so it is
# versioned per clang resource major (patches\hip\<major>\) and selected from
# the installed dist — a ROCm bump that changes the clang major fails fast
# with regeneration instructions (patches\hip\README.md) instead of silently
# force-including a stale wrapper. Flags go through both CMAKE_CXX_FLAGS and
# CMAKE_HIP_FLAGS because CMake on Windows treats HIP sources as CXX when
# enable_language(HIP) is not called.
$clangMajor = $null
foreach ($resRoot in @('lib\llvm\lib\clang', 'lib\clang')) {   # TheRock dist, legacy HIP SDK
    $d = Join-Path $cfg.HipPath $resRoot
    if (-not (Test-Path $d)) { continue }
    $clangMajor = Get-ChildItem $d -Directory |
        ForEach-Object { $v = 0; if ([int]::TryParse($_.Name, [ref]$v)) { $v } } |
        Sort-Object -Descending | Select-Object -First 1
    if ($clangMajor) { break }
}
if (-not $clangMajor) {
    throw "could not detect the ROCm clang resource version under $($cfg.HipPath) (probed lib\llvm\lib\clang and lib\clang)"
}
$hipPatchedInc = Join-Path $PSScriptRoot "patches\hip\$clangMajor\__clang_hip_runtime_wrapper.h"
if (-not (Test-Path $hipPatchedInc)) {
    throw "no patched HIP runtime wrapper for clang $clangMajor (expected $hipPatchedInc). New toolchain — regenerate and validate it per patches\hip\README.md."
}
Write-Host "HIP wrapper patch: patches\hip\$clangMajor (clang resource major $clangMajor)" -ForegroundColor DarkGray
$hipPatchedInc = $hipPatchedInc -replace '\\', '/'
$hipWorkaroundFlags = "-D__CLANG_HIP_RUNTIME_WRAPPER_H__ -include `"$hipPatchedInc`""
$cmakeArgs += "-DCMAKE_CXX_FLAGS=$($cfg.MarchFlags) -w $hipWorkaroundFlags"
$cmakeArgs += "-DCMAKE_HIP_FLAGS=$hipWorkaroundFlags"

if ($sccachePath) {
    $cmakeArgs += "-DCMAKE_C_COMPILER_LAUNCHER=$sccachePath"
    $cmakeArgs += "-DCMAKE_CXX_COMPILER_LAUNCHER=$sccachePath"
    # nvcc is intentionally NOT wrapped with sccache (no CMAKE_CUDA_COMPILER_LAUNCHER):
    # sccache still mishandles multi-arch fatbin generation on CUDA 13.x, so fatbinary
    # fails with "Could not open input file '<tu>.compute_75.ptx'" on every .cu.obj.
    # Retested with sccache 0.17.0 (2026-07) — still broken, in server AND
    # client-side mode (minimal repro: multi-gencode nvcc -c, fatbinary can't
    # find the per-arch intermediates sccache's nvcc decomposition produced).
    # Host C/CXX (clang) caching is unaffected and kept.
    # -U clears any stale CUDA launcher a prior run/experiment may have baked into
    # CMakeCache.txt: an existing cache keeps the value even when it's no longer
    # passed, silently re-wrapping nvcc and breaking the CUDA build.
    $cmakeArgs += "-UCMAKE_CUDA_COMPILER_LAUNCHER"
}

Write-Host "Configuring..." -ForegroundColor Cyan
cmake @cmakeArgs
if ($LASTEXITCODE -ne 0) { throw "CMake configure failed" }

$cmakeBuildArgs = @("--build", $buildDir)
if ($cfg.BuildJobs -gt 0) { $cmakeBuildArgs += "-j", $cfg.BuildJobs } else { $cmakeBuildArgs += "-j" }

Write-Host "Building..." -ForegroundColor Cyan
cmake @cmakeBuildArgs
if ($LASTEXITCODE -ne 0) { throw "CMake build failed" }

if ($sccachePath) {
    Write-Host ""
    Write-Host "sccache stats:" -ForegroundColor Cyan
    sccache --show-stats
    sccache --stop-server 2>$null | Out-Null
}

Write-Host "llama.cpp build complete: $buildDir\bin\" -ForegroundColor Green

# ── llama-cpp-config (Rust) ───────────────────────────────────────
Write-Host ""
Write-Host "  === llama-cpp-config (Rust) ===" -ForegroundColor Yellow

$cargo = Get-Command cargo -ErrorAction SilentlyContinue
if (-not $cargo) {
    throw "cargo not found. Install Rust: https://rustup.rs"
}
Write-Host "cargo: $($cargo.Source)" -ForegroundColor DarkGray

$rustProjectDir = Join-Path $PSScriptRoot "llama-cpp-config"

Push-Location $rustProjectDir
try {
    # Cap cargo at the same job count as the C++ build (0 = cargo's default,
    # which is all cores) so a Rust rebuild leaves the same CPU headroom.
    $cargoBuildArgs = @("build", "--release")
    if ($cfg.BuildJobs -gt 0) { $cargoBuildArgs += "--jobs", $cfg.BuildJobs }
    cargo @cargoBuildArgs
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
}
finally {
    Pop-Location
}

# 03-package.ps1 stages the binary straight from cargo's target\release dir — no
# intermediate copy under build\.
$exe = Join-Path $rustProjectDir "target\release\llama-cpp-config.exe"
Write-Host "llama-cpp-config build complete: $exe" -ForegroundColor Green
Write-Host ""
