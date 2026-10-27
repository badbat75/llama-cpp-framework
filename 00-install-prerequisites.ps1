# Install & update the llama.cpp build toolchain in one shot.
#
# winget packages (PowerShell 7+, OpenSSL, the .NET SDK) are installed if missing and
# upgraded if present, in a single self-elevated session (which also symlinks
# OpenSSL's lib\VC\x64\MD\*.lib up to lib\ so cmake's find_package(OpenSSL)
# resolves). OpenSSL is treated as a system library and stays where winget
# puts it.
#
# The GPU SDKs the build links against (ROCm/HIP, the CUDA Toolkit, the Vulkan
# SDK) are installed in that same session, every one of them as
#   <DistRoot>\<Name>\<Version>\windows      e.g. E:\TheRock\10.1.0\windows
# DistRoot defaults to the one in installer\dist-sources.psd1 and is
# overridden by -DistRoot, by LLAMA_FW_DIST_ROOT, or by what 01-configure.ps1
# recorded (dists.ps1). Versions sit side by side, and nothing is ever
# deleted. Nothing is pinned either: all three converge to the newest STABLE
# release their vendor publishes (dist-sources.psd1 says where each one is
# looked up). ROCm comes from AMD's TheRock dist tarball (the classic HIP SDK
# installer is discontinued) and is the only one with a machine environment:
# HIP_PATH on the ACTIVE version + PATH, pruned of the others, because a
# llama-server run from the build tree finds rocBLAS through it (the installed
# one carries its own runtime, bundled by 03-package.ps1). CUDA is assembled from
# NVIDIA's redist archives, Vulkan comes from LunarG's installer.
#
# sccache comes straight from its GitHub releases (user scope, no elevation)
# rather than winget, whose manifest lags upstream by days; a winget copy left
# by an earlier setup is uninstalled once the release copy is in place.
#
# The WiX Toolset that builds the MSI (03-package.ps1) is a .NET global tool,
# PINNED to the version in installer\wix-toolset.psd1 (user scope, no
# elevation), with the extensions the .wxs uses. WiX v7 needs its EULA accepted
# once per user (`wix eula accept wix7`): this script never does that on the
# user's behalf, it reports it.
#
# When build\windows\config-build.psd1 + llama.cpp clone exist, also fetches the
# source and flags a rebuild when a newer release tag (vX.Y.Z) is available. (No `git
# pull`: 02-build.ps1 pins the clone to a tag on a detached HEAD, so a pull
# would always fail; the checkout onto the new tag is 02-build.ps1's job.)
#
# Safe to run any time: idempotent.

[CmdletBinding()]
param(
    [string]$DistRoot  # where the SDK dists live (<DistRoot>\<Name>\<Version>\windows). If omitted: LLAMA_FW_DIST_ROOT, config-build.psd1, dist-sources.psd1
)

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

# The WiX pin (version, extensions, EULA id) and the .NET SDK that hosts it.
$wixPin = Import-PowerShellDataFile (Join-Path $PSScriptRoot 'installer\wix-toolset.psd1')

$wingetPackages = @(
    @{ Id = 'Microsoft.PowerShell'    ; Name = 'PowerShell 7+' }
    @{ Id = 'ShiningLight.OpenSSL.Dev'; Name = 'OpenSSL' }
    @{ Id = $wixPin.DotNetSdkWingetId ; Name = '.NET SDK' }
)

$sources = Import-PowerShellDataFile (Join-Path $PSScriptRoot 'installer\dist-sources.psd1')
. (Join-Path $PSScriptRoot 'dists.ps1')

# Read directly, not through common.ps1, which throws until 01-configure.ps1
# has written the file: this script is the one that runs before it.
$cfgPath = Join-Path $PSScriptRoot 'build\windows\config-build.psd1'
$cfg = if (Test-Path $cfgPath) { Import-PowerShellDataFile $cfgPath } else { $null }

# Every SDK dist below installs as <DistRoot>\<Dir>\<Version>\<DistOs>.
$distRoot = Resolve-DistRoot $DistRoot $cfg $sources
$distOs   = $sources.DistOs

# ── ROCm (TheRock) dist ─────────────────────────────────────────────
# AMD now distributes Windows ROCm/HIP as TheRock dist tarballs (the classic
# HIP SDK installer is discontinued). Not pinned: every run scans AMD's
# tarball indexes (installer\dist-sources.psd1) and
# converges the install to the newest STABLE dist (rationale for the
# multiarch tarball and the index scan lives next to the data).
#
# Dists install SIDE BY SIDE, one directory per version, and HIP_PATH names
# the active one. So converging to a version already extracted by an earlier
# run costs a HIP_PATH move and nothing else, and an upgrade never destroys
# the dist the machine is currently building against. One thing follows,
# handled below: the machine PATH must be PRUNED of the other dists' dirs (two
# amdhip64_*.dll dirs on PATH is the classic silent crash).
#
# A dist OUTSIDE the layout (HIP_PATH on C:\TheRock\<version>, what every
# install before 1.17.1 wrote) is not adopted: the run converges to the
# newest stable inside the layout, by a HIP_PATH move when that version is
# already there, by a download otherwise, and leaves the old tree where it
# is. Copy it to <DistRoot>\TheRock\<version>\windows first (its marker comes
# along) to save the 4.6 GB.
$rocm = $sources.Rocm
$rocmRoot = Get-DistHome 'Rocm' $sources $distRoot

function Get-RocmVersionDir([string]$Version) { Get-DistDir 'Rocm' $Version $sources $distRoot }

# The version a dist directory holds: the marker our install step writes;
# else the dist's own .info\version (covers a manual install; note it says
# "7.14.0" even for an rc build, which is why our marker takes precedence);
# else 'unknown' for an interrupted/unversioned tree ('unknown' never parses
# as a version, so the next run converges it to the newest stable).
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
# copy); a HIP_PATH outside the layout is reported too, it is still what a
# llama-server run from the build tree loads until this run moves it.
function Get-RocmActiveDir {
    foreach ($hp in @($env:HIP_PATH, [Environment]::GetEnvironmentVariable('HIP_PATH', 'Machine'))) {
        if ($hp -and (Get-RocmDirVersion $hp.TrimEnd('\'))) { return $hp.TrimEnd('\') }
    }
    return $null
}

# Every COMPLETE dist in the layout, version -> path. Complete means the
# marker is there: a tree without it is an interrupted extract, which the
# install leg clears and redoes, never something to point HIP_PATH at.
function Get-RocmInstalled {
    $found = [ordered]@{}
    foreach ($dir in @(Get-ChildItem $rocmRoot -Directory -ErrorAction SilentlyContinue | Sort-Object Name)) {
        $d = Join-Path $dir.FullName $distOs
        if (-not (Test-Path (Join-Path $d $rocm.Marker))) { continue }
        $v = Get-RocmDirVersion $d
        if ($v -and -not $found.Contains($v)) { $found[$v] = $d }
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
# or $null when none is reachable. It is what this run installs.
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

# ── CUDA Toolkit, Vulkan SDK ────────────────────────────────────────
# Both converge to the newest stable release, each version in its own
# directory of the layout. 02-build.ps1 picks the newest COMPLETE dist by
# version, so a new one is used by the next build with no reconfigure; an old
# one stays as a fallback for the user to delete. "Installed" here is
# Find-Dist (dists.ps1), the same test the build applies, so it means
# "buildable"; an SDK anywhere else on the machine does not count.
$cuda   = $sources.Cuda
$vulkan = $sources.Vulkan

# The newest CUDA release NVIDIA publishes as Parsed/Version/Parts, or $null.
# The redist index carries one redistrib_<X.Y.Z>.json per release (the same
# strict X.Y.Z match as the ROCm scan), and that manifest names every
# component's archive and sha256. Parts is the archive (Url/Sha256) of each of
# the sources' Components: all of them are FOUND in the manifest, so a release
# that lacks one for Windows is simply not taken this run.
function Get-CudaLatestPublished {
    # Same PS 5.1 redirected-stderr rationale as Get-WingetVersion.
    $ErrorActionPreference = 'Continue'
    $page = curl.exe -sL --fail --connect-timeout 5 --max-time 20 $cuda.RedistIndex 2>$null | Out-String
    if ($LASTEXITCODE -ne 0 -or -not $page) { return $null }
    $best = $null
    foreach ($m in [regex]::Matches($page, 'redistrib_(\d+\.\d+\.\d+)\.json')) {
        $v = [version]$m.Groups[1].Value
        if (-not $best -or $v -gt $best) { $best = $v }
    }
    if (-not $best) { return $null }
    $json = curl.exe -sL --fail --max-time 30 ($cuda.RedistIndex + "redistrib_$best.json") 2>$null | Out-String
    if ($LASTEXITCODE -ne 0 -or -not $json) { return $null }
    try { $manifest = ConvertFrom-Json $json } catch { return $null }
    $parts = @()
    foreach ($name in $cuda.Components) {
        $entry = $manifest.$name.($cuda.Platform)
        if (-not $entry -or -not $entry.relative_path -or $entry.sha256 -notmatch '^[0-9a-fA-F]{64}$') { return $null }
        $parts += @{ Url = $cuda.RedistIndex + $entry.relative_path; Sha256 = $entry.sha256 }
    }
    return @{ Parsed = $best; Version = "$best"; Parts = $parts }
}

# The newest Vulkan SDK LunarG publishes for Windows as Parsed/Version/Url/
# Size, or $null (offline, or the installer not downloadable yet).
function Get-VulkanLatestPublished {
    # Same PS 5.1 redirected-stderr rationale as Get-WingetVersion.
    $ErrorActionPreference = 'Continue'
    $text = curl.exe -sL --fail --connect-timeout 5 --max-time 20 $vulkan.LatestUrl 2>$null | Out-String
    if ($LASTEXITCODE -ne 0 -or $text.Trim() -notmatch '^(\d+\.\d+\.\d+\.\d+)$') { return $null }
    $ver  = $Matches[1]
    $url  = $vulkan.Installer -f $ver
    $size = Get-RemoteFileSize $url
    if (-not $size) { return $null }
    return @{ Parsed = [version]$ver; Version = $ver; Url = $url; Size = $size }
}

# ── sccache (GitHub release) ────────────────────────────────────────
# Installed from mozilla/sccache's own release assets, not winget: the winget
# manifest lags upstream by days (0.18.0 was published 2026-09-14 and winget
# still served 0.17.0 four days later), and sccache is the tool whose releases
# the nvcc wrapping in 02-build.ps1 waits on. It tracks the LATEST release,
# like everything else here.
#
# The directory is fixed and versionless on purpose. winget's portable layout
# puts the version in the path (...\sccache-v0.17.0-x86_64-pc-windows-msvc\),
# and 02-build.ps1 hands that full path to CMake as the compiler launcher, so
# every upgrade changed the command line of every C/C++ object and ninja re-ran
# all of them. 02-build.ps1 looks here before PATH for the same reason: a
# console opened before this run can still resolve the old copy from PATH.
$sccacheHome = Join-Path $env:LOCALAPPDATA 'Programs\sccache'
$sccacheExe  = Join-Path $sccacheHome 'sccache.exe'
$sccacheArch = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'aarch64' } else { 'x86_64' }

# "sccache 0.18.0" -> "0.18.0"; $null when the exe is missing or unreadable.
function Get-SccacheVersion([string]$Exe) {
    # Same PS 5.1 redirected-stderr rationale as Get-WingetVersion.
    $ErrorActionPreference = 'Continue'
    if (-not $Exe -or -not (Test-Path $Exe)) { return $null }
    $out = & $Exe --version 2>$null | Out-String
    if ($out -match 'sccache\s+(\d+\.\d+\.\d+)') { return $Matches[1] }
    return $null
}

# The newest sccache release for this arch as Version/Url/Sha256, or $null
# when GitHub cannot say (offline, or the unauthenticated API limit of 60
# requests an hour). /releases/latest already skips drafts and pre-releases.
# The zip is FOUND in the release's asset list rather than composed from the
# tag, and its sha256 comes from the API's `digest`, falling back to the
# .sha256 file sccache publishes beside every asset.
function Get-SccacheLatest {
    # Same PS 5.1 redirected-stderr rationale as Get-WingetVersion.
    $ErrorActionPreference = 'Continue'
    # curl.exe explicitly: PS 5.1 aliases `curl` to Invoke-WebRequest.
    $json = curl.exe -sL --fail --connect-timeout 5 --max-time 20 'https://api.github.com/repos/mozilla/sccache/releases/latest' 2>$null | Out-String
    if ($LASTEXITCODE -ne 0 -or -not $json) { return $null }
    try { $release = $json | ConvertFrom-Json } catch { return $null }
    if ($release.tag_name -notmatch '^v(\d+\.\d+\.\d+)$') { return $null }
    $version = $Matches[1]
    $name  = "sccache-v$version-$sccacheArch-pc-windows-msvc.zip"
    $asset = $release.assets | Where-Object { $_.name -eq $name } | Select-Object -First 1
    if (-not $asset) { return $null }
    $sha = $null
    if ($asset.digest -match '^sha256:([0-9a-fA-F]{64})$') {
        $sha = $Matches[1]
    } else {
        $sidecar = $release.assets | Where-Object { $_.name -eq "$name.sha256" } | Select-Object -First 1
        if ($sidecar) {
            $text = curl.exe -sL --fail --max-time 20 $sidecar.browser_download_url 2>$null | Out-String
            if ($LASTEXITCODE -eq 0 -and $text -match '([0-9a-fA-F]{64})') { $sha = $Matches[1] }
        }
    }
    if (-not $sha) { return $null }
    return @{ Version = $version; Url = $asset.browser_download_url; Sha256 = $sha.ToLowerInvariant() }
}

# Stop the server of every sccache copy we know of: it keeps its exe mapped,
# which blocks both replacing ours and uninstalling winget's, and a build that
# died mid-way leaves one running. No server running is not an error.
function Stop-SccacheServers {
    # Same PS 5.1 redirected-stderr rationale as Get-WingetVersion.
    $ErrorActionPreference = 'Continue'
    foreach ($exe in @($sccacheExe, $sccacheOnPath | Where-Object { $_ -and (Test-Path $_) } | Select-Object -Unique)) {
        & $exe --stop-server 2>$null | Out-Null
    }
}

# Download, verify and put the release's sccache.exe at $sccacheExe. Returns
# $null on success, else the reason (the report prints it). The exe is staged
# beside the target and MOVED over it, so a download or copy that dies halfway
# never leaves a truncated sccache.exe, and one still in use (a build running)
# stays exactly as it was.
function Install-Sccache($Latest) {
    # Same PS 5.1 redirected-stderr rationale as Get-WingetVersion.
    $ErrorActionPreference = 'Continue'
    $work = Join-Path $env:TEMP "sccache-v$($Latest.Version)-install"
    if (Test-Path $work) { Remove-Item $work -Recurse -Force }
    New-Item -ItemType Directory -Force -Path $work | Out-Null
    try {
        $zip = Join-Path $work (Split-Path $Latest.Url -Leaf)
        curl.exe --fail -sSL --retry 3 --retry-delay 5 -o $zip $Latest.Url
        if ($LASTEXITCODE -ne 0) { return "download failed (curl exit $LASTEXITCODE)" }
        $hash = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($hash -ne $Latest.Sha256) { return "sha256 mismatch on $(Split-Path $zip -Leaf) (got $hash)" }
        tar.exe -xf $zip -C $work
        if ($LASTEXITCODE -ne 0) { return "extraction failed (tar exit $LASTEXITCODE)" }
        $new = Get-ChildItem $work -Recurse -Filter 'sccache.exe' | Select-Object -First 1
        if (-not $new) { return "no sccache.exe in $(Split-Path $zip -Leaf)" }

        Stop-SccacheServers
        New-Item -ItemType Directory -Force -Path $sccacheHome | Out-Null
        $staged = "$sccacheExe.new"
        Copy-Item $new.FullName $staged -Force
        Move-Item $staged $sccacheExe -Force -ErrorAction SilentlyContinue
        if (Test-Path $staged) {
            Remove-Item $staged -Force
            return "$sccacheExe is in use (a build running?); re-run once it finishes"
        }
        return $null
    } finally {
        Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# Append $Dir to the USER PATH; $true when it changed anything. Raw registry,
# for the reason the machine PATH goes through it in the elevated leg: the
# value must stay REG_EXPAND_SZ (a fresh profile's own entry is
# %USERPROFILE%\AppData\Local\Microsoft\WindowsApps).
function Add-UserPathEntry([string]$Dir) {
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
    try {
        $path  = [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        $parts = @($path -split ';' | Where-Object { $_ })
        if (@($parts | Where-Object { $_.TrimEnd('\') -eq $Dir.TrimEnd('\') }).Count) { return $false }
        $key.SetValue('Path', (@($parts + $Dir) -join ';'), [Microsoft.Win32.RegistryValueKind]::ExpandString)
    } finally {
        $key.Close()
    }
    # A raw registry write notifies nobody (unlike
    # [Environment]::SetEnvironmentVariable), so tell Explorer, or consoles it
    # starts would not see the entry until the next logon.
    if (-not ('Win32.EnvBroadcast' -as [type])) {
        Add-Type -Namespace Win32 -Name EnvBroadcast -MemberDefinition @'
[DllImport("user32.dll", CharSet = CharSet.Unicode)]
public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint msg, UIntPtr wParam, string lParam, uint flags, uint timeout, out UIntPtr result);
'@
    }
    $result = [UIntPtr]::Zero
    # HWND_BROADCAST, WM_SETTINGCHANGE, SMTO_ABORTIFHUNG, 5 s
    [void][Win32.EnvBroadcast]::SendMessageTimeout([IntPtr]0xffff, 0x1A, [UIntPtr]::Zero, 'Environment', 2, 5000, [ref]$result)
    return $true
}

# ── WiX Toolset (dotnet global tool) ────────────────────────────────
# The tool lands in %USERPROFILE%\.dotnet\tools, which the .NET SDK puts on the
# user PATH at its first run; a console opened before the SDK install has
# neither on PATH, hence the fixed fallbacks.
$wixExe = Join-Path $env:USERPROFILE '.dotnet\tools\wix.exe'

function Get-DotnetExe {
    $cmd = Get-Command dotnet -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty Source
    if ($cmd) { return $cmd }
    $fixed = Join-Path $env:ProgramFiles 'dotnet\dotnet.exe'
    if (Test-Path $fixed) { return $fixed }
    return $null
}

# "7.0.1+b3f3403" -> "7.0.1"; $null when the tool is missing or unreadable.
function Get-WixVersion {
    if (-not (Test-Path $wixExe)) { return $null }
    $ErrorActionPreference = 'Continue'   # PS 5.1 + redirected stderr: see Get-WingetVersion
    $out = & $wixExe --version 2>$null | Out-String
    if ($out -match '(\d+\.\d+\.\d+)') { return $Matches[1] }
    return $null
}

# Install or move the tool to the pinned version, then add the extensions the
# .wxs uses. Returns $null when WiX is ready to build, else what is missing
# (the report prints it). Never accepts the EULA.
function Install-Wix {
    $ErrorActionPreference = 'Continue'
    $dotnet = Get-DotnetExe
    if (-not $dotnet) { return '.NET SDK not found (winget leg failed?); re-run this script' }
    $current = Get-WixVersion
    if ($current -ne $wixPin.Version) {
        Write-Host "  installing WiX $($wixPin.Version) (dotnet global tool)" -ForegroundColor Cyan
        if ($current) {
            # --allow-downgrade: the pin is the version, newer is not better here.
            & $dotnet tool update --global wix --version $wixPin.Version --allow-downgrade 2>&1 | Out-Host
        } else {
            & $dotnet tool install --global wix --version $wixPin.Version 2>&1 | Out-Host
        }
        if ($LASTEXITCODE -ne 0) { return "dotnet tool install wix $($wixPin.Version) failed (exit $LASTEXITCODE)" }
    }
    $toolsDir = Split-Path $wixExe
    if (@($env:PATH -split ';') -notcontains $toolsDir) { $env:PATH = "$toolsDir;$env:PATH" }
    # Any command but --version doubles as the EULA check (WIX7015 until accepted).
    $list = & $wixExe extension list -g 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0) {
        if ($list -match 'WIX7015') {
            return "EULA not accepted: read https://opensourcemaintenancefee.org, then run: wix eula accept $($wixPin.EulaId) (and re-run this script for the extensions)"
        }
        return "wix extension list failed: $($list.Trim())"
    }
    foreach ($ext in $wixPin.Extensions) {
        if ($list -match "(?m)^$([regex]::Escape($ext)) $([regex]::Escape($wixPin.Version))\b") { continue }
        Write-Host "  adding WiX extension $ext/$($wixPin.Version)" -ForegroundColor Cyan
        & $wixExe extension add -g "$ext/$($wixPin.Version)" 2>&1 | Out-Host
        if ($LASTEXITCODE -ne 0) { return "wix extension add $ext/$($wixPin.Version) failed (exit $LASTEXITCODE)" }
    }
    return $null
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
# sccache as 02-build.ps1 resolves it: our directory first, then PATH.
$sccacheOnPath = Get-Command sccache -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty Source
$sccacheWinget = Get-WingetVersion 'Mozilla.sccache'
$sccacheBefore = Get-SccacheVersion $sccacheExe
$sccacheBeforeFrom = $sccacheHome
if (-not $sccacheBefore -and $sccacheOnPath) {
    $sccacheBefore     = Get-SccacheVersion $sccacheOnPath
    $sccacheBeforeFrom = if ($sccacheWinget) { 'winget' } else { Split-Path $sccacheOnPath }
}

$wixBefore = Get-WixVersion

$beforeLlama = if ($cfg) { Get-GitDescribe $cfg.LlamaCppDir } else { $null }

Write-Host "  SDK dists under $distRoot (<Name>\<Version>\$distOs)" -ForegroundColor DarkGray
# The layout is trusted: the build runs the compiler found there, the elevated
# session unpacks into it, and the active ROCm dist goes on the machine PATH.
# A dist every account can write lets any of them swap a compiler or a DLL, so
# the elevated session LOCKS each Windows dist (Protect-DistDir: administrators
# and SYSTEM full control, everyone else read + execute) and nothing else: the
# roots above them also hold the other OS's dists and keep the ACL their owner
# gave them. Get-DistBroadWriters names who else can write a directory; it is
# shared with the elevated batch, hence a string.
$distAclFunctions = @'
function Get-DistBroadWriters([string]$Dir) {
    $broad = @('S-1-1-0', 'S-1-5-11', 'S-1-5-32-545')   # Everyone, Authenticated Users, Users
    $writeBits = [Security.AccessControl.FileSystemRights]'WriteData, AppendData, Delete, DeleteSubdirectoriesAndFiles, ChangePermissions, TakeOwnership'
    try {
        return @((Get-Acl -LiteralPath $Dir).Access | Where-Object {
            $_.AccessControlType -eq 'Allow' -and ($_.FileSystemRights -band $writeBits) -and
            ($broad -contains $_.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value)
        } | ForEach-Object { $_.IdentityReference.Value } | Select-Object -Unique)
    } catch { return @() }
}
function Protect-DistDir([string]$Dir) {
    if (-not (Test-Path -LiteralPath $Dir) -or -not @(Get-DistBroadWriters $Dir).Count) { return }
    # Owner first: a tree an unelevated account created is OWNED by it, and an
    # owner can always rewrite the ACL below. Then inheritance off at the dist
    # directory and three explicit entries that its whole tree inherits (the
    # files carry no ACL of their own).
    icacls.exe $Dir /setowner '*S-1-5-32-544' /T /C /Q | Out-Null
    $owned = ($LASTEXITCODE -eq 0)
    icacls.exe $Dir /inheritance:r /grant:r '*S-1-5-32-544:(OI)(CI)F' '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-545:(OI)(CI)RX' /Q | Out-Null
    if ($LASTEXITCODE -ne 0) { Write-Host "  could not lock $Dir (icacls exit $LASTEXITCODE)" -ForegroundColor Red }
    elseif (-not $owned)     { Write-Host "  locked $Dir, but its owner could not be changed to Administrators" -ForegroundColor Yellow }
    else                     { Write-Host "  locked $Dir (write access: administrators only)" -ForegroundColor DarkGray }
}
'@
. ([scriptblock]::Create($distAclFunctions))
# Every Windows dist of the layout, complete or not. Only directories named
# like a version, the ones Find-Dist can ever return: anything else under a
# root other accounts may write is not ours to touch.
function Get-LayoutDistDirs {
    foreach ($name in 'Rocm', 'Cuda', 'Vulkan') {
        $distHome = Get-DistHome $name $sources $distRoot
        if (-not (Test-Path $distHome)) { continue }
        foreach ($v in (Get-ChildItem $distHome -Directory -ErrorAction SilentlyContinue)) {
            if ($v.Name -notmatch '^\d+(\.\d+){1,3}$') { continue }
            $d = Join-Path $v.FullName $distOs
            if (Test-Path $d) { $d }
        }
    }
}
$distUnlocked = @(Get-LayoutDistDirs | Where-Object { @(Get-DistBroadWriters $_).Count })
foreach ($d in $distUnlocked) {
    Write-Host "  [..] $d is writable by $((Get-DistBroadWriters $d) -join ', '): will be locked" -ForegroundColor Yellow
}
foreach ($p in $wingetPackages) {
    $v = $before[$p.Id]
    if ($v) { Write-Host "  [OK] $($p.Name) $v" -ForegroundColor Green }
    else    { Write-Host "  [..] $($p.Name) not installed" -ForegroundColor Yellow }
}
if ($sccacheBefore) { Write-Host "  [OK] sccache $sccacheBefore ($sccacheBeforeFrom)" -ForegroundColor Green }
else                { Write-Host "  [..] sccache not installed" -ForegroundColor Yellow }
if ($wixBefore) { Write-Host "  [OK] WiX Toolset $wixBefore" -ForegroundColor Green }
else            { Write-Host "  [..] WiX Toolset not installed" -ForegroundColor Yellow }
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
$cudaBefore   = Find-Dist 'Cuda' $sources $distRoot
$vulkanBefore = Find-Dist 'Vulkan' $sources $distRoot
if ($cudaBefore) { Write-Host "  [OK] CUDA Toolkit $($cudaBefore.Version) ($($cudaBefore.Dir))" -ForegroundColor Green }
else             { Write-Host "  [..] CUDA Toolkit not installed" -ForegroundColor Yellow }
if ($vulkanBefore) { Write-Host "  [OK] Vulkan SDK $($vulkanBefore.Version) ($($vulkanBefore.Dir))" -ForegroundColor Green }
else               { Write-Host "  [..] Vulkan SDK not installed" -ForegroundColor Yellow }
Write-Host ""

# ── Resolve the newest stable releases ──────────────────────────────
# Best effort: an unreachable source keeps what is installed and says so in
# the report; it never uninstalls or downgrades anything.

Write-Host "Checking AMD, NVIDIA and LunarG for the newest stable releases..." -ForegroundColor DarkGray
$rocmLatest   = Get-RocmLatestPublished
$cudaLatest   = Get-CudaLatestPublished
$vulkanLatest = Get-VulkanLatestPublished

$cudaTarget = $null
if ($cudaLatest -and (-not $cudaBefore -or $cudaBefore.Parsed -lt $cudaLatest.Parsed)) {
    $cudaTarget = $cudaLatest
    $from = if ($cudaBefore) { "$($cudaBefore.Version) -> " } else { '' }
    Write-Host "CUDA Toolkit $from$($cudaTarget.Version) will be installed to $(Get-DistDir 'Cuda' $cudaTarget.Version $sources $distRoot) ($($cudaTarget.Parts.Count) redist archives, the display driver is left alone)" -ForegroundColor Cyan
}
$vulkanTarget = $null
if ($vulkanLatest -and (-not $vulkanBefore -or $vulkanBefore.Parsed -lt $vulkanLatest.Parsed)) {
    $vulkanTarget = $vulkanLatest
    $from = if ($vulkanBefore) { "$($vulkanBefore.Version) -> " } else { '' }
    $mb = [math]::Round($vulkanTarget.Size / 1MB)
    Write-Host "Vulkan SDK $from$($vulkanTarget.Version) will be installed to $(Get-DistDir 'Vulkan' $vulkanTarget.Version $sources $distRoot) ($mb MB download)" -ForegroundColor Cyan
}

# ── Decide the ROCm action ──────────────────────────────────────────
# Cheapest convergence first, which is what the version directories buy: a
# dist an earlier run already extracted only needs HIP_PATH moved onto it.
# Downloading 4.6 GB is the last resort, not the first.

$rocmTarget  = $null   # dist to download and extract this run (Version/Url/Size)
$rocmSwitch  = $null   # version dir already on disk to point HIP_PATH at
$rocmBlocked = $null   # why the machine cannot converge to the newest stable this run

# Converge to the newest stable, never AWAY from a newer one: a version that
# does not parse ('unknown', a hand-installed nightly such as 10.2.0a2026...)
# is replaced, a parsed one only when it is older. An active dist OUTSIDE the
# layout is replaced whatever its version: the layout is where dists live.
$rocmBeforeParsed = $null
if ($rocmBefore) { [void][version]::TryParse($rocmBefore, [ref]$rocmBeforeParsed) }
$rocmInLayout = $rocmBeforeDir -and (@($rocmOnDisk.Values) -contains $rocmBeforeDir)
if (-not $rocmLatest) {
    $rocmBlocked = 'newest stable not resolved (offline? index unreachable?)'
} elseif (-not $rocmInLayout -or -not $rocmBeforeParsed -or $rocmBeforeParsed -lt $rocmLatest.Parsed) {
    if ($rocmOnDisk.Contains($rocmLatest.Version)) {
        $rocmSwitch = $rocmOnDisk[$rocmLatest.Version]   # extracted earlier, never activated
    } else {
        $size = Get-RemoteFileSize $rocmLatest.Url
        if ($size) {
            $rocmTarget = @{ Version = $rocmLatest.Version; Url = $rocmLatest.Url; Size = $size }
        } else {
            $rocmBlocked = "$($rocmLatest.Version) is listed but its tarball is not downloadable yet"
        }
    }
}
if ($rocmTarget -and (Test-Path (Get-RocmVersionDir $rocmTarget.Version)) -and
    (Get-Process llama-server -ErrorAction SilentlyContinue)) {
    # An install into an EXISTING version dir wipes it first; never yank it
    # out from under a running llama-server. Extracting a version the machine
    # does not have is safe while it runs: side by side is the whole point.
    $rocmBlocked = 'llama-server is running - stop it and re-run'
    $rocmTarget  = $null
}

# The directory HIP_PATH names after this run. The elevated env leg gates it
# on the marker, so a failed download leaves the machine exactly where it was.
$rocmActiveDir = $rocmBeforeDir
if ($rocmSwitch) { $rocmActiveDir = $rocmSwitch }
if ($rocmTarget) { $rocmActiveDir = Get-RocmVersionDir $rocmTarget.Version }

if ($rocmTarget) {
    $gb = [math]::Round($rocmTarget.Size / 1GB, 1)
    Write-Host "ROCm (TheRock) $($rocmTarget.Version) will be installed to $(Get-RocmVersionDir $rocmTarget.Version) ($gb GB download)" -ForegroundColor Cyan
    $drive  = (Split-Path -Qualifier $rocmRoot).TrimEnd(':')
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

# ── Build the elevated batch (winget, ROCm, CUDA, Vulkan, symlinks) ─

# The batch is GENERATED code that runs elevated, and some of what goes into
# it was read from places this account does not control alone: directory names
# under a root other accounts may write, HIP_PATH, URLs out of a vendor's
# index. Every such value enters as a single-quoted literal built by Lit, with
# the quotes inside it doubled, so it can only ever be data. PowerShell's own
# escaper, because the parser also closes a string on the typographic quotes
# (U+2018 to U+201B), which a plain -replace "'" would let through.
function Lit([string]$Value) {
    return "'" + [System.Management.Automation.Language.CodeGeneration]::EscapeSingleQuotedStringContent($Value) + "'"
}

$blocks = @()
# A downloaded installer runs elevated only when Authenticode says it is
# intact AND signed by its vendor (the sources' Signer, matched against the
# certificate's O=): a valid signature by anyone else is not enough. The file
# sits in a user-writable .downloads directory, so it is held open (share =
# Read only: no write, rename or delete by anyone) from BEFORE the check until
# the installer exits; what was verified is what runs. Returns the exit code,
# $null when the installer was refused.
$blocks += @'
function Invoke-SignedInstaller([string]$File, [string]$Org, [string]$Arguments) {
    $lock = [IO.File]::Open($File, 'Open', 'Read', 'Read')
    try {
        $sig = Get-AuthenticodeSignature $File
        $subject = if ($sig.SignerCertificate) { $sig.SignerCertificate.Subject } else { '' }
        if ($sig.Status -ne 'Valid' -or $subject -notmatch ('(^|, )O="?' + [regex]::Escape($Org) + '"?(, |$)')) {
            Write-Host "  [!!] $(Split-Path $File -Leaf): signature $($sig.Status), signer '$subject' (expected O=$Org); NOT run" -ForegroundColor Red
            return $null
        }
        return (Start-Process $File -ArgumentList $Arguments -Wait -PassThru).ExitCode
    } finally { $lock.Dispose() }
}
'@
foreach ($p in $missing) {
    $blocks += "Write-Host 'Installing $($p.Name)...' -ForegroundColor Cyan"
    $blocks += "winget install --id $($p.Id) --exact --silent --accept-source-agreements --accept-package-agreements"
}
foreach ($p in $present) {
    $blocks += "Write-Host 'Upgrading $($p.Name)...' -ForegroundColor Cyan"
    $blocks += "winget upgrade --id $($p.Id) --exact --silent --accept-source-agreements --accept-package-agreements"
}
# ROCm: download + extract when converging to the newest stable, and set the
# machine env every run (idempotent, and it self-heals a previously declined
# elevation). $rocmPrevDir is the dist HIP_PATH named before this run: its
# PATH entries are pruned with the layout's, wherever it sits.
if ($rocmTarget -or $rocmActiveDir) {
    $blocks += "`$rocmRoot = $(Lit $rocmRoot); `$rocmMarker = $(Lit $rocm.Marker); `$rocmActiveDir = $(Lit $rocmActiveDir); `$rocmPrevDir = $(Lit $rocmBeforeDir)"
    if ($rocmTarget) {
        $blocks += "`$rocmDir = $(Lit (Get-RocmVersionDir $rocmTarget.Version))"
        $blocks += "`$rocmVer = $(Lit $rocmTarget.Version); `$rocmUrl = $(Lit $rocmTarget.Url); `$rocmSize = $([long]$rocmTarget.Size)"
        $blocks += @'

Write-Host "Installing ROCm (TheRock) $rocmVer..." -ForegroundColor Cyan
$rocmDl = Join-Path $rocmRoot '.downloads'
New-Item -ItemType Directory -Force -Path $rocmDl | Out-Null
$rocmTar = Join-Path $rocmDl (Split-Path $rocmUrl -Leaf)
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
    # Every OTHER dist's dir is pruned out of it (the layout's other versions
    # and the dist that was active before this run, which may sit outside
    # the layout), and that is not tidiness: amdhip64_*.dll reachable from
    # two PATH directories is an ambiguous load order, the classic cause of
    # silent crashes in a multi-backend build.
    $key = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey('SYSTEM\CurrentControlSet\Control\Session Manager\Environment', $true)
    $path = [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    $keep  = @("$rocmActiveDir\bin", "$rocmActiveDir\lib\llvm\bin")
    $under = @($rocmRoot.TrimEnd('\') + '\')
    if ($rocmPrevDir) { $under += $rocmPrevDir.TrimEnd('\') + '\' }
    $parts = @()
    foreach ($p in @($path -split ';' | Where-Object { $_ })) {
        $t = $p.TrimEnd('\')
        $other = @($under | Where-Object { $t.StartsWith($_, [StringComparison]::OrdinalIgnoreCase) }).Count -gt 0
        if ($other -and ($keep -notcontains $t)) {
            Write-Host "  PATH -= $p (other ROCm dist)" -ForegroundColor DarkGray
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
# CUDA Toolkit: assembled from NVIDIA's redist archives, not its installer
# (dist-sources.psd1 says why). Each archive is checked against the manifest's
# sha256 before it is unpacked, and unpacks as <name>-archive\{bin,include,
# lib,...}: stripping that one level merges them into a single toolkit
# prefix. Nothing is registered machine-wide (no CUDA_PATH, no PATH entry, no
# driver): 02-build.ps1 finds the toolkit in the layout. A finished download
# is reused by a re-run; the marker is written only once every component is
# in, so a tree without it is an interrupted install of this same version and
# is cleared first.
if ($cudaTarget) {
    $cudaHome = Get-DistHome 'Cuda' $sources $distRoot
    $blocks += "`$cudaVer = $(Lit $cudaTarget.Version); `$cudaDir = $(Lit (Get-DistDir 'Cuda' $cudaTarget.Version $sources $distRoot)); `$cudaMarker = $(Lit $cuda.Marker)"
    $blocks += "`$cudaDl = $(Lit (Join-Path (Join-Path $cudaHome '.downloads') $cudaTarget.Version))"
    $blocks += "`$cudaParts = @(" + (($cudaTarget.Parts | ForEach-Object { "@{ Url = $(Lit $_.Url); Sha256 = $(Lit $_.Sha256) }" }) -join ', ') + ')'
    $blocks += @'

Write-Host "Installing CUDA Toolkit $cudaVer (redist archives, no driver)..." -ForegroundColor Cyan
New-Item -ItemType Directory -Force -Path $cudaDl | Out-Null
if (Test-Path $cudaDir) { Remove-Item $cudaDir -Recurse -Force -ErrorAction SilentlyContinue }
if (Test-Path $cudaDir) {
    Write-Host "  cannot clear $cudaDir (files in use?); close whatever uses it and re-run" -ForegroundColor Red
} else {
    New-Item -ItemType Directory -Force -Path $cudaDir | Out-Null
    $cudaOk = $true
    foreach ($part in $cudaParts) {
        $zip = Join-Path $cudaDl (Split-Path $part.Url -Leaf)
        # The archive sits in a .downloads directory other accounts may be
        # able to write, and this session is elevated: it is held open (share
        # = Read only, as Invoke-SignedInstaller does) from BEFORE the hash
        # until tar is done, so the bytes that matched are the bytes
        # unpacked. A leftover that does not match is fetched once more.
        $state = 'missing'
        foreach ($attempt in 1, 2) {
            if (-not (Test-Path $zip)) {
                Write-Host "  $(Split-Path $zip -Leaf)" -ForegroundColor DarkGray
                curl.exe --fail -sSL --retry 3 --retry-delay 5 -o $zip $part.Url
                if ($LASTEXITCODE -ne 0) { $state = 'download'; break }
            }
            $lock = [IO.File]::Open($zip, 'Open', 'Read', 'Read')
            try {
                $hash = [BitConverter]::ToString([Security.Cryptography.SHA256]::Create().ComputeHash($lock)) -replace '-', ''
                if ($hash -eq $part.Sha256) {
                    tar.exe -xf $zip -C $cudaDir --strip-components=1
                    $state = if ($LASTEXITCODE -eq 0) { 'ok' } else { "tar exit $LASTEXITCODE" }
                } else { $state = 'sha256' }
            } finally { $lock.Dispose() }
            if ($state -ne 'sha256') { break }
            Remove-Item $zip -Force -ErrorAction SilentlyContinue
        }
        if ($state -ne 'ok') {
            Write-Host "  $(Split-Path $zip -Leaf): failed ($state)" -ForegroundColor Red
            $cudaOk = $false; break
        }
    }
    if ($cudaOk) {
        Set-Content -Path (Join-Path $cudaDir $cudaMarker) -Value $cudaVer
        Remove-Item $cudaDl -Recurse -Force -ErrorAction SilentlyContinue
        Write-Host "  CUDA Toolkit $cudaVer installed" -ForegroundColor Green
    } else {
        Write-Host "  CUDA Toolkit $cudaVer NOT installed; a re-run reuses the finished downloads" -ForegroundColor Red
    }
}
'@
}
# Vulkan SDK: LunarG's Qt installer, headless, into its own version directory
# (it sets VULKAN_SDK and PATH itself; the build reads neither). Checked
# against the expected size first, so a truncated download never runs. The
# marker is written only when the installer left an SDK the build can use.
if ($vulkanTarget) {
    $vkHome = Get-DistHome 'Vulkan' $sources $distRoot
    $blocks += "`$vkVer = $(Lit $vulkanTarget.Version); `$vkUrl = $(Lit $vulkanTarget.Url); `$vkSize = $([long]$vulkanTarget.Size); `$vkDir = $(Lit (Get-DistDir 'Vulkan' $vulkanTarget.Version $sources $distRoot)); `$vkSigner = $(Lit $vulkan.Signer)"
    $blocks += "`$vkDl = $(Lit (Join-Path $vkHome '.downloads')); `$vkMarker = $(Lit $vulkan.Marker)"
    $blocks += @'

Write-Host "Installing Vulkan SDK $vkVer..." -ForegroundColor Cyan
New-Item -ItemType Directory -Force -Path $vkDl | Out-Null
$vkExe = Join-Path $vkDl (Split-Path $vkUrl -Leaf)
curl.exe --fail -L --retry 3 --retry-delay 5 -o $vkExe $vkUrl
if ($LASTEXITCODE -ne 0 -or (Get-Item $vkExe -ErrorAction SilentlyContinue).Length -ne $vkSize) {
    Write-Host "  download failed or incomplete (curl exit $LASTEXITCODE)" -ForegroundColor Red
} else {
    $rc = Invoke-SignedInstaller $vkExe $vkSigner "--root `"$vkDir`" --accept-licenses --default-answer --confirm-command install"
    if ($null -ne $rc -and $rc -ne 0) { Write-Host "  Vulkan SDK installer exit code $rc" -ForegroundColor Red }
    if ((Test-Path (Join-Path $vkDir 'Lib\vulkan-1.lib')) -and (Test-Path (Join-Path $vkDir 'Bin\glslc.exe'))) {
        Set-Content -Path (Join-Path $vkDir $vkMarker) -Value $vkVer
        Write-Host "  Vulkan SDK $vkVer installed" -ForegroundColor Green
    }
}
Remove-Item $vkExe -Force -ErrorAction SilentlyContinue
'@
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
# Lock the Windows dists: the ones found unlocked above and the ones this run
# installs (a tree extracted under a root everyone can write inherits that).
$lockDirs = @($distUnlocked)
if ($rocmTarget)   { $lockDirs += Get-RocmVersionDir $rocmTarget.Version }
if ($cudaTarget)   { $lockDirs += Get-DistDir 'Cuda' $cudaTarget.Version $sources $distRoot }
if ($vulkanTarget) { $lockDirs += Get-DistDir 'Vulkan' $vulkanTarget.Version $sources $distRoot }
$lockDirs = @($lockDirs | Select-Object -Unique)
if ($lockDirs.Count) {
    $blocks += $distAclFunctions
    $blocks += "foreach (`$d in @(" + (($lockDirs | ForEach-Object { Lit $_ }) -join ', ') + ")) { Protect-DistDir `$d }"
}
$script = $blocks -join "`n"

if (Test-IsAdmin) {
    & ([scriptblock]::Create($script))
} else {
    Write-Host "Requesting administrator privileges for winget + ROCm + CUDA + Vulkan..." -ForegroundColor Yellow
    # Through a file, not -EncodedCommand: base64 of UTF-16 inflates the batch
    # ~2.7x, and with the ROCm, CUDA and Vulkan legs together it no longer
    # fits Windows' 32,767-character command line. The file sits in the
    # user-writable TEMP, so it is never run by path: the elevated side reads
    # its bytes ONCE, checks them against the SHA256 on its own command line
    # (which nothing unelevated can alter after the UAC prompt) and runs the
    # verified bytes from memory, so a swap after the write is refused, not
    # executed. Decoded as UTF-8 explicitly, so no BOM is needed.
    $elevated = Join-Path $env:TEMP "llama-cpp-framework-elevated-$PID.ps1"
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes($script)
    [IO.File]::WriteAllBytes($elevated, $bytes)
    $hash = [BitConverter]::ToString([Security.Cryptography.SHA256]::Create().ComputeHash($bytes)) -replace '-', ''
    $loader = "`$b = [IO.File]::ReadAllBytes($(Lit $elevated)); " +
        "if (([BitConverter]::ToString([Security.Cryptography.SHA256]::Create().ComputeHash(`$b)) -replace '-', '') -ne '$hash') " +
        "{ Write-Host 'Elevated batch was modified after it was written; refusing to run it.' -ForegroundColor Red; Start-Sleep 10; exit 3 }; " +
        "& ([scriptblock]::Create([Text.Encoding]::UTF8.GetString(`$b)))"
    try {
        $proc = Start-Process powershell -Verb RunAs -Wait -PassThru `
            -ArgumentList "-ExecutionPolicy Bypass -NoProfile -Command `"$($loader -replace '"', '\"')`""
        if ($proc.ExitCode -ne 0) {
            Write-Host "Elevated session exited with code $($proc.ExitCode)" -ForegroundColor Red
        }
    } finally {
        Remove-Item $elevated -Force -ErrorAction SilentlyContinue
    }
}

# ── sccache: install / update from the GitHub release ───────────────
# User scope, so it runs here and not in the elevated batch (an elevation
# with a different admin account would install into THAT profile).

Write-Host ""
Write-Host "Checking sccache releases..." -ForegroundColor Cyan
$sccacheIssue  = $null
$sccacheWingetRemoved = $false
$sccacheLatest = Get-SccacheLatest
$sccacheOurs   = Get-SccacheVersion $sccacheExe
if (-not $sccacheLatest) {
    $sccacheIssue = 'latest release not resolved (offline, API rate limit, or no verifiable Windows zip); kept what is installed'
} elseif (-not $sccacheOurs -or [version]$sccacheOurs -lt [version]$sccacheLatest.Version) {
    Write-Host "  installing sccache $($sccacheLatest.Version) to $sccacheHome" -ForegroundColor Cyan
    $sccacheIssue = Install-Sccache $sccacheLatest
}
if (Get-SccacheVersion $sccacheExe) {
    if (Add-UserPathEntry $sccacheHome) {
        Write-Host "  user PATH += $sccacheHome" -ForegroundColor DarkGray
    }
    if (@($env:PATH -split ';') -notcontains $sccacheHome) { $env:PATH = "$sccacheHome;$env:PATH" }
    # Only once our copy works, so a failed download never leaves the machine
    # with no sccache at all. Uninstalling also takes winget's directory off
    # the user PATH, where it sat AHEAD of ours.
    if ($sccacheWinget) {
        Write-Host "  removing the winget copy (Mozilla.sccache $sccacheWinget)..." -ForegroundColor Cyan
        Stop-SccacheServers
        winget uninstall --id Mozilla.sccache --exact --silent --accept-source-agreements
        if ($LASTEXITCODE -eq 0) {
            $sccacheWingetRemoved = $true
        } else {
            $sccacheIssue = "winget uninstall Mozilla.sccache failed (exit $LASTEXITCODE); remove it by hand, its PATH entry shadows $sccacheHome"
        }
    }
}

# ── WiX Toolset: install / pin / extensions ─────────────────────────
# User scope like sccache, after the elevated batch (which installs the .NET
# SDK the tool runs on).

Write-Host ""
Write-Host "Checking the WiX Toolset ($($wixPin.Version))..." -ForegroundColor Cyan
$wixIssue = Install-Wix

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
$cudaAfter    = Find-Dist 'Cuda' $sources $distRoot
$vulkanAfter  = Find-Dist 'Vulkan' $sources $distRoot

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
    # Drop the previous dist's entries first: two amdhip64_*.dll dirs on
    # PATH is an ambiguous load order (the machine PATH is pruned the same
    # way in the elevated leg).
    $under = @($rocmRoot.TrimEnd('\') + '\')
    if ($rocmBeforeDir) { $under += $rocmBeforeDir.TrimEnd('\') + '\' }
    $keep  = @("$rocmAfterDir\bin", "$rocmAfterDir\lib\llvm\bin")
    $env:PATH = (@($env:PATH -split ';' | Where-Object {
        $t = $_.TrimEnd('\')
        $_ -and (@($under | Where-Object { $t.StartsWith($_, [StringComparison]::OrdinalIgnoreCase) }).Count -eq 0 -or
                 $keep -contains $t)
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

# The copy 02-build.ps1 will pick: ours, else whatever PATH still offers.
$sccacheAfter = Get-SccacheVersion $sccacheExe
if (-not $sccacheAfter) { $sccacheAfter = Get-SccacheVersion $sccacheOnPath }
if (-not $sccacheAfter) {
    Write-ReportRow "[!!]" Red "sccache" "not installed"
} elseif (-not $sccacheBefore) {
    Write-ReportRow "[++]" Green "sccache" "installed $sccacheAfter (GitHub release)"
} elseif ($sccacheBefore -ne $sccacheAfter) {
    Write-ReportRow "[++]" Green "sccache" "$sccacheBefore -> $sccacheAfter (GitHub release)"
} elseif ($sccacheIssue) {
    Write-ReportRow "[..]" Yellow "sccache" $sccacheAfter
} else {
    Write-ReportRow "[OK]" DarkGray "sccache" $sccacheAfter
}
if ($sccacheIssue) {
    Write-ReportRow "    " Yellow "" $sccacheIssue
}
if ($sccacheWingetRemoved) {
    Write-ReportRow "    " DarkGray "" "winget copy removed; consoles opened earlier still have its PATH entry"
}

$wixAfter = Get-WixVersion
if (-not $wixAfter) {
    Write-ReportRow "[!!]" Red "WiX Toolset" "not installed"
} elseif ($wixIssue) {
    Write-ReportRow "[..]" Yellow "WiX Toolset" $wixAfter
} elseif ($wixBefore -ne $wixAfter) {
    $from = if ($wixBefore) { "$wixBefore -> " } else { 'installed ' }
    Write-ReportRow "[++]" Green "WiX Toolset" "$from$wixAfter (+ $($wixPin.Extensions -join ', '))"
} else {
    Write-ReportRow "[OK]" DarkGray "WiX Toolset" "$wixAfter (+ $($wixPin.Extensions -join ', '))"
}
if ($wixIssue) {
    Write-ReportRow "    " Yellow "" $wixIssue
}

# HipPath is what build\windows\config-build.psd1 pins (compiler path included), so
# the "re-run 01-configure" verdict follows the DIRECTORY, not the version:
# moving into the layout changes HIP_PATH without changing a digit.
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
} elseif ($rocmBlocked) {
    Write-ReportRow "[..]" Yellow "ROCm (TheRock)" "$rocmAfter ($rocmBlocked)"
} else {
    Write-ReportRow "[OK]" DarkGray "ROCm (TheRock)" $rocmAfter
}
$rocmOther = @($rocmOnDisk.Keys | Where-Object { $rocmOnDisk[$_] -ne $rocmAfterDir })
if ($rocmOther.Count) {
    # Never deleted here: an inactive dist is a working rollback (switching
    # back is a HIP_PATH move), and 25 GB is the user's call, not the script's.
    Write-ReportRow "    " DarkGray "" "inactive dists in ${rocmRoot}: $($rocmOther -join ', ') (~25 GB each, delete the directory to reclaim)"
}
if ($rocmBeforeDir -and $rocmBeforeDir -ne $rocmAfterDir -and (Test-Path $rocmBeforeDir) -and
    @($rocmOnDisk.Values) -notcontains $rocmBeforeDir) {
    Write-ReportRow "    " DarkGray "" "previous dist kept in $rocmBeforeDir, outside the layout (delete it to reclaim the space)"
}

# CUDA Toolkit and Vulkan SDK: the same verdicts, keyed on the newest
# complete dist in the layout (what the build will use), before vs after.
function Write-SdkRows([string]$Name, $Before, $After, $Latest, $Target) {
    if (-not $After) {
        $detail = if ($Target) { "install of $($Target.Version) failed" }
                  elseif (-not $Latest) { 'not installed: newest release not resolved (offline?)' }
                  else { 'not installed' }
        Write-ReportRow "[!!]" Red $Name $detail
    } elseif (-not $Before) {
        Write-ReportRow "[++]" Green $Name "installed $($After.Version) in $($After.Dir)"
    } elseif ($Before.Parsed -ne $After.Parsed) {
        Write-ReportRow "[++]" Green $Name "$($Before.Version) -> $($After.Version) ($($After.Dir))"
    } elseif ($Target) {
        Write-ReportRow "[!!]" Red $Name "$($After.Version) (install of $($Target.Version) failed)"
    } elseif (-not $Latest) {
        Write-ReportRow "[..]" Yellow $Name "$($After.Version) (newest release not resolved: offline?)"
    } else {
        Write-ReportRow "[OK]" DarkGray $Name $After.Version
    }
    if ($After -and $Before -and $Before.Dir -ne $After.Dir -and (Test-Path $Before.Dir)) {
        # Never deleted here, like an inactive ROCm dist: it is a fallback.
        Write-ReportRow "    " DarkGray "" "previous install kept in $($Before.Dir) (delete it to reclaim the space)"
    }
}
Write-SdkRows 'CUDA Toolkit' $cudaBefore $cudaAfter $cudaLatest $cudaTarget
Write-SdkRows 'Vulkan SDK' $vulkanBefore $vulkanAfter $vulkanLatest $vulkanTarget
$sdkChanged = ($cudaBefore.Version -ne $cudaAfter.Version) -or ($vulkanBefore.Version -ne $vulkanAfter.Version)
# A dist still writable by everyone after the elevated session (declined
# elevation, icacls failure) is a compiler any account can replace: say so.
foreach ($d in @(Get-LayoutDistDirs)) {
    $writers = @(Get-DistBroadWriters $d)
    if ($writers.Count) { Write-ReportRow "[!!]" Yellow "SDK dist" "$d is writable by $($writers -join ', '); re-run this script and accept the elevation to lock it" }
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
# The installed framework's bin\ is NOT a second dist, although it carries
# amdhip64_*.dll and its optional PATH component puts it on the system PATH:
# the installer bundles there the whole runtime of the dist it was BUILT
# against (03-package.ps1), a self-contained set that an exe resolves from its
# own directory before PATH is consulted, and PATH never decides
# amdhip64_7.dll for any other exe (its own dir, then the driver's System32
# copy, both precede PATH). It does not follow a HIP_PATH move and does not
# have to: it changes with the next installer.
# Doubled separators collapsed and a trailing one dropped, for comparison only:
# the MSI writes its PATH entry as ...\llama.cpp\bin\, and a hand-edited entry
# may differ in either.
function Get-NormalizedDir([string]$Dir) { ($Dir -replace '(?<!^)\\{2,}', '\').TrimEnd('\') }
$fwBin = $null
$fwDir = (Get-ItemProperty 'HKLM:\Software\llama.cpp' -ErrorAction SilentlyContinue).InstallDir
if ($fwDir -and (Test-Path (Join-Path $fwDir 'bin'))) { $fwBin = Get-NormalizedDir (Join-Path $fwDir 'bin') }
# amdhip64_*.dll in more than one PATH dir: a second DIST (legacy SDK, a
# hand-added TheRock), whose rocblas.dll/libhipblaslt.dll then resolve by
# PATH order. The driver's System32 copy and the installed one are set aside.
$pathAll = ([Environment]::GetEnvironmentVariable('Path', 'Machine'), [Environment]::GetEnvironmentVariable('Path', 'User')) -join ';'
$hipDllDirs = @($pathAll -split ';' | Where-Object { $_ } | ForEach-Object { Get-NormalizedDir $_ } | Select-Object -Unique |
    Where-Object { ($_ -notlike "$env:windir*") -and ($_ -ne $fwBin) -and (Test-Path (Join-Path $_ 'amdhip64*.dll')) })
if ($hipDllDirs.Count -gt 1) {
    $envWarnings += "amdhip64_*.dll in $($hipDllDirs.Count) PATH dirs (ambiguous load order): $($hipDllDirs -join '; ')"
}
if ($envWarnings.Count) {
    Write-Host ""
    foreach ($w in $envWarnings) { Write-Host "  [!!] $w" -ForegroundColor Yellow }
}

# ── Recommendations ─────────────────────────────────────────────────

Write-Host ""
if ($rocmChanged) {
    Write-Host "  ROCm dist changed: verify with hipInfo.exe in a NEW terminal (all GPUs should list)." -ForegroundColor DarkGray
    Write-Host "  01-configure.ps1 re-derives GpuTargets from the dist itself; the kernel set is:" -ForegroundColor DarkGray
    Write-Host "    dir $rocmAfterDir\.kpack\blas_lib_gfx*.kpack" -ForegroundColor DarkGray
    Write-Host ""
}
# 01-configure.ps1 has to be told the dist root only when it would not resolve
# this one by itself (a -DistRoot given here and recorded nowhere yet).
$configure = '.\01-configure.ps1'
if ($distRoot -ne (Resolve-DistRoot $null $cfg $sources)) { $configure += " -DistRoot `"$distRoot`"" }
$configStale = $cfg -and (($cfg.DistRoot -ne $distRoot) -or ($rocmAfterDir -and $cfg.HipPath -ne $rocmAfterDir))
if (-not $cfg) {
    Write-Host "  Next: $configure   # detect paths and generate build\windows\config-build.psd1" -ForegroundColor Cyan
} elseif ($rocmChanged -or $rebuildLlama -or $sdkChanged -or $configStale) {
    # 01-configure.ps1 records the dist root and HipPath (and the clang inside
    # it), nothing of CUDA or Vulkan: 02-build.ps1 finds the newest of those
    # itself.
    Write-Host "  Recommended actions (open a NEW terminal first: the installers changed the machine env):" -ForegroundColor Yellow
    if ($rocmChanged -or $configStale) {
        Write-Host "    $configure        # config-build.psd1 names other SDK paths" -ForegroundColor Yellow
    }
    $why = @()
    if ($rebuildLlama) { $why += 'newer llama.cpp release available' }
    if ($rocmChanged)  { $why += 'new ROCm' }
    if ($sdkChanged)   { $why += 'new CUDA/Vulkan SDK' }
    if (-not $why.Count) { $why += 'SDK paths changed' }
    Write-Host "    .\02-build.ps1            # $($why -join ', ')" -ForegroundColor Yellow
    Write-Host "    .\03-package.ps1          # rebuild installer afterwards" -ForegroundColor Yellow
} else {
    Write-Host "  Toolchain up to date." -ForegroundColor Green
}
Write-Host ""
