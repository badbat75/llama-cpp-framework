# Where the build machine keeps its GPU SDK dists (ROCm, CUDA, Vulkan), and
# which one a build uses. OpenSSL is not one of them: a system library, left
# where winget installs it.
# Dot-sourced by scripts\00-install-prerequisites.ps1, xtask configure and scripts\common.ps1:
#   . "$PSScriptRoot\dists.ps1"
# The PowerShell counterpart of the finders in xtask\src\<os>\dists.rs. Functions only, no
# side effects, and Windows PowerShell 5.1-safe and ASCII-only on purpose:
# scripts\00-install-prerequisites.ps1 is the script that INSTALLS PowerShell 7.
#
# Layout (installer\dist-sources.psd1 holds the names, this file the rules):
#   <DistRoot>\<Dir>\<Version>\<DistOs>      e.g. E:\TheRock\10.1.0\windows
# A dist counts only when it sits in that layout, carries its marker (written
# after a successful install, so an interrupted one is never taken) AND holds
# the files the build consumes. Nothing is read from the environment: a
# HIP_PATH, CUDA_PATH or VULKAN_SDK left by another install, or by a console
# opened before an upgrade, cannot pull a build out of the layout.

# The dist root in use. An explicit -DistRoot wins, then LLAMA_FW_DIST_ROOT,
# then what xtask configure recorded in config-build.psd1, then the default
# in dist-sources.psd1. Returned without a trailing separator, except for a
# bare drive root (E:\), which needs it to stay an absolute path.
function Resolve-DistRoot {
    param([string]$Explicit, $Config, $Sources)
    $root = $Explicit
    if (-not $root) { $root = $env:LLAMA_FW_DIST_ROOT }
    if (-not $root -and $Config) { $root = $Config.DistRoot }
    if (-not $root) { $root = $Sources.DistRoot }
    if (-not $root) { throw 'no dist root: dist-sources.psd1 has no DistRoot' }
    $root = [IO.Path]::GetFullPath($root)
    if ($root.Length -gt 3) { $root = $root.TrimEnd('\') }
    return $root
}

# <DistRoot>\<Dir>: where every version of one dist lives, plus its .downloads.
function Get-DistHome {
    param([string]$Name, $Sources, [string]$DistRoot)
    return (Join-Path $DistRoot $Sources.$Name.Dir)
}

# <DistRoot>\<Dir>\<Version>\<DistOs>: the directory of one version.
function Get-DistDir {
    param([string]$Name, [string]$Version, $Sources, [string]$DistRoot)
    return (Join-Path (Join-Path (Get-DistHome $Name $Sources $DistRoot) $Version) $Sources.DistOs)
}

# What each build leg needs from its dist, relative to the dist directory;
# also the completeness test scripts\00-install-prerequisites.ps1 applies before it
# calls a version installed.
#   Rocm:    the compiler, the HIP CMake package, and rocBLAS's Tensile
#            kernels (without them the backend loads and the first matmul dies
#            with "Could not initialize Tensile host").
#   Cuda:    nvcc, the static runtime ggml-cuda links, and cuBLAS.
#   Vulkan:  the import lib and the shader compiler ggml-vulkan invokes.
$distFiles = @{
    Rocm    = @('lib\llvm\bin\clang.exe', 'lib\cmake\hip\hip-config.cmake', 'bin\rocblas\library')
    Cuda    = @('bin\nvcc.exe', 'lib\x64\cudart_static.lib', 'lib\x64\cublas.lib')
    Vulkan  = @('Lib\vulkan-1.lib', 'Bin\glslc.exe')
}

# The newest COMPLETE dist of one kind (Rocm, Cuda, Vulkan) as
# Parsed/Version/Dir, or $null. Versions compare numerically ([version]): a
# string sort ranks 1.4.9 above 1.4.10.
function Find-Dist {
    param([string]$Name, $Sources, [string]$DistRoot)
    $distHome = Get-DistHome $Name $Sources $DistRoot
    $best = $null
    foreach ($d in @(Get-ChildItem $distHome -Directory -ErrorAction SilentlyContinue)) {
        $v = $null
        if (-not [version]::TryParse($d.Name, [ref]$v)) { continue }
        $dir = Join-Path $d.FullName $Sources.DistOs
        $complete = $true
        foreach ($rel in (@($Sources.$Name.Marker) + $distFiles[$Name])) {
            if (-not (Test-Path (Join-Path $dir $rel))) { $complete = $false; break }
        }
        if (-not $complete) { continue }
        if (-not $best -or $v -gt $best.Parsed) { $best = @{ Parsed = $v; Version = $d.Name; Dir = $dir } }
    }
    return $best
}
