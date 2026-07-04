# Install & update the llama.cpp build toolchain in one shot.
#
# winget packages (PowerShell 7+, OpenSSL, NSIS) are installed if missing and
# upgraded if present, in a single self-elevated session (which also symlinks
# OpenSSL's lib\VC\x64\MD\*.lib up to lib\ so cmake's find_package(OpenSSL)
# resolves). Manual SDKs (CUDA, Vulkan, AMD HIP) are only probed and their
# install URLs printed.
#
# When build\config-build.psd1 + llama.cpp clone exist, also fetches the source
# and flags a rebuild when a newer release tag (bNNNN) is available. (No `git
# pull`: 02-build.ps1 pins the clone to a tag on a detached HEAD, so a pull
# would always fail — the checkout onto the new tag is 02-build.ps1's job.)
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
    # winget's table output is locale-dependent and the column order puts Name
    # first (e.g. "PowerShell 7-x64  Microsoft.PowerShell  7.4.6.0  winget"),
    # so match the Id token anywhere on the line and return the next
    # whitespace-separated token as the version.
    $output = winget list --id $Id --exact --accept-source-agreements 2>&1 | Out-String
    foreach ($line in ($output -split "`r?`n")) {
        if (-not $line.Contains($Id)) { continue }
        $cols = $line -split '\s+' | Where-Object { $_ }
        for ($i = 0; $i -lt $cols.Count - 1; $i++) {
            if ($cols[$i] -eq $Id) { return $cols[$i + 1].Trim() }
        }
    }
    return $null
}

# The checked-out llama.cpp build tag (02-build.ps1 detaches onto the newest
# bNNNN release tag, so `git describe --tags` is e.g. "b9871").
function Get-GitDescribe {
    param([string]$RepoDir)
    if (-not $RepoDir -or -not (Test-Path "$RepoDir\.git")) { return $null }
    $tag = git -C $RepoDir describe --tags 2>$null
    if ($LASTEXITCODE -ne 0 -or -not $tag) { return $null }
    return $tag.Trim()
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
$beforeLlama = if ($cfg) { Get-GitDescribe $cfg.LlamaCppDir } else { $null }

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
    Write-Host "Requesting administrator privileges for winget..." -ForegroundColor Yellow
    $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($script))
    $proc = Start-Process powershell -Verb RunAs -Wait -PassThru `
        -ArgumentList "-ExecutionPolicy Bypass -EncodedCommand $encoded"
    if ($proc.ExitCode -ne 0) {
        Write-Host "Elevated session exited with code $($proc.ExitCode)" -ForegroundColor Red
    }
}

# ── Check llama.cpp source for a newer release tag ──────────────────
# The clone sits on a detached HEAD (02-build.ps1 pins it to a bNNNN tag), so
# no pull here — just fetch and compare against the newest tag reachable from
# origin/master (the same tag 02-build.ps1 would check out).

$latestLlama = $null
if ($cfg -and $beforeLlama) {
    Write-Host ""
    Write-Host "Checking llama.cpp for updates..." -ForegroundColor Cyan
    git -C $cfg.LlamaCppDir fetch origin --tags
    if ($LASTEXITCODE -ne 0) {
        Write-Host "  git fetch failed in $($cfg.LlamaCppDir)" -ForegroundColor Yellow
    } else {
        $latestLlama = (git -C $cfg.LlamaCppDir describe --tags --abbrev=0 origin/master 2>$null | Select-Object -First 1)
        if ($latestLlama) { $latestLlama = $latestLlama.Trim() }
    }
}

# ── Capture post-state ──────────────────────────────────────────────

$after = @{}
foreach ($p in $wingetPackages) { $after[$p.Id] = Get-WingetVersion $p.Id }

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
if (-not $beforeLlama) {
    Write-ReportRow "[--]" DarkGray "llama.cpp" "(not cloned)"
} elseif ($latestLlama -and $latestLlama -ne $beforeLlama) {
    # 02-build.ps1 performs the actual checkout onto the new tag.
    Write-ReportRow "[++]" Green "llama.cpp" "$beforeLlama -> $latestLlama available"
    $rebuildLlama = $true
} else {
    Write-ReportRow "[OK]" DarkGray "llama.cpp" $beforeLlama
}

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
    Write-Host "    .\02-build.ps1            # newer llama.cpp release available" -ForegroundColor Yellow
    Write-Host "    .\03-package.ps1          # rebuild installer afterwards" -ForegroundColor Yellow
} else {
    Write-Host "  Toolchain up to date." -ForegroundColor Green
}
Write-Host ""
