# Shared bootstrap for the BUILD scripts (02-build.ps1 / 03-package.ps1), which
# dot-source it: . "$PSScriptRoot\common.ps1"
# 00-install-prerequisites.ps1 and 01-configure.ps1 deliberately do NOT — they
# read build\config-build.psd1 directly so they work on a fresh machine before
# (or while) that file exists.
#
# - Loads $cfg from build\config-build.psd1 (build-time paths, GPU targets, compiler).
# - Adds ROCm/HIP\bin to PATH so HIP DLLs are loadable at both build and run time.
# - Exposes Enable-VsDevShell as a function; both dot-sourcers call it (the
#   activation stays opt-in so a future non-build consumer isn't forced into it).

$cfgPath = Join-Path $PSScriptRoot 'build\config-build.psd1'
if (-not (Test-Path $cfgPath)) {
    throw "build\config-build.psd1 not found. Run 01-configure.ps1 first."
}
$cfg = Import-PowerShellDataFile $cfgPath

if ($cfg.HipPath -and (Test-Path $cfg.HipPath)) {
    $env:HIP_PATH = $cfg.HipPath
    if ($env:PATH -notlike "*$($env:HIP_PATH)\bin*") {
        $env:PATH = "$env:HIP_PATH\bin;$env:PATH"
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
