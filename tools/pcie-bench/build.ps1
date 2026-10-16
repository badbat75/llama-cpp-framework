<#
.SYNOPSIS
    Builds pcie-bench (HIP and/or CUDA) into build\windows\pcie-bench\.

.DESCRIPTION
    Supplies the machine-derived half of the configure line the same way
    02-build.ps1 does for llama.cpp: ROCm's clang as the CXX compiler, the
    process-scoped HIP compile vars from common.ps1, and, below clang 24, the
    patched __clang_hip_runtime_wrapper.h for the dist's clang resource major
    (without it MSVC's <cmath> clashes with the HIP device math declarations).
    Writes compile_commands.json for VS Code's IntelliSense. Needs
    build\windows\config-build.psd1, i.e. 01-configure.ps1 run once.

.PARAMETER NoHip
    Skip pcie-bench-hip.exe.

.PARAMETER NoCuda
    Skip pcie-bench-cuda.exe.
#>
param(
    [switch]$NoHip,
    [switch]$NoCuda
)
$ErrorActionPreference = 'Stop'

$repo = Resolve-Path (Join-Path $PSScriptRoot '..\..')
. (Join-Path $repo 'common.ps1')
Enable-VsDevShell

$buildDir = Join-Path $buildRoot 'pcie-bench'
$cmakeArgs = @('-G', 'Ninja', '-S', $PSScriptRoot, '-B', $buildDir, '-DCMAKE_BUILD_TYPE=Release',
               '-DCMAKE_EXPORT_COMPILE_COMMANDS=ON',
               "-DPCIE_BENCH_HIP=$(if ($NoHip) { 'OFF' } else { 'ON' })",
               "-DPCIE_BENCH_CUDA=$(if ($NoCuda) { 'OFF' } else { 'ON' })")

if (-not $NoHip) {
    $clang = Join-Path $cfg.HipPath 'lib\llvm\bin\clang++.exe'
    if (-not (Test-Path $clang)) { $clang = $cfg.CxxCompiler }
    $clangMajor = $null
    foreach ($resRoot in @('lib\llvm\lib\clang', 'lib\clang')) {
        $d = Join-Path $cfg.HipPath $resRoot
        if (-not (Test-Path $d)) { continue }
        $clangMajor = Get-ChildItem $d -Directory |
            ForEach-Object { $v = 0; if ([int]::TryParse($_.Name, [ref]$v)) { $v } } |
            Sort-Object -Descending | Select-Object -First 1
        if ($clangMajor) { break }
    }
    # Clang 24 is the floor, as in 02-build.ps1: older HIP runtime wrappers
    # clash with MSVC's <cmath>. -w is still passed so a cache built with the
    # old patched wrapper drops its -include.
    if ($clangMajor -lt 24) {
        throw "ROCm clang $clangMajor under $($cfg.HipPath) is below 24 (its HIP runtime wrapper clashes with MSVC's <cmath>); -NoHip skips the HIP build"
    }
    $cmakeArgs += "-DCMAKE_CXX_COMPILER=$($clang -replace '\\', '/')"
    $cmakeArgs += "-DCMAKE_CXX_FLAGS=-w"
}

if (-not $NoCuda) {
    # The newest toolkit of the dist layout, as in 02-build.ps1. A toolkit
    # change on an existing build dir needs that script's cache reset; here
    # deleting build\windows\pcie-bench is cheaper.
    $cudaToolkit = (Find-Dist 'Cuda' $sources $distRoot).Dir
    if (-not $cudaToolkit) {
        throw "no CUDA toolkit under $(Get-DistHome 'Cuda' $sources $distRoot) (00-install-prerequisites.ps1 installs it; -NoCuda skips it)"
    }
    $env:CUDA_PATH = $cudaToolkit
    $cmakeArgs += "-DCMAKE_CUDA_COMPILER:FILEPATH=$((Join-Path $cudaToolkit 'bin\nvcc.exe') -replace '\\', '/')"
}

cmake @cmakeArgs
if ($LASTEXITCODE -ne 0) { throw "cmake configure failed ($LASTEXITCODE)" }
cmake --build $buildDir
if ($LASTEXITCODE -ne 0) { throw "cmake build failed ($LASTEXITCODE)" }
Write-Host "built into $buildDir" -ForegroundColor Green
