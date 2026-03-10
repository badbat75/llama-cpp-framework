# Build llama.cpp with CUDA + Vulkan + HIP (ROCm) support

. "$PSScriptRoot\common.ps1"  # loads $cfg, activates VS Dev Shell + ROCm

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

Push-Location $cfg.LlamaCppDir

$opensslPath = $cfg.OpenSSLDir -replace '\\', '/'

$cmakeArgs = @(
    "-B", "build"
    "-G", "Ninja"
    "-DGGML_NATIVE=OFF"
    "-DGGML_CUDA=ON"
    "-DGGML_VULKAN=ON"
    "-DGGML_HIP=ON"
    "-DGPU_TARGETS=$($cfg.GpuTargets)"
    "-DCMAKE_BUILD_TYPE=$($cfg.BuildType)"
    "-DCMAKE_C_COMPILER=$($cfg.CCompiler)"
    "-DCMAKE_CXX_COMPILER=$($cfg.CxxCompiler)"
    "-DCMAKE_C_FLAGS=$($cfg.MarchFlags)"
    "-DCMAKE_CXX_FLAGS=$($cfg.MarchFlags)"
    "-DOPENSSL_ROOT_DIR:PATH=$opensslPath"
)

Write-Host "Configuring..." -ForegroundColor Cyan
cmake @cmakeArgs
if ($LASTEXITCODE -ne 0) { Pop-Location; throw "CMake configure failed" }

$buildArgs = @("--build", "build")
if ($cfg.BuildJobs -gt 0) { $buildArgs += "-j", $cfg.BuildJobs } else { $buildArgs += "-j" }

Write-Host "Building..." -ForegroundColor Cyan
cmake @buildArgs
if ($LASTEXITCODE -ne 0) { Pop-Location; throw "CMake build failed" }

Pop-Location
Write-Host "Build complete: $($cfg.LlamaCppDir)\build\bin\" -ForegroundColor Green
