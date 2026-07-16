# Install & update the llama.cpp build toolchain in one shot.
#
# winget packages (PowerShell 7+, OpenSSL, NSIS) are installed if missing and
# upgraded if present, in a single self-elevated session (which also symlinks
# OpenSSL's lib\VC\x64\MD\*.lib up to lib\ so cmake's find_package(OpenSSL)
# resolves). ROCm/HIP is installed from AMD's TheRock dist tarball (the classic
# HIP SDK installer is discontinued): the pinned multiarch tarball is downloaded
# and extracted to C:\TheRock\build in the same elevated session, which also
# sets the machine environment (HIP_PATH & co. + PATH). The remaining manual
# SDKs (CUDA, Vulkan) are only probed and their install URLs printed.
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
    # Locally relax EAP: under Windows PowerShell 5.1 — which is exactly what
    # runs this script on a fresh machine, since it is the script that INSTALLS
    # PowerShell 7 (hence no `#requires -Version 7` like its siblings) — a
    # native command writing anything to a REDIRECTED stderr throws a
    # terminating NativeCommandError when $ErrorActionPreference is 'Stop'.
    # Function-local, so the rest of the script keeps fail-fast semantics.
    $ErrorActionPreference = 'Continue'
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
    # Same PS 5.1 stderr-redirect rationale as Get-WingetVersion: a tagless /
    # grafted clone makes `git describe` print to the redirected stderr, which
    # must degrade to $null here, not terminate the script.
    $ErrorActionPreference = 'Continue'
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
)

# ── ROCm (TheRock) dist ─────────────────────────────────────────────
# AMD now distributes Windows ROCm/HIP as TheRock dist tarballs (the classic
# HIP SDK installer is discontinued). The version is PINNED in
# installer\dist-pins.psd1 — the single source of truth shared with the
# end-user runtime-deps script that 03-package.ps1 bundles into the installer.
# Bump it THERE, deliberately, and re-run; this script converges the install
# to the pin (rationale for the multiarch tarball and the prerelease fallback
# lives next to the data).
$rocm = (Import-PowerShellDataFile (Join-Path $PSScriptRoot 'installer\dist-pins.psd1')).Rocm

# Installed TheRock version: the marker our install step writes; else the
# dist's own .info\version (covers a manual install — note it says "7.14.0"
# even for an rc build, which is why our marker takes precedence); else
# 'unknown' for an unversioned/interrupted tree ('unknown' never equals Pin,
# so the next run converges it to the pin).
function Get-RocmInstalledVersion {
    $marker = Join-Path $rocm.InstallDir $rocm.Marker
    if (Test-Path $marker) { return (Get-Content $marker -TotalCount 1).Trim() }
    $infoVer = Join-Path $rocm.InstallDir '.info\version'
    if (Test-Path $infoVer) { return (Get-Content $infoVer -TotalCount 1).Trim() }
    if ((Test-Path (Join-Path $rocm.InstallDir 'bin\hipcc.exe')) -or
        (Test-Path (Join-Path $rocm.InstallDir 'bin\hipInfo.exe'))) { return 'unknown' }
    return $null
}

# HEAD-probe a dist URL: returns Content-Length, or $null when unreachable.
# Doubles as the "is this published yet?" check and as the size the download
# (and any leftover partial tarball) is verified against.
function Get-RemoteFileSize {
    param([string]$Url)
    # Same PS 5.1 redirected-stderr rationale as Get-WingetVersion.
    $ErrorActionPreference = 'Continue'
    # curl.exe explicitly — PS 5.1 aliases `curl` to Invoke-WebRequest.
    $head = curl.exe -sIL --fail --max-time 30 $Url 2>$null | Out-String
    if ($LASTEXITCODE -ne 0) { return $null }
    if ($head -match '(?im)^Content-Length:\s*(\d+)') { return [long]$Matches[1] }
    return $null
}

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
$rocmBefore = Get-RocmInstalledVersion

$cfgPath = Join-Path $PSScriptRoot 'build\config-build.psd1'
$cfg = if (Test-Path $cfgPath) { Import-PowerShellDataFile $cfgPath } else { $null }
$beforeLlama = if ($cfg) { Get-GitDescribe $cfg.LlamaCppDir } else { $null }

foreach ($p in $wingetPackages) {
    $v = $before[$p.Id]
    if ($v) { Write-Host "  [OK] $($p.Name) $v" -ForegroundColor Green }
    else    { Write-Host "  [..] $($p.Name) not installed" -ForegroundColor Yellow }
}
if ($rocmBefore) { Write-Host "  [OK] ROCm (TheRock) $rocmBefore" -ForegroundColor Green }
else             { Write-Host "  [..] ROCm (TheRock) not installed" -ForegroundColor Yellow }
foreach ($s in $manualSdks) {
    if (& $s.Probe) { Write-Host "  [OK] $($s.Name)" -ForegroundColor Green }
    else            { Write-Host "  [--] $($s.Name) not found (manual install)" -ForegroundColor Yellow }
}
Write-Host ""

# ── Decide the ROCm action ──────────────────────────────────────────

$rocmTarget  = $null   # dist to install this run (Version/Url/Size)
$rocmBlocked = $null   # why the install cannot converge to the pin this run
if ($rocmBefore -ne $rocm.Pin) {
    foreach ($d in $rocm.Dists) {
        $size = Get-RemoteFileSize $d.Url
        if ($size) { $rocmTarget = @{ Version = $d.Version; Url = $d.Url; Size = $size }; break }
    }
    if (-not $rocmTarget) {
        $rocmBlocked = 'no dist URL reachable (offline? not yet published?)'
    } elseif ($rocmTarget.Version -eq $rocmBefore) {
        # Already on the reachable fallback (e.g. the rc while the stable pin
        # is still propagating) — nothing better is published yet, keep it.
        $rocmBlocked = "pinned $($rocm.Pin) not published yet"
        $rocmTarget  = $null
    }
}
if ($rocmTarget -and $rocmBefore -and (Get-Process llama-server -ErrorAction SilentlyContinue)) {
    # Upgrading wipes InstallDir, and a running llama-server holds ROCm DLLs
    # loaded from there — never yank them out from under it.
    $rocmBlocked = 'llama-server is running - stop it and re-run'
    $rocmTarget  = $null
}
if ($rocmTarget) {
    $gb = [math]::Round($rocmTarget.Size / 1GB, 1)
    Write-Host "ROCm (TheRock) $($rocmTarget.Version) will be installed to $($rocm.InstallDir) ($gb GB download)" -ForegroundColor Cyan
    $drive  = (Split-Path -Qualifier $rocm.InstallDir).TrimEnd(':')
    $freeGB = [math]::Round((Get-PSDrive $drive).Free / 1GB, 1)
    if ($freeGB -lt 40) {
        Write-Host "  warning: only $freeGB GB free on ${drive}: — download + extract want ~40 GB" -ForegroundColor Yellow
    }
} elseif ($rocmBlocked) {
    Write-Host "ROCm (TheRock): $rocmBlocked" -ForegroundColor Yellow
}

# ── Build the elevated batch (winget + symlinks + ROCm dist) ────────

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
# ROCm: download + extract only when converging to the pin; the machine env
# every run (idempotent, and it self-heals a previously declined elevation).
if ($rocmTarget -or $rocmBefore) {
    $blocks += "`$rocmDir = '$($rocm.InstallDir)'; `$rocmMarker = '$($rocm.Marker)'"
    if ($rocmTarget) {
        $blocks += "`$rocmVer = '$($rocmTarget.Version)'; `$rocmUrl = '$($rocmTarget.Url)'; `$rocmSize = $($rocmTarget.Size)"
        $blocks += @'

Write-Host "Installing ROCm (TheRock) $rocmVer..." -ForegroundColor Cyan
$rocmTar = Join-Path $env:TEMP (Split-Path $rocmUrl -Leaf)
$haveTar = $false
if (Test-Path $rocmTar) {
    $len = (Get-Item $rocmTar).Length
    if ($len -eq $rocmSize)     { $haveTar = $true; Write-Host "  tarball already downloaded" -ForegroundColor DarkGray }
    elseif ($len -gt $rocmSize) { Remove-Item $rocmTar -Force }   # stale/corrupt; a smaller one resumes below
}
if (-not $haveTar) {
    # curl.exe explicitly (PS 5.1 aliases `curl` to Invoke-WebRequest);
    # -C - resumes a partial download left by an interrupted run.
    curl.exe --fail -L -C - --retry 3 --retry-delay 5 -o $rocmTar $rocmUrl
    if ($LASTEXITCODE -eq 0 -and (Get-Item $rocmTar -ErrorAction SilentlyContinue).Length -eq $rocmSize) {
        $haveTar = $true
    } else {
        Write-Host "  download failed (curl exit $LASTEXITCODE) — partial file kept, a re-run resumes it" -ForegroundColor Red
    }
}
if ($haveTar) {
    if (Test-Path $rocmDir) {
        Write-Host "  removing previous dist at $rocmDir..." -ForegroundColor DarkGray
        Remove-Item $rocmDir -Recurse -Force -ErrorAction SilentlyContinue
    }
    if (Test-Path $rocmDir) {
        Write-Host "  cannot clear $rocmDir (files in use?) — close whatever uses ROCm and re-run" -ForegroundColor Red
    } else {
        New-Item -ItemType Directory -Force -Path $rocmDir | Out-Null
        Write-Host "  extracting to $rocmDir (several GB, takes a while)..." -ForegroundColor DarkGray
        tar.exe -xzf $rocmTar -C $rocmDir --strip-components=1
        if ($LASTEXITCODE -eq 0) {
            Set-Content -Path (Join-Path $rocmDir $rocmMarker) -Value $rocmVer
            Remove-Item $rocmTar -Force
            Write-Host "  ROCm (TheRock) $rocmVer installed" -ForegroundColor Green
        } else {
            Write-Host "  extraction failed (tar exit $LASTEXITCODE) — tarball kept for retry" -ForegroundColor Red
        }
    }
}
'@
    }
    $blocks += @'

# Machine environment for the dist: HIP_PATH ONLY, gated on the marker so a
# failed install never points it at a broken tree. The compile-time vars
# (HIP_DEVICE_LIB_PATH, HIP_PLATFORM, LLVM_PATH) are deliberately NOT set
# machine-wide: the Adrenalin driver's own HIP runtime (System32's
# amdhip64_7.dll + amd_comgr_3.dll) reads LLVM_PATH at RUNTIME, and pointing
# it at TheRock's newer LLVM half-breaks it — hipMemGetInfo starts returning
# "invalid argument" (devices report 0 MiB) and every llama-server dies with
# an access violation (0xC0000005) mid weight-upload. Found 2026-07-16 with
# Adrenalin + TheRock 7.14; builds get all three per-process from common.ps1.
# The removal below self-heals machines poisoned by earlier versions of this
# script.
if (Test-Path (Join-Path $rocmDir $rocmMarker)) {
    Write-Host "Setting ROCm machine environment..." -ForegroundColor Cyan
    [Environment]::SetEnvironmentVariable('HIP_PATH', $rocmDir, 'Machine')
    foreach ($legacy in 'HIP_DEVICE_LIB_PATH', 'HIP_PLATFORM', 'LLVM_PATH') {
        if ([Environment]::GetEnvironmentVariable($legacy, 'Machine')) {
            [Environment]::SetEnvironmentVariable($legacy, $null, 'Machine')
            Write-Host "  removed machine-wide $legacy (breaks the driver HIP runtime)" -ForegroundColor DarkGray
        }
    }
    # PATH goes through the raw registry: setx /M truncates PATH at 1024 chars,
    # and [Environment]::SetEnvironmentVariable rewrites REG_EXPAND_SZ as
    # REG_SZ, breaking %SystemRoot%-style entries other software put there.
    $key = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey('SYSTEM\CurrentControlSet\Control\Session Manager\Environment', $true)
    $path = [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    $parts = @($path -split ';' | Where-Object { $_ })
    foreach ($add in @("$rocmDir\bin", "$rocmDir\lib\llvm\bin")) {
        if ($parts -notcontains $add) {
            $parts += $add
            Write-Host "  PATH += $add" -ForegroundColor DarkGray
        }
    }
    $key.SetValue('Path', ($parts -join ';'), [Microsoft.Win32.RegistryValueKind]::ExpandString)
    $key.Close()
}
'@
}
$script = $blocks -join "`n"

if (Test-IsAdmin) {
    & ([scriptblock]::Create($script))
} else {
    Write-Host "Requesting administrator privileges for winget + ROCm..." -ForegroundColor Yellow
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
        # Set/restore EAP around the stderr redirect (same PS 5.1 rationale as
        # Get-WingetVersion — this one runs at script scope, not in a function).
        $prevEap = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        $latestLlama = (git -C $cfg.LlamaCppDir describe --tags --abbrev=0 origin/master 2>$null | Select-Object -First 1)
        $ErrorActionPreference = $prevEap
        if ($latestLlama) { $latestLlama = $latestLlama.Trim() }
    }
}

# ── Capture post-state ──────────────────────────────────────────────

$after = @{}
foreach ($p in $wingetPackages) { $after[$p.Id] = Get-WingetVersion $p.Id }
$rocmAfter = Get-RocmInstalledVersion

# Make the dist visible to THIS session too (a 01-configure.ps1 run in the
# same console). HIP_PATH/PATH reach new terminals via the machine env; the
# three compile-time vars are PROCESS-scoped on purpose (machine-wide
# LLVM_PATH breaks the driver HIP runtime — see the elevated block) and
# build consoles get them from common.ps1.
if ($rocmAfter -and $rocmAfter -ne 'unknown') {
    $env:HIP_PATH            = $rocm.InstallDir
    $env:HIP_DEVICE_LIB_PATH = "$($rocm.InstallDir)\lib\llvm\amdgcn\bitcode"
    $env:HIP_PLATFORM        = 'amd'
    $env:LLVM_PATH           = "$($rocm.InstallDir)\lib\llvm"
    foreach ($add in @("$($rocm.InstallDir)\bin", "$($rocm.InstallDir)\lib\llvm\bin")) {
        if (@($env:PATH -split ';') -notcontains $add) { $env:PATH = "$add;$env:PATH" }
    }
}

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

$rocmChanged = $false
if (-not $rocmBefore -and $rocmAfter) {
    Write-ReportRow "[++]" Green "ROCm (TheRock)" "installed $rocmAfter"
    $rocmChanged = $true
} elseif (-not $rocmBefore -and -not $rocmAfter) {
    $detail = 'install failed'
    if ($rocmBlocked) { $detail = "not installed — $rocmBlocked" }
    Write-ReportRow "[!!]" Red "ROCm (TheRock)" $detail
} elseif ($rocmBefore -ne $rocmAfter) {
    Write-ReportRow "[++]" Green "ROCm (TheRock)" "$rocmBefore -> $rocmAfter"
    $rocmChanged = $true
} elseif ($rocmAfter -eq $rocm.Pin) {
    Write-ReportRow "[OK]" DarkGray "ROCm (TheRock)" $rocmAfter
} else {
    $detail = "$rocmAfter (pin $($rocm.Pin)"
    if ($rocmBlocked) { $detail += " — $rocmBlocked" }
    $detail += ')'
    Write-ReportRow "[..]" Yellow "ROCm (TheRock)" $detail
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

# ── ROCm environment sanity ─────────────────────────────────────────
# Two HIP runtimes reachable in an ambiguous order are the classic cause of
# silent crashes in multi-backend builds — surface the known offenders.

$envWarnings = @()
if (Test-Path "${env:ProgramFiles}\AMD\ROCm") {
    $envWarnings += "legacy AMD HIP SDK still under ${env:ProgramFiles}\AMD\ROCm — uninstall it (duplicate HIP runtimes)"
}
$userHip = [Environment]::GetEnvironmentVariable('HIP_PATH', 'User')
if ($userHip -and ($userHip.TrimEnd('\') -ne $rocm.InstallDir)) {
    $envWarnings += "user-level HIP_PATH ($userHip) shadows the machine one — remove it"
}
# Compile-time vars in a persistent scope poison the DRIVER's HIP runtime
# (System32 amdhip64_7.dll reads LLVM_PATH at runtime: hipMemGetInfo fails
# with "invalid argument", model loads die with 0xC0000005). The elevated leg
# self-heals the machine scope; anything remaining (declined UAC, user scope,
# other tooling) still needs to go.
foreach ($scope in 'Machine', 'User') {
    foreach ($name in 'LLVM_PATH', 'HIP_DEVICE_LIB_PATH', 'HIP_PLATFORM') {
        $v = [Environment]::GetEnvironmentVariable($name, $scope)
        if ($v) {
            $envWarnings += "$scope-level $name ($v) breaks the driver HIP runtime (0xC0000005 at model load) — remove it; builds set it per-process via common.ps1"
        }
    }
}
# amdhip64_*.dll in more than one PATH dir (the driver's System32 copy aside).
$pathAll = ([Environment]::GetEnvironmentVariable('Path', 'Machine'), [Environment]::GetEnvironmentVariable('Path', 'User')) -join ';'
$hipDllDirs = @($pathAll -split ';' | Where-Object { $_ } | ForEach-Object { $_.TrimEnd('\') } | Select-Object -Unique |
    Where-Object { ($_ -notlike "$env:windir*") -and (Test-Path (Join-Path $_ 'amdhip64*.dll')) })
if ($hipDllDirs.Count -gt 1) {
    $envWarnings += "amdhip64_*.dll in $($hipDllDirs.Count) PATH dirs (ambiguous load order): $($hipDllDirs -join '; ')"
}
if ($envWarnings.Count) {
    Write-Host ""
    foreach ($w in $envWarnings) { Write-Host "  [!!] $w" -ForegroundColor Yellow }
}

Write-Host ""
Write-Host "  Manual SDKs (not auto-updated):" -ForegroundColor DarkGray
foreach ($s in $manualSdks) {
    Write-Host ("    {0,-15} - {1}" -f $s.Name, $s.Url) -ForegroundColor DarkGray
}

# ── Recommendations ─────────────────────────────────────────────────

Write-Host ""
if ($rocmChanged) {
    Write-Host "  ROCm dist changed — verify with hipInfo.exe in a NEW terminal (all GPUs should list)," -ForegroundColor DarkGray
    Write-Host "  and re-check GpuTargets in build\config-build.psd1 against the new kernel set:" -ForegroundColor DarkGray
    Write-Host "    dir $($rocm.InstallDir)\bin\rocblas\library\TensileLibrary_lazy_gfx*.dat" -ForegroundColor DarkGray
    Write-Host ""
}
if (-not $cfg) {
    Write-Host "  Next: .\01-configure.ps1   # detect paths and generate build\config-build.psd1" -ForegroundColor Cyan
} elseif ($rocmChanged -or $rebuildLlama) {
    Write-Host "  Recommended actions:" -ForegroundColor Yellow
    if ($rocmChanged) {
        Write-Host "    .\01-configure.ps1        # HipPath moved to the TheRock dist" -ForegroundColor Yellow
    }
    if ($rebuildLlama) {
        Write-Host "    .\02-build.ps1            # newer llama.cpp release available" -ForegroundColor Yellow
    } else {
        Write-Host "    .\02-build.ps1            # rebuild against the new ROCm" -ForegroundColor Yellow
    }
    Write-Host "    .\03-package.ps1          # rebuild installer afterwards" -ForegroundColor Yellow
} else {
    Write-Host "  Toolchain up to date." -ForegroundColor Green
}
Write-Host ""
