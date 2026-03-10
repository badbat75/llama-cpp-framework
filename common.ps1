# Common bootstrap: load config and activate VS Developer Shell
# Dot-source this at the top of every script: . "$PSScriptRoot\common.ps1"

$cfg = Import-PowerShellDataFile "$PSScriptRoot\config.psd1"

# Activate VS Developer Shell (required for cmake, clang, ninja, etc.)
if (-not (Test-Path $cfg.VsDevShell)) {
    throw "VsDevShell not found at '$($cfg.VsDevShell)'. Run 00-configure.ps1 to fix paths."
}
$prevDir = Get-Location
& $cfg.VsDevShell -Arch amd64
Set-Location $prevDir

# Set up ROCm / HIP if configured (also provides clang, cmake if not already in PATH)
if ($cfg.HipPath -and (Test-Path $cfg.HipPath)) {
    $env:HIP_PATH = $cfg.HipPath
    if ($env:PATH -notlike "*$($env:HIP_PATH)\bin*") {
        $env:PATH = "$env:HIP_PATH\bin;$env:PATH"
    }
}
