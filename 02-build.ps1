# Build llama.cpp with CUDA + Vulkan + HIP (ROCm) support

. "$PSScriptRoot\common.ps1"  # loads $cfg, adds ROCm to PATH
Enable-VsDevShell

# Clone llama.cpp if missing, otherwise pull latest
if (-not (Test-Path "$($cfg.LlamaCppDir)\CMakeLists.txt")) {
    Write-Host "llama.cpp not found at $($cfg.LlamaCppDir), cloning..." -ForegroundColor Yellow
    git clone https://github.com/ggerganov/llama.cpp $cfg.LlamaCppDir
    if ($LASTEXITCODE -ne 0) { throw "git clone failed" }
} else {
    Write-Host "Pulling latest llama.cpp..." -ForegroundColor Cyan
    git -C $cfg.LlamaCppDir pull
    if ($LASTEXITCODE -ne 0) { throw "git pull failed" }
}

$buildDir = Join-Path $PSScriptRoot "build\llama.cpp-cmake"
New-Item -ItemType Directory -Path $buildDir -Force | Out-Null
$opensslPath = $cfg.OpenSSLDir -replace '\\', '/'

# ── sccache: use local cache if available ─────────────────────────
$sccachePath = Get-Command sccache -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source
if ($sccachePath) {
    $sccacheDir = Join-Path $PSScriptRoot ".sccache"
    New-Item -ItemType Directory -Path $sccacheDir -Force | Out-Null
    $env:SCCACHE_DIR = $sccacheDir
    $env:SCCACHE_CACHE_SIZE = "10G"
    $env:SCCACHE_IDLE_TIMEOUT = "0"
    $env:SCCACHE_MAX_FRAME_LENGTH = "104857600"  # 100MB — GPU multi-arch objects are large
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
    "-DCMAKE_BUILD_TYPE=$($cfg.BuildType)"
    "-DCMAKE_C_COMPILER=$($cfg.CCompiler)"
    "-DCMAKE_CXX_COMPILER=$($cfg.CxxCompiler)"
    "-DCMAKE_C_FLAGS=$($cfg.MarchFlags) -w"
    "-DCMAKE_CXX_FLAGS=$($cfg.MarchFlags) -w"
    "-DOPENSSL_ROOT_DIR:PATH=$opensslPath"
    "-DCMAKE_CUDA_FLAGS=-w"
)

if ($sccachePath) {
    $cmakeArgs += "-DCMAKE_C_COMPILER_LAUNCHER=$sccachePath"
    $cmakeArgs += "-DCMAKE_CXX_COMPILER_LAUNCHER=$sccachePath"
    $cmakeArgs += "-DCMAKE_CUDA_COMPILER_LAUNCHER=$sccachePath"
}

Write-Host "Configuring..." -ForegroundColor Cyan
cmake @cmakeArgs
if ($LASTEXITCODE -ne 0) { throw "CMake configure failed" }

$buildArgs = @("--build", $buildDir)
if ($cfg.BuildJobs -gt 0) { $buildArgs += "-j", $cfg.BuildJobs } else { $buildArgs += "-j" }

Write-Host "Building..." -ForegroundColor Cyan
cmake @buildArgs
if ($LASTEXITCODE -ne 0) { throw "CMake build failed" }

if ($sccachePath) {
    Write-Host ""
    Write-Host "sccache stats:" -ForegroundColor Cyan
    sccache --show-stats
    sccache --stop-server 2>$null | Out-Null
}

Write-Host "Build complete: $buildDir\bin\" -ForegroundColor Green
