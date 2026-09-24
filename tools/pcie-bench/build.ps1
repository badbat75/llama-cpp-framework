<#
.SYNOPSIS
    Builds pcie-bench (HIP and/or CUDA) into build\pcie-bench\.

.DESCRIPTION
    Supplies the machine-derived half of the configure line the same way
    02-build.ps1 does for llama.cpp: ROCm's clang as the CXX compiler, the
    process-scoped HIP compile vars from common.ps1, and the patched
    __clang_hip_runtime_wrapper.h for the dist's clang resource major (without
    it MSVC's <cmath> clashes with the HIP device math declarations). Needs
    build\config-build.psd1, i.e. 01-configure.ps1 run once.

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

$buildDir = Join-Path $repo 'build\pcie-bench'
$cmakeArgs = @('-G', 'Ninja', '-S', $PSScriptRoot, '-B', $buildDir, '-DCMAKE_BUILD_TYPE=Release',
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
    $wrapper = Join-Path $repo "patches\hip\$clangMajor\__clang_hip_runtime_wrapper.h"
    if (-not (Test-Path $wrapper)) {
        throw "no patched HIP runtime wrapper for clang $clangMajor (expected $wrapper): see patches\hip\README.md"
    }
    $wrapper = $wrapper -replace '\\', '/'
    $cmakeArgs += "-DCMAKE_CXX_COMPILER=$($clang -replace '\\', '/')"
    $cmakeArgs += "-DCMAKE_CXX_FLAGS=-w -D__CLANG_HIP_RUNTIME_WRAPPER_H__ -include `"$wrapper`""
}

cmake @cmakeArgs
if ($LASTEXITCODE -ne 0) { throw "cmake configure failed ($LASTEXITCODE)" }
cmake --build $buildDir
if ($LASTEXITCODE -ne 0) { throw "cmake build failed ($LASTEXITCODE)" }
Write-Host "built into $buildDir" -ForegroundColor Green
