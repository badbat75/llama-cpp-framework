# Install & update the llama.cpp build toolchain in one shot.
#
# winget packages (PowerShell 7+, OpenSSL, NSIS) are installed if missing and
# upgraded if present, in a single self-elevated session (which also symlinks
# OpenSSL's lib\VC\x64\MD\*.lib up to lib\ so cmake's find_package(OpenSSL)
# resolves). ROCm/HIP is installed from AMD's TheRock dist tarball (the classic
# HIP SDK installer is discontinued): the pinned multiarch tarball is downloaded
# and extracted into its own version directory (C:\TheRock\<version>) in the
# same elevated session, which also sets the machine environment (HIP_PATH on
# the ACTIVE version + PATH, pruned of the others). Dists therefore sit side
# by side, and converging to a version already on disk costs a HIP_PATH move
# instead of a download. The remaining manual SDKs (CUDA, Vulkan) are only
# probed and their install URLs printed.
#
# When build\config-build.psd1 + llama.cpp clone exist, also fetches the source
# and flags a rebuild when a newer release tag (vX.Y.Z) is available. (No `git
# pull`: 02-build.ps1 pins the clone to a tag on a detached HEAD, so a pull
# would always fail; the checkout onto the new tag is 02-build.ps1's job.)
#
# Safe to run any time: idempotent.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

function Test-IsAdmin {
    ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
    ).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Get-WingetVersion {
    param([string]$Id)
    # Locally relax EAP: under Windows PowerShell 5.1, which is exactly what
    # runs this script on a fresh machine, since it is the script that INSTALLS
    # PowerShell 7 (hence no `#requires -Version 7` like its siblings), a
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

# The checked-out llama.cpp tag (02-build.ps1 detaches onto the newest vX.Y.Z
# RELEASE tag, so `git describe --tags` is e.g. "v0.2.0"; on a commit tagged both
# ways describe prefers the annotated release tag over the lightweight bNNNN).
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
# installer\dist-pins.psd1: the single source of truth shared with the
# end-user runtime-deps script that 03-package.ps1 bundles into the installer.
# Bump it THERE, deliberately, and re-run; this script converges the install
# to the pin (rationale for the multiarch tarball and the candidate list
# lives next to the data).
#
# Dists install SIDE BY SIDE, one directory per version under Root, and
# HIP_PATH names the active one. So converging to a version already extracted
# by an earlier run costs a HIP_PATH move and nothing else, and an upgrade
# never destroys the dist the machine is currently building against. Two
# things follow, both handled below: the machine PATH must be PRUNED of the
# other version dirs (two amdhip64_*.dll dirs on PATH is the classic silent
# crash), and the pre-versioning single-directory layout (C:\TheRock\build,
# what every install before 1.11.4 wrote) is RENAMED into its version dir
# rather than re-downloaded: same bytes, 4.6 GB cheaper.
$rocm = (Import-PowerShellDataFile (Join-Path $PSScriptRoot 'installer\dist-pins.psd1')).Rocm
$rocmLegacyDir = Join-Path $rocm.Root 'build'   # pre-versioning layout

function Get-RocmVersionDir([string]$Version) { Join-Path $rocm.Root $Version }

# The version a dist directory holds: the marker our install step writes;
# else the dist's own .info\version (covers a manual install; note it says
# "7.14.0" even for an rc build, which is why our marker takes precedence);
# else 'unknown' for an interrupted/unversioned tree ('unknown' never equals
# Pin, so the next run converges it to the pin).
function Get-RocmDirVersion([string]$Dir) {
    if (-not $Dir -or -not (Test-Path $Dir)) { return $null }
    $marker = Join-Path $Dir $rocm.Marker
    if (Test-Path $marker) { return (Get-Content $marker -TotalCount 1).Trim() }
    $infoVer = Join-Path $Dir '.info\version'
    if (Test-Path $infoVer) { return (Get-Content $infoVer -TotalCount 1).Trim() }
    if ((Test-Path (Join-Path $Dir 'bin\hipcc.exe')) -or
        (Test-Path (Join-Path $Dir 'bin\hipInfo.exe'))) { return 'unknown' }
    return $null
}

# The dist the machine is actually using: whatever HIP_PATH points at. Process
# env first, then machine (a console opened before an earlier run has a stale
# copy); a HIP_PATH outside Root is honoured too, it is still what builds and
# llama-server load.
function Get-RocmActiveDir {
    foreach ($hp in @($env:HIP_PATH, [Environment]::GetEnvironmentVariable('HIP_PATH', 'Machine'))) {
        if ($hp -and (Get-RocmDirVersion $hp.TrimEnd('\'))) { return $hp.TrimEnd('\') }
    }
    return $null
}

# Every dist directory under Root, version -> path. The legacy unversioned
# directory is included under the version it holds, but only when no proper
# version dir already claims it (after a migration both exist for as long as
# the user keeps the old one).
function Get-RocmInstalled {
    $found = [ordered]@{}
    foreach ($dir in @(Get-ChildItem $rocm.Root -Directory -ErrorAction SilentlyContinue | Sort-Object Name)) {
        $v = Get-RocmDirVersion $dir.FullName
        if ($v -and -not $found.Contains($v)) { $found[$v] = $dir.FullName }
    }
    return $found
}

# HEAD-probe a dist URL: returns Content-Length, or $null when unreachable.
# Doubles as the "is this published yet?" check and as the size the download
# (and any leftover partial tarball) is verified against.
function Get-RemoteFileSize {
    param([string]$Url)
    # Same PS 5.1 redirected-stderr rationale as Get-WingetVersion.
    $ErrorActionPreference = 'Continue'
    # curl.exe explicitly: PS 5.1 aliases `curl` to Invoke-WebRequest.
    $head = curl.exe -sIL --fail --max-time 30 $Url 2>$null | Out-String
    if ($LASTEXITCODE -ne 0) { return $null }
    if ($head -match '(?im)^Content-Length:\s*(\d+)') { return [long]$Matches[1] }
    return $null
}

# The newest STABLE dist AMD has published, scanned from the tarball indexes,
# or $null when none is reachable. Purely informational: it feeds a report row
# recommending a Pin bump, never the install decision (why that split is the
# pins file's call, not this script's: see installer\dist-pins.psd1).
#
# Both indexes are HTML pages that carry their listing as a JS `files` array
# ({"name": ..., "mtime": ...}), so the names are matched with a regex rather
# than parsed: no contract to depend on beyond the file names themselves,
# which are the thing that has stayed stable while the HOST moved. The regex
# is strict for the reason 02-build.ps1's tag regex is: `-tests-` builds sit
# in the same listing, and a nightly is a version with a date glued on
# (10.1.0a20260831), so anything but `\d+\.\d+\.\d+` right before the
# extension is not a stable release. Scanning both hosts and taking the
# highest also means the URL comes back WITH the version, which is what makes
# this safe to trust: the index a version lives on is a function of the
# version, and getting that wrong is a 403 that reads like "not published".
function Get-RocmLatestPublished {
    if (-not $rocm.Indexes) { return $null }
    # Same PS 5.1 redirected-stderr rationale as Get-WingetVersion.
    $ErrorActionPreference = 'Continue'
    $best = $null
    foreach ($index in $rocm.Indexes) {
        # curl.exe explicitly: PS 5.1 aliases `curl` to Invoke-WebRequest.
        $page = curl.exe -sL --fail --connect-timeout 5 --max-time 20 $index 2>$null | Out-String
        if ($LASTEXITCODE -ne 0 -or -not $page) { continue }
        foreach ($m in [regex]::Matches($page, 'therock-dist-windows-multiarch-(\d+\.\d+\.\d+)\.tar\.gz')) {
            $v = [version]$m.Groups[1].Value
            if (-not $best -or $v -gt $best.Parsed) {
                $best = @{
                    Parsed  = $v
                    Version = $m.Groups[1].Value
                    Url     = [uri]::new([uri]$index, $m.Value).AbsoluteUri
                }
            }
        }
    }
    return $best
}

# ── Banner ──────────────────────────────────────────────────────────

Write-Host ""
Write-Host "  llama.cpp-framework: Install & Update Toolchain" -ForegroundColor Cyan
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
$rocmBeforeDir = Get-RocmActiveDir
$rocmBefore    = Get-RocmDirVersion $rocmBeforeDir
$rocmOnDisk    = Get-RocmInstalled

$cfgPath = Join-Path $PSScriptRoot 'build\config-build.psd1'
$cfg = if (Test-Path $cfgPath) { Import-PowerShellDataFile $cfgPath } else { $null }
$beforeLlama = if ($cfg) { Get-GitDescribe $cfg.LlamaCppDir } else { $null }

foreach ($p in $wingetPackages) {
    $v = $before[$p.Id]
    if ($v) { Write-Host "  [OK] $($p.Name) $v" -ForegroundColor Green }
    else    { Write-Host "  [..] $($p.Name) not installed" -ForegroundColor Yellow }
}
if ($rocmBefore) {
    Write-Host "  [OK] ROCm (TheRock) $rocmBefore ($rocmBeforeDir)" -ForegroundColor Green
    $others = @($rocmOnDisk.Keys | Where-Object { $rocmOnDisk[$_] -ne $rocmBeforeDir })
    if ($others.Count) {
        Write-Host "       other dists on disk (~25 GB each): $($others -join ', ')" -ForegroundColor DarkGray
    }
} elseif ($rocmOnDisk.Count) {
    Write-Host "  [..] ROCm (TheRock) installed but not active (HIP_PATH unset): $($rocmOnDisk.Keys -join ', ')" -ForegroundColor Yellow
} else {
    Write-Host "  [..] ROCm (TheRock) not installed" -ForegroundColor Yellow
}
foreach ($s in $manualSdks) {
    if (& $s.Probe) { Write-Host "  [OK] $($s.Name)" -ForegroundColor Green }
    else            { Write-Host "  [--] $($s.Name) not found (manual install)" -ForegroundColor Yellow }
}
Write-Host ""

# ── Decide the ROCm action ──────────────────────────────────────────
# Cheapest convergence first, which is what the version directories buy: a
# dist an earlier run already extracted only needs HIP_PATH moved onto it,
# and the pre-versioning tree is migrated by a rename. Downloading 4.6 GB is
# the last resort, not the first.

$rocmTarget  = $null   # dist to download and extract this run (Version/Url/Size)
$rocmSwitch  = $null   # version dir already on disk to point HIP_PATH at
$rocmRename  = $null   # pre-versioning tree to move into its version dir
$rocmBlocked = $null   # why the machine cannot converge to the pin this run

# Migrate C:\TheRock\build, but only when it holds a version we can NAME (an
# interrupted tree reads 'unknown', and guessing a name for it would be worse
# than leaving it alone) and nothing already occupies the destination.
$rocmLegacyVer = Get-RocmDirVersion $rocmLegacyDir
if ($rocmLegacyVer -and $rocmLegacyVer -ne 'unknown' -and
    -not (Test-Path (Get-RocmVersionDir $rocmLegacyVer))) {
    $rocmRename = @{ Version = $rocmLegacyVer
                     From    = $rocmLegacyDir
                     To      = (Get-RocmVersionDir $rocmLegacyVer) }
}

# Where a version sits once this run's rename has happened.
function Resolve-RocmDir([string]$Version) {
    if ($rocmRename -and $rocmRename.Version -eq $Version) { return $rocmRename.To }
    if ($rocmOnDisk.Contains($Version)) { return $rocmOnDisk[$Version] }
    return $null
}

# HIP_PATH on a tree that is not the version's own directory (the legacy one,
# with the migrated copy already beside it): prefer the version dir, so the
# leftover is inactive and free to delete. Get-RocmInstalled ranks a proper
# version dir above the legacy one for the same version.
if ($rocmBefore -and $rocmOnDisk.Contains($rocmBefore) -and $rocmOnDisk[$rocmBefore] -ne $rocmBeforeDir) {
    $rocmSwitch = $rocmOnDisk[$rocmBefore]
}

if ($rocmBefore -ne $rocm.Pin) {
    $pinDir = Resolve-RocmDir $rocm.Pin
    if ($pinDir) {
        $rocmSwitch = $pinDir
    } else {
        foreach ($d in $rocm.Dists) {
            $size = Get-RemoteFileSize $d.Url
            if ($size) { $rocmTarget = @{ Version = $d.Version; Url = $d.Url; Size = $size }; break }
        }
        if (-not $rocmTarget) {
            $rocmBlocked = 'no dist URL reachable (offline? not yet published?)'
        } else {
            # A reachable candidate that is NOT the pin means the pin is not
            # published yet. Those entries are there for a machine that has
            # nothing; one already serving a dist keeps it and waits.
            if ($rocmTarget.Version -ne $rocm.Pin) {
                $rocmBlocked = "pinned $($rocm.Pin) not published yet"
                if ($rocmBefore) { $rocmTarget = $null }
            }
            if ($rocmTarget) {
                $alt = Resolve-RocmDir $rocmTarget.Version   # extracted, never activated
                if ($alt) { $rocmSwitch = $alt; $rocmTarget = $null }
            }
        }
    }
}
if (Get-Process llama-server -ErrorAction SilentlyContinue) {
    # A rename moves the tree a running llama-server loaded its ROCm DLLs
    # from, and an install into an EXISTING version dir wipes it first;
    # never yank either out from under it. Extracting a version the machine
    # does not have is safe while it runs: side by side is the whole point.
    if ($rocmRename) {
        $rocmBlocked = 'llama-server is running - stop it and re-run (the dist directory has to be renamed)'
        if ($rocmSwitch -eq $rocmRename.To) { $rocmSwitch = $null }
        $rocmRename = $null
    }
    if ($rocmTarget -and (Test-Path (Get-RocmVersionDir $rocmTarget.Version))) {
        $rocmBlocked = 'llama-server is running - stop it and re-run'
        $rocmTarget  = $null
    }
}

# The directory HIP_PATH names after this run. The elevated env leg gates it
# on the marker, so a failed download leaves the machine exactly where it was.
$rocmActiveDir = $rocmBeforeDir
if ($rocmRename -and $rocmActiveDir -eq $rocmRename.From) { $rocmActiveDir = $rocmRename.To }
if ($rocmSwitch) { $rocmActiveDir = $rocmSwitch }
if ($rocmTarget) { $rocmActiveDir = Get-RocmVersionDir $rocmTarget.Version }

if ($rocmRename) {
    Write-Host "ROCm (TheRock) $($rocmRename.Version): $($rocmRename.From) -> $($rocmRename.To) (version directories)" -ForegroundColor Cyan
}
if ($rocmTarget) {
    $gb = [math]::Round($rocmTarget.Size / 1GB, 1)
    Write-Host "ROCm (TheRock) $($rocmTarget.Version) will be installed to $(Get-RocmVersionDir $rocmTarget.Version) ($gb GB download)" -ForegroundColor Cyan
    $drive  = (Split-Path -Qualifier $rocm.Root).TrimEnd(':')
    $freeGB = [math]::Round((Get-PSDrive $drive).Free / 1GB, 1)
    if ($freeGB -lt 40) {
        Write-Host "  warning: only $freeGB GB free on ${drive}: (download + extract want ~40 GB)" -ForegroundColor Yellow
    }
} elseif ($rocmSwitch -and $rocmSwitch -ne $rocmBeforeDir) {
    Write-Host "ROCm (TheRock): HIP_PATH moves to $rocmSwitch (already installed, nothing to download)" -ForegroundColor Cyan
}
if ($rocmBlocked) {
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
# directly under lib\. Idempotent: safe to re-run after any OpenSSL touch.
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
# ROCm: migrate the pre-versioning tree, download + extract when converging to
# the pin, and set the machine env every run (idempotent, and it self-heals a
# previously declined elevation).
if ($rocmTarget -or $rocmRename -or $rocmActiveDir) {
    $blocks += "`$rocmRoot = '$($rocm.Root)'; `$rocmMarker = '$($rocm.Marker)'; `$rocmActiveDir = '$rocmActiveDir'"
    if ($rocmRename) {
        # Same bytes, new name: a rename converts the pre-versioning layout in
        # a second, where a re-download costs 4.6 GB and half an hour.
        $blocks += "`$rocmFrom = '$($rocmRename.From)'; `$rocmTo = '$($rocmRename.To)'"
        $blocks += @'

Write-Host "Moving the ROCm dist into its version directory..." -ForegroundColor Cyan
if ((Test-Path $rocmFrom) -and -not (Test-Path $rocmTo)) {
    Move-Item -LiteralPath $rocmFrom -Destination $rocmTo -ErrorAction SilentlyContinue
    if (Test-Path $rocmTo) {
        Write-Host "  $rocmFrom -> $rocmTo" -ForegroundColor Green
    } else {
        Write-Host "  move failed (files in use?); close whatever uses ROCm and re-run" -ForegroundColor Red
    }
}
'@
    }
    if ($rocmTarget) {
        $blocks += "`$rocmDir = '$(Get-RocmVersionDir $rocmTarget.Version)'"
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
        Write-Host "  download failed (curl exit $LASTEXITCODE); partial file kept, a re-run resumes it" -ForegroundColor Red
    }
}
if ($haveTar) {
    if (Test-Path $rocmDir) {
        # Only ever an incomplete tree of THIS version: every other version
        # owns its own directory and is left where it is.
        Write-Host "  clearing an incomplete $rocmVer tree at $rocmDir..." -ForegroundColor DarkGray
        Remove-Item $rocmDir -Recurse -Force -ErrorAction SilentlyContinue
    }
    if (Test-Path $rocmDir) {
        Write-Host "  cannot clear $rocmDir (files in use?); close whatever uses ROCm and re-run" -ForegroundColor Red
    } else {
        New-Item -ItemType Directory -Force -Path $rocmDir | Out-Null
        Write-Host "  extracting to $rocmDir (several GB, takes a while)..." -ForegroundColor DarkGray
        tar.exe -xzf $rocmTar -C $rocmDir --strip-components=1
        if ($LASTEXITCODE -eq 0) {
            Set-Content -Path (Join-Path $rocmDir $rocmMarker) -Value $rocmVer
            Remove-Item $rocmTar -Force
            Write-Host "  ROCm (TheRock) $rocmVer installed" -ForegroundColor Green
        } else {
            Write-Host "  extraction failed (tar exit $LASTEXITCODE); tarball kept for retry" -ForegroundColor Red
        }
    }
}
'@
    }
    $blocks += @'

# Machine environment for the ACTIVE dist: HIP_PATH ONLY, gated on the marker
# so a failed install never points it at a broken tree. The compile-time vars
# (HIP_DEVICE_LIB_PATH, HIP_PLATFORM, LLVM_PATH) are deliberately NOT set
# machine-wide: the Adrenalin driver's own HIP runtime (System32's
# amdhip64_7.dll + amd_comgr_3.dll) reads LLVM_PATH at RUNTIME, and pointing
# it at TheRock's newer LLVM half-breaks it: hipMemGetInfo starts returning
# "invalid argument" (devices report 0 MiB) and every llama-server dies with
# an access violation (0xC0000005) mid weight-upload. Found 2026-07-16 with
# Adrenalin + TheRock 7.14; builds get all three per-process from common.ps1.
# The removal below self-heals machines poisoned by earlier versions of this
# script.
if ($rocmActiveDir -and (Test-Path (Join-Path $rocmActiveDir $rocmMarker))) {
    Write-Host "Setting ROCm machine environment..." -ForegroundColor Cyan
    [Environment]::SetEnvironmentVariable('HIP_PATH', $rocmActiveDir, 'Machine')
    foreach ($legacy in 'HIP_DEVICE_LIB_PATH', 'HIP_PLATFORM', 'LLVM_PATH') {
        if ([Environment]::GetEnvironmentVariable($legacy, 'Machine')) {
            [Environment]::SetEnvironmentVariable($legacy, $null, 'Machine')
            Write-Host "  removed machine-wide $legacy (breaks the driver HIP runtime)" -ForegroundColor DarkGray
        }
    }
    # PATH goes through the raw registry: setx /M truncates PATH at 1024 chars,
    # and [Environment]::SetEnvironmentVariable rewrites REG_EXPAND_SZ as
    # REG_SZ, breaking %SystemRoot%-style entries other software put there.
    # Every OTHER version dir is pruned out of it, and that is not tidiness:
    # amdhip64_*.dll reachable from two PATH directories is an ambiguous load
    # order, the classic cause of silent crashes in a multi-backend build.
    $key = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey('SYSTEM\CurrentControlSet\Control\Session Manager\Environment', $true)
    $path = [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    $keep  = @("$rocmActiveDir\bin", "$rocmActiveDir\lib\llvm\bin")
    $under = $rocmRoot.TrimEnd('\') + '\'
    $parts = @()
    foreach ($p in @($path -split ';' | Where-Object { $_ })) {
        $t = $p.TrimEnd('\')
        if ($t.StartsWith($under, [StringComparison]::OrdinalIgnoreCase) -and ($keep -notcontains $t)) {
            Write-Host "  PATH -= $p (other ROCm version)" -ForegroundColor DarkGray
            continue
        }
        $parts += $p
    }
    foreach ($add in $keep) {
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

# ── Check AMD for a stable newer than the pin ───────────────────────
# Report-only, and deliberately so (installer\dist-pins.psd1 carries the why).
# Best effort: an unreachable index is not a failure, it just leaves the row
# out, so an offline run reports the same as it always did.

Write-Host ""
Write-Host "Checking AMD for a newer ROCm stable..." -ForegroundColor Cyan
$rocmPublished = Get-RocmLatestPublished
$rocmNewer = $null
if ($rocmPublished -and [version]$rocm.Pin -lt $rocmPublished.Parsed) { $rocmNewer = $rocmPublished }

# ── Check llama.cpp source for a newer release tag ──────────────────
# The clone sits on a detached HEAD (02-build.ps1 pins it to a vX.Y.Z release
# tag), so no pull here, just fetch and compare against the newest RELEASE tag
# reachable from origin/master. The selection has to be the same one
# 02-build.ps1 makes, or this report recommends a rebuild for a nightly that
# build would never check out: highest `^v\d+\.\d+\.\d+$` among the tags merged
# into master, version-sorted, nightlies and pre-releases ignored. The rationale
# for each half of that lives at the selection in 02-build.ps1.

$latestLlama = $null
if ($cfg -and $beforeLlama) {
    Write-Host ""
    Write-Host "Checking llama.cpp for updates..." -ForegroundColor Cyan
    git -C $cfg.LlamaCppDir fetch origin --tags
    if ($LASTEXITCODE -ne 0) {
        Write-Host "  git fetch failed in $($cfg.LlamaCppDir)" -ForegroundColor Yellow
    } else {
        # Set/restore EAP around the stderr redirect (same PS 5.1 rationale as
        # Get-WingetVersion; this one runs at script scope, not in a function).
        $prevEap = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        $latestLlama = (git -C $cfg.LlamaCppDir tag --list 'v[0-9]*' --merged origin/master --sort=-v:refname 2>$null |
            ForEach-Object { $_.Trim() } |
            Where-Object { $_ -match '^v\d+\.\d+\.\d+$' } |
            Select-Object -First 1)
        $ErrorActionPreference = $prevEap
    }
}

# ── Capture post-state ──────────────────────────────────────────────

$after = @{}
foreach ($p in $wingetPackages) { $after[$p.Id] = Get-WingetVersion $p.Id }
# Straight from the MACHINE scope, never Get-RocmActiveDir: this process' own
# HIP_PATH is the pre-run value (a switch or a rename just moved the machine
# one, and the process copy is fixed up right below).
$rocmAfterDir = [Environment]::GetEnvironmentVariable('HIP_PATH', 'Machine')
if ($rocmAfterDir) { $rocmAfterDir = $rocmAfterDir.TrimEnd('\') }
$rocmAfter    = Get-RocmDirVersion $rocmAfterDir
$rocmOnDisk   = Get-RocmInstalled

# Make the dist visible to THIS session too (a 01-configure.ps1 run in the
# same console). HIP_PATH/PATH reach new terminals via the machine env; the
# three compile-time vars are PROCESS-scoped on purpose (machine-wide
# LLVM_PATH breaks the driver HIP runtime; see the elevated block) and
# build consoles get them from common.ps1.
if ($rocmAfter -and $rocmAfter -ne 'unknown') {
    $env:HIP_PATH            = $rocmAfterDir
    $env:HIP_DEVICE_LIB_PATH = "$rocmAfterDir\lib\llvm\amdgcn\bitcode"
    $env:HIP_PLATFORM        = 'amd'
    $env:LLVM_PATH           = "$rocmAfterDir\lib\llvm"
    # Drop the previous version's entries first: two amdhip64_*.dll dirs on
    # PATH is an ambiguous load order (the machine PATH is pruned the same
    # way in the elevated leg).
    $under = $rocm.Root.TrimEnd('\') + '\'
    $keep  = @("$rocmAfterDir\bin", "$rocmAfterDir\lib\llvm\bin")
    $env:PATH = (@($env:PATH -split ';' | Where-Object {
        $_ -and (-not $_.TrimEnd('\').StartsWith($under, [StringComparison]::OrdinalIgnoreCase) -or
                 $keep -contains $_.TrimEnd('\'))
    }) -join ';')
    foreach ($add in $keep) {
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

# HipPath is what build\config-build.psd1 pins (compiler path included), so
# the "re-run 01-configure" verdict follows the DIRECTORY, not the version:
# migrating the pre-versioning tree moves HIP_PATH without changing a digit.
$rocmChanged = ($rocmBeforeDir -ne $rocmAfterDir)
if (-not $rocmBefore -and $rocmAfter) {
    Write-ReportRow "[++]" Green "ROCm (TheRock)" "installed $rocmAfter in $rocmAfterDir"
} elseif (-not $rocmBefore -and -not $rocmAfter) {
    $detail = 'install failed'
    if ($rocmBlocked) { $detail = "not installed: $rocmBlocked" }
    Write-ReportRow "[!!]" Red "ROCm (TheRock)" $detail
} elseif ($rocmBefore -ne $rocmAfter) {
    Write-ReportRow "[++]" Green "ROCm (TheRock)" "$rocmBefore -> $rocmAfter ($rocmAfterDir)"
} elseif ($rocmChanged) {
    Write-ReportRow "[++]" Green "ROCm (TheRock)" "$rocmAfter moved to $rocmAfterDir"
} elseif ($rocmAfter -eq $rocm.Pin) {
    Write-ReportRow "[OK]" DarkGray "ROCm (TheRock)" $rocmAfter
} else {
    $detail = if ($rocmBlocked) { "$rocmAfter ($rocmBlocked)" } else { "$rocmAfter (pin $($rocm.Pin))" }
    Write-ReportRow "[..]" Yellow "ROCm (TheRock)" $detail
}
$rocmOther = @($rocmOnDisk.Keys | Where-Object { $rocmOnDisk[$_] -ne $rocmAfterDir })
if ($rocmOther.Count) {
    # Never deleted here: an inactive dist is a working rollback (switching
    # back is a HIP_PATH move), and 25 GB is the user's call, not the script's.
    Write-ReportRow "    " DarkGray "" "inactive dists in $($rocm.Root): $($rocmOther -join ', ') (~25 GB each, delete the directory to reclaim)"
}
if ($rocmNewer) {
    # Names the version and stops: converging is an edit of the pin, not
    # something this run did or is about to do (Recommendations says how).
    Write-ReportRow "    " Yellow "" "newer stable published: $($rocmNewer.Version) (pin is $($rocm.Pin))"
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
# silent crashes in multi-backend builds; surface the known offenders.

$envWarnings = @()
if (Test-Path "${env:ProgramFiles}\AMD\ROCm") {
    $envWarnings += "legacy AMD HIP SDK still under ${env:ProgramFiles}\AMD\ROCm; uninstall it (duplicate HIP runtimes)"
}
$userHip = [Environment]::GetEnvironmentVariable('HIP_PATH', 'User')
if ($userHip -and ($userHip.TrimEnd('\') -ne $rocmAfterDir)) {
    $envWarnings += "user-level HIP_PATH ($userHip) shadows the machine one; remove it"
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
            $envWarnings += "$scope-level $name ($v) breaks the driver HIP runtime (0xC0000005 at model load): remove it; builds set it per-process via common.ps1"
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
    Write-Host "  ROCm dist changed: verify with hipInfo.exe in a NEW terminal (all GPUs should list)." -ForegroundColor DarkGray
    Write-Host "  01-configure.ps1 re-derives GpuTargets from the dist itself; the kernel set is:" -ForegroundColor DarkGray
    Write-Host "    dir $rocmAfterDir\.kpack\blas_lib_gfx*.kpack" -ForegroundColor DarkGray
    Write-Host ""
}
if ($rocmNewer) {
    Write-Host "  ROCm $($rocmNewer.Version) is published (pin $($rocm.Pin)). Taking it is a deliberate edit of" -ForegroundColor DarkGray
    Write-Host "  installer\dist-pins.psd1: set Rocm.Pin and add the dist above the current one, then re-run" -ForegroundColor DarkGray
    Write-Host "    @{ Version = '$($rocmNewer.Version)'" -ForegroundColor DarkGray
    Write-Host "       Url = '$($rocmNewer.Url)' }" -ForegroundColor DarkGray
    Write-Host "  Check patches\hip\ first: a clang resource major with no patched wrapper fails 02-build.ps1" -ForegroundColor DarkGray
    Write-Host "  by design (patches\hip\README.md has the regeneration recipe)." -ForegroundColor DarkGray
    Write-Host ""
}
if (-not $cfg) {
    Write-Host "  Next: .\01-configure.ps1   # detect paths and generate build\config-build.psd1" -ForegroundColor Cyan
} elseif ($rocmChanged -or $rebuildLlama) {
    Write-Host "  Recommended actions:" -ForegroundColor Yellow
    if ($rocmChanged) {
        Write-Host "    .\01-configure.ps1        # HipPath is now $rocmAfterDir" -ForegroundColor Yellow
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
