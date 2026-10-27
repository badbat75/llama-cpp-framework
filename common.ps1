# Shared bootstrap for the BUILD scripts (02-build.ps1 / 03-package.ps1), which
# dot-source it: . "$PSScriptRoot\common.ps1"
# 00-install-prerequisites.ps1 and 01-configure.ps1 deliberately do NOT: they
# read build\windows\config-build.psd1 directly so they work on a fresh machine
# before (or while) that file exists.
#
# - Sets $buildRoot, the per-OS directory everything Windows generates lives in.
# - Loads $cfg from build\windows\config-build.psd1 (build-time paths, GPU targets, compiler).
# - Loads $sources + dists.ps1 and sets $distRoot, the root of the SDK dist layout.
# - Adds ROCm/HIP\bin to PATH so HIP DLLs are loadable at both build and run time.
# - Exposes Enable-VsDevShell as a function; both dot-sourcers call it (the
#   activation stays opt-in so a future non-build consumer isn't forced into it).

# Everything the Windows scripts generate (config, source clone, cmake binary
# dir, sccache cache, cargo target dir, MSI staging) lives under build\windows\;
# the Linux scripts use build/linux/ the same way, so the two systems never
# share an artifact on a drive both mount.
$buildRoot = Join-Path $PSScriptRoot 'build\windows'

$cfgPath = Join-Path $buildRoot 'config-build.psd1'
if (-not (Test-Path $cfgPath)) {
    throw "build\windows\config-build.psd1 not found. Run 01-configure.ps1 first."
}
$cfg = Import-PowerShellDataFile $cfgPath

# The SDK dist layout (dists.ps1): $sources says how dists are named, and
# $distRoot is the root 01-configure.ps1 RECORDED, so the CUDA Toolkit and the
# Vulkan SDK that 02-build.ps1 resolves come from the same root as the HipPath
# in the config. A config written before the key existed falls
# back to LLAMA_FW_DIST_ROOT, then to the default in dist-sources.psd1.
$sources = Import-PowerShellDataFile (Join-Path $PSScriptRoot 'installer\dist-sources.psd1')
. (Join-Path $PSScriptRoot 'dists.ps1')
$distRoot = if ($cfg.DistRoot) { $cfg.DistRoot } else { Resolve-DistRoot $null $null $sources }

# cargo's target dir for llama-cpp-config. 02-build.ps1 passes it to cargo as
# CARGO_TARGET_DIR, 03-package.ps1 stages the exe from it. A bare `cargo build`
# outside the scripts lands here too, through the `[build] target-dir` that
# 01-configure.ps1 writes into .cargo\config.toml (keep the two in step).
$cargoTargetDir = Join-Path $buildRoot 'cargo-target'

if ($cfg.HipPath -and (Test-Path $cfg.HipPath)) {
    $env:HIP_PATH = $cfg.HipPath
    if ($env:PATH -notlike "*$($env:HIP_PATH)\bin*") {
        $env:PATH = "$env:HIP_PATH\bin;$env:PATH"
    }
    # TheRock dist keeps the LLVM toolchain (clang for the HIP device compile)
    # under lib\llvm\bin; the legacy HIP SDK shipped clang in bin\ instead.
    $llvmBin = Join-Path $env:HIP_PATH 'lib\llvm\bin'
    if ((Test-Path $llvmBin) -and ($env:PATH -notlike "*$llvmBin*")) {
        $env:PATH = "$llvmBin;$env:PATH"
    }
    # Mirror the rest of the TheRock machine env (00-install sets it system-wide,
    # but a console opened before that run has a stale copy): clang finds the
    # device bitcode via HIP_DEVICE_LIB_PATH; TheRock keeps it under
    # lib\llvm\amdgcn\bitcode, not the <rocm>\amdgcn\bitcode layout clang
    # derives from --rocm-path, so without the var the HIP device compile dies
    # with "cannot find ROCm device library".
    $bitcode = Join-Path $env:HIP_PATH 'lib\llvm\amdgcn\bitcode'
    if (Test-Path $bitcode) {
        $env:HIP_DEVICE_LIB_PATH = $bitcode
        $env:HIP_PLATFORM       = 'amd'
        $env:LLVM_PATH          = Join-Path $env:HIP_PATH 'lib\llvm'
    }
}

function Enable-VsDevShell {
    if (-not $cfg.VsDevShell) {
        throw "VsDevShell not configured. Install Visual Studio with the C++ workload and re-run 01-configure.ps1."
    }
    if (-not (Test-Path $cfg.VsDevShell)) {
        throw "VsDevShell not found at '$($cfg.VsDevShell)'. Re-run 01-configure.ps1 to fix the path."
    }
    # vswhere.exe lives in the VS Installer dir, which isn't on PATH by default.
    # The VsDevCmd batch that Launch-VsDevShell.ps1 spawns calls vswhere by bare
    # name, so without this it prints "'vswhere.exe' is not recognized" (cosmetic,
    # but noisy). Prepend the Installer dir so that bare call resolves.
    $pf86 = ${env:ProgramFiles(x86)}
    if (-not $pf86) { $pf86 = 'C:\Program Files (x86)' }
    $vsInstaller = Join-Path $pf86 'Microsoft Visual Studio\Installer'
    if ((Test-Path (Join-Path $vsInstaller 'vswhere.exe')) -and ($env:PATH -notlike "*$vsInstaller*")) {
        $env:PATH = "$vsInstaller;$env:PATH"
    }
    $prev = Get-Location
    & $cfg.VsDevShell -Arch amd64
    Set-Location $prev
}
