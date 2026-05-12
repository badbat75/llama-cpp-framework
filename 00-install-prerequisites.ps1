# Install & update the llama.cpp build toolchain in one shot.
#
# winget packages (PowerShell 7+, OpenSSL, NSIS) are installed if missing and
# upgraded if present, in a single self-elevated session. Manual SDKs (CUDA,
# Vulkan, AMD HIP) are only probed and their install URLs printed.
#
# When build\config-build.psd1 + llama.cpp clone exist, also runs `git pull --ff-only`
# on the source and flags a rebuild if the commit moved.
#
# Safe to run any time — idempotent.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

function Test-IsAdmin {
    ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
    ).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Get-WingetVersion {
    param([string]$Id)
    # winget's table output is locale-dependent; matching the row by Id prefix
    # sidesteps that.
    $output = winget list --id $Id --exact --accept-source-agreements 2>&1 | Out-String
    foreach ($line in ($output -split "`r?`n")) {
        if ($line.StartsWith($Id)) {
            $cols = $line -split '\s{2,}'
            if ($cols.Count -ge 3) { return $cols[2].Trim() }
        }
    }
    return $null
}

function Get-GitCommit {
    param([string]$RepoDir)
    if (-not $RepoDir -or -not (Test-Path "$RepoDir\.git")) { return $null }
    $sha = git -C $RepoDir rev-parse --short HEAD 2>$null
    if ($LASTEXITCODE -ne 0) { return $null }
    return $sha.Trim()
}

# ── Tracked packages and SDKs ───────────────────────────────────────

$wingetPackages = @(
    @{ Id = 'Microsoft.PowerShell'    ; Name = 'PowerShell 7+' }
    @{ Id = 'ShiningLight.OpenSSL.Dev'; Name = 'OpenSSL' }
    @{ Id = 'NSIS.NSIS'               ; Name = 'NSIS' }
)

$manualSdks = @(
    @{ Name = 'CUDA Toolkit'; Url = 'https://developer.nvidia.com/cuda-downloads'
       Probe = { Test-Path "${env:ProgramFiles}\NVIDIA GPU Computing Toolkit\CUDA\*\bin\nvcc.exe" } }
    @{ Name = 'Vulkan SDK'  ; Url = 'https://vulkan.lunarg.com/sdk/home'
       Probe = { ($env:VULKAN_SDK -and (Test-Path $env:VULKAN_SDK)) -or (Test-Path "${env:ProgramFiles}\VulkanSDK\*\Bin\glslc.exe") } }
    @{ Name = 'AMD HIP SDK' ; Url = 'https://www.amd.com/en/developer/resources/rocm-hub/hip-sdk.html'
       Probe = { Test-Path "${env:ProgramFiles}\AMD\ROCm\*\bin\hipcc.exe" } }
)

# ── Banner ──────────────────────────────────────────────────────────

Write-Host ""
Write-Host "  llama.cpp-framework — Install & Update Toolchain" -ForegroundColor Cyan
Write-Host "  ================================================" -ForegroundColor Cyan
Write-Host ""

# ── Capture pre-state ───────────────────────────────────────────────

Write-Host "Capturing current state..." -ForegroundColor DarkGray
$before  = @{}
$missing = @()
$present = @()
foreach ($p in $wingetPackages) {
    $v = Get-WingetVersion $p.Id
    $before[$p.Id] = $v
    if ($v) { $present += $p } else { $missing += $p }
}

$cfgPath = Join-Path $PSScriptRoot 'build\config-build.psd1'
$cfg = if (Test-Path $cfgPath) { Import-PowerShellDataFile $cfgPath } else { $null }
$beforeLlama = if ($cfg) { Get-GitCommit $cfg.LlamaCppDir } else { $null }

foreach ($p in $wingetPackages) {
    $v = $before[$p.Id]
    if ($v) { Write-Host "  [OK] $($p.Name) $v" -ForegroundColor Green }
    else    { Write-Host "  [..] $($p.Name) not installed" -ForegroundColor Yellow }
}
foreach ($s in $manualSdks) {
    if (& $s.Probe) { Write-Host "  [OK] $($s.Name)" -ForegroundColor Green }
    else            { Write-Host "  [--] $($s.Name) not found (manual install)" -ForegroundColor Yellow }
}
Write-Host ""

# ── Build the elevated batch (winget install + upgrade + symlinks) ──

$blocks = @()
foreach ($p in $missing) {
    $blocks += "Write-Host 'Installing $($p.Name)...' -ForegroundColor Cyan"
    $blocks += "winget install --id $($p.Id) --exact --silent --accept-source-agreements --accept-package-agreements"
}
foreach ($p in $present) {
    $blocks += "Write-Host 'Upgrading $($p.Name)...' -ForegroundColor Cyan"
    $blocks += "winget upgrade --id $($p.Id) --exact --silent --accept-source-agreements --accept-package-agreements"
}
# OpenSSL ships libs under lib\VC\x64\MD\ but cmake/find_package expects them
# directly under lib\. Idempotent — safe to re-run after any OpenSSL touch.
$blocks += @'

$d = "${env:ProgramFiles}\OpenSSL-Win64"
if (Test-Path "$d\lib\VC\x64\MD\libcrypto.lib") {
    if (-not (Test-Path "$d\lib\libcrypto.lib")) {
        New-Item -ItemType SymbolicLink -Path "$d\lib\libcrypto.lib" -Target "$d\lib\VC\x64\MD\libcrypto.lib" | Out-Null
        Write-Host "  Created symlink: libcrypto.lib" -ForegroundColor DarkGray
    }
    if (-not (Test-Path "$d\lib\libssl.lib")) {
        New-Item -ItemType SymbolicLink -Path "$d\lib\libssl.lib" -Target "$d\lib\VC\x64\MD\libssl.lib" | Out-Null
        Write-Host "  Created symlink: libssl.lib" -ForegroundColor DarkGray
    }
}
'@
$script = $blocks -join "`n"

if (Test-IsAdmin) {
    & ([scriptblock]::Create($script))
} else {
    $script += @'

Write-Host ""
Write-Host "Done." -ForegroundColor Green
Read-Host "Press Enter to close"
'@
    Write-Host "Requesting administrator privileges for winget..." -ForegroundColor Yellow
    $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($script))
    $proc = Start-Process powershell -Verb RunAs -Wait -PassThru `
        -ArgumentList "-ExecutionPolicy Bypass -EncodedCommand $encoded"
    if ($proc.ExitCode -ne 0) {
        Write-Host "Elevated session exited with code $($proc.ExitCode)" -ForegroundColor Red
    }
}

# ── Pull llama.cpp source if cloned ─────────────────────────────────

if ($cfg -and $beforeLlama) {
    Write-Host ""
    Write-Host "Updating llama.cpp..." -ForegroundColor Cyan
    git -C $cfg.LlamaCppDir pull --ff-only
    if ($LASTEXITCODE -ne 0) {
        Write-Host "  git pull failed in $($cfg.LlamaCppDir)" -ForegroundColor Yellow
    }
}

# ── Capture post-state ──────────────────────────────────────────────

$after = @{}
foreach ($p in $wingetPackages) { $after[$p.Id] = Get-WingetVersion $p.Id }
$afterLlama = if ($cfg) { Get-GitCommit $cfg.LlamaCppDir } else { $null }

# ── Report ──────────────────────────────────────────────────────────

Write-Host ""
Write-Host "  Update Report" -ForegroundColor Cyan
Write-Host "  =============" -ForegroundColor Cyan
Write-Host ""

function Write-ReportRow {
    param([string]$Marker, [ConsoleColor]$Color, [string]$Name, [string]$Detail)
    Write-Host ("  {0} {1,-20} {2}" -f $Marker, $Name, $Detail) -ForegroundColor $Color
}

foreach ($p in $wingetPackages) {
    $b = $before[$p.Id]
    $a = $after[$p.Id]
    if      (-not $b -and $a)      { Write-ReportRow "[++]" Green    $p.Name "installed $a" }
    elseif  (-not $b -and -not $a) { Write-ReportRow "[!!]" Red      $p.Name "install failed" }
    elseif  ($b -and -not $a)      { Write-ReportRow "[!!]" Red      $p.Name "no longer detected" }
    elseif  ($b -ne $a)            { Write-ReportRow "[++]" Green    $p.Name "$b -> $a" }
    else                           { Write-ReportRow "[OK]" DarkGray $p.Name $a }
}

$rebuildLlama = $false
if      (-not $beforeLlama)            { Write-ReportRow "[--]" DarkGray "llama.cpp" "(not cloned)" }
elseif  ($beforeLlama -ne $afterLlama) { Write-ReportRow "[++]" Green    "llama.cpp" "$beforeLlama -> $afterLlama"; $rebuildLlama = $true }
else                                   { Write-ReportRow "[OK]" DarkGray "llama.cpp" $beforeLlama }

Write-Host ""
Write-Host "  Manual SDKs (not auto-updated):" -ForegroundColor DarkGray
foreach ($s in $manualSdks) {
    Write-Host ("    {0,-15} - {1}" -f $s.Name, $s.Url) -ForegroundColor DarkGray
}

# ── Recommendations ─────────────────────────────────────────────────

Write-Host ""
if (-not $cfg) {
    Write-Host "  Next: .\01-configure.ps1   # detect paths and generate build\config-build.psd1" -ForegroundColor Cyan
} elseif ($rebuildLlama) {
    Write-Host "  Recommended actions:" -ForegroundColor Yellow
    Write-Host "    .\02-build.ps1            # llama.cpp source updated" -ForegroundColor Yellow
    Write-Host "    .\03-package.ps1          # rebuild installer afterwards" -ForegroundColor Yellow
} else {
    Write-Host "  Toolchain up to date." -ForegroundColor Green
}
Write-Host ""
