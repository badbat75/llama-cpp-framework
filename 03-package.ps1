#requires -Version 7
# Package llama.cpp binaries + llama-cpp-config into an NSIS installer
# Requires: a successful build (02-build.ps1) and NSIS
# (PowerShell 7 pin: under 5.1 `Set-Content -Encoding UTF8` would BOM the
# generated .nsi, and the shared scripts assume pwsh semantics throughout.)

. "$PSScriptRoot\common.ps1"  # loads $cfg, adds ROCm to PATH
Enable-VsDevShell             # cmake --install needs the VS env

$ErrorActionPreference = 'Stop'

# ── Resolve versions ────────────────────────────────────────────────
# Framework version = the llama-cpp-config crate version. The configurator and
# the framework as a whole are versioned together (starting at 1.0.0); this is
# the headline version shown in the installer and the package name.
$cargoTomlPath = Join-Path $PSScriptRoot 'llama-cpp-config\Cargo.toml'
$cargoToml = Get-Content $cargoTomlPath -Raw
if ($cargoToml -match '(?ms)^\[package\].*?^\s*version\s*=\s*"([^"]+)"') {
    $frameworkVersion = $Matches[1]
}
else {
    throw "Could not read [package] version from $cargoTomlPath"
}

# llama build = the identity of the bundled llama.cpp checkout, which since
# 2026-08 is its RELEASE tag (e.g. v0.2.0) rather than a bNNNN nightly.
#
# `git describe --tags` is the source, and the tag it reports is the release one
# even on a commit carrying both: upstream cuts `bNNNN` lightweight and `vX.Y.Z`
# annotated, and describe prefers an annotated tag over a lightweight one at the
# same commit. So no --match is needed to steer it; the regex below only decides
# whether to TRUST the answer.
#
# The objection this used to raise, that a product version is not a build
# identity because `v0.2.0` names every commit from the tag until the next
# release, no longer applies: 02-build.ps1 detaches onto the tag itself, so the
# checkout IS the release commit and describe returns the bare tag. Should that
# ever not hold (a hand-moved HEAD, a build from an untagged tree), describe
# returns `v0.2.0-11-g1234567` or nothing, the regex rejects it and the fallback
# names the checkout by BUILD number instead: cmake/build-info.cmake derives
# LLAMA_BUILD_NUMBER from this very `rev-list --count` and llama-server prints
# it as `build N`. That fallback is deliberately shaped `bNNNN`, unlike anything
# this script now produces on the happy path, so a package built off a release
# cannot be mistaken for one that was not.
Push-Location $cfg.LlamaCppDir
$llamaBuild = (git describe --tags 2>$null | Select-Object -First 1)
if ($llamaBuild) { $llamaBuild = $llamaBuild.Trim() }
if ($llamaBuild -notmatch '^v\d+\.\d+\.\d+$') {
    $buildNumber = (git rev-list --count HEAD 2>$null | Select-Object -First 1)
    $llamaBuild = if ($buildNumber) { "b$($buildNumber.Trim())" } else { "b0-$(git rev-parse --short HEAD)" }
    Write-Host "llama.cpp checkout is not on a release tag; naming it by build number" -ForegroundColor Yellow
}
Pop-Location

# Architecture token for the package name (native 64-bit build).
$arch = switch ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture) {
    'Arm64' { 'arm64' }
    default { 'x64' }
}

Write-Host "Framework version: $frameworkVersion" -ForegroundColor Cyan
Write-Host "llama.cpp release: $llamaBuild" -ForegroundColor Cyan
Write-Host "Architecture:      $arch" -ForegroundColor Cyan

# ── Ensure NSIS is installed ────────────────────────────────────────
$nsisExe = $null
$nsisSearchPaths = @(
    "${env:ProgramFiles}\NSIS\makensis.exe"
    "${env:ProgramFiles(x86)}\NSIS\makensis.exe"
)
foreach ($p in $nsisSearchPaths) {
    if (Test-Path $p) { $nsisExe = $p; break }
}

if (-not $nsisExe) {
    Write-Host "NSIS not found. Installing via winget..." -ForegroundColor Yellow
    winget install --id NSIS.NSIS --accept-source-agreements --accept-package-agreements
    if ($LASTEXITCODE -ne 0) { throw "Failed to install NSIS" }
    foreach ($p in $nsisSearchPaths) {
        if (Test-Path $p) { $nsisExe = $p; break }
    }
    if (-not $nsisExe) { throw "NSIS installed but makensis.exe not found. Try restarting the shell." }
}
Write-Host "NSIS: $nsisExe" -ForegroundColor Cyan

# ── Stage llama.cpp binaries with cmake --install ───────────────────
$buildDir  = Join-Path $PSScriptRoot "build\llama.cpp-cmake"
$stageDir  = Join-Path $PSScriptRoot "build\staging"
$outputDir = Join-Path $PSScriptRoot "dist"

if (Test-Path $stageDir) { Remove-Item $stageDir -Recurse -Force }
New-Item -ItemType Directory -Path $stageDir -Force | Out-Null
New-Item -ItemType Directory -Path $outputDir -Force | Out-Null

Write-Host "Staging llama.cpp binaries..." -ForegroundColor Cyan
cmake --install $buildDir --prefix $stageDir
if ($LASTEXITCODE -ne 0) { throw "cmake --install failed" }

# ── Drop llama.cpp's own test binaries ──────────────────────────────
# `cmake --install` stages every built executable, which includes 26 `test-*`
# harnesses (~11 MB): test-backend-ops, test-tokenizer-0, test-chat-template
# and friends. They are llama.cpp's unit tests, useless to an end user and
# actively unwanted here, because the installer offers to put `bin\` on the
# SYSTEM PATH: shipping them puts 26 generically-named `test-*` commands in
# every shell on the machine. Pruned here rather than filtered in the .nsi so
# the staging tree is exactly what ships, which is also what the OpenSSL import
# scan below then has to read.
$testBins = @(Get-ChildItem (Join-Path $stageDir 'bin') -Filter 'test*.exe' -File)
if ($testBins) {
    $testBins | Remove-Item -Force
    Write-Host ("Pruned {0} llama.cpp test binaries ({1:N1} MB)" -f
        $testBins.Count, (($testBins | Measure-Object Length -Sum).Sum / 1MB)) -ForegroundColor DarkGray
}

# ── Stage the OpenSSL runtime next to llama-server.exe ──────────────
# llama-server links OpenSSL dynamically as NORMAL imports (libcrypto-N-x64 /
# libssl-N-x64); on a machine without an OpenSSL install it fails at load, so
# those DLLs must ship in bin\ (the application dir wins the DLL search order,
# so a system OpenSSL of any other version cannot conflict).
#
# The ABI-versioned names are read from the staged binaries' import tables
# (plain ASCII in the PE image, no tooling needed) rather than hard-coded,
# because the ABI number moves with the OpenSSL install the build linked
# against. What is scanned is EVERY staged .exe and .dll, not a name pattern:
# llama.cpp v0.2.0 split each tool into a thin launcher plus an impl DLL
# (`llama-server.exe` is 9 KB and imports `llama-server-impl.dll`, which is
# where the OpenSSL imports moved, alongside a new `llama-common.dll`). The
# previous scan looked at `llama-server*` and survived that only by accident of
# the wildcard; a rename to something else, or the imports migrating into
# another shared DLL, would have staged NOTHING and shipped a server that dies
# at load on any machine without OpenSSL. Scanning everything has no such
# assumption to break.
#
# Read in chunks rather than whole files: `ggml-hip.dll` alone is ~900 MB
# (20 gfx targets), and ReadAllBytes + ASCII.GetString would cost ~2.8 GB of
# RAM for that one file. The overlap carried between chunks is longer than any
# name matched, so a name straddling a chunk boundary is still seen.
#
# The ordinal IndexOf gate before the regex is what makes scanning everything
# affordable, and it is not a micro-optimization: measured on ggml-hip.dll,
# regex-per-chunk takes 32.9 s where the gated version takes 0.9 s (the whole
# tree: 44 s against ~3 s). IndexOf ordinal is vectorized and none of the GPU
# kernel blobs contain the prefix, so the regex only ever runs on the handful
# of chunks that could actually match.
function Find-OpenSslImports {
    param([Parameter(Mandatory)][string] $Path)
    $overlap = 64
    $found = [System.Collections.Generic.HashSet[string]]::new()
    $stream = [System.IO.File]::OpenRead($Path)
    try {
        $buffer = [byte[]]::new(4MB)
        $tail = ''
        while (($read = $stream.Read($buffer, 0, $buffer.Length)) -gt 0) {
            $chunk = $tail + [System.Text.Encoding]::ASCII.GetString($buffer, 0, $read)
            if ($chunk.IndexOf('libcrypto-', [StringComparison]::Ordinal) -ge 0 -or
                $chunk.IndexOf('libssl-', [StringComparison]::Ordinal) -ge 0) {
                foreach ($m in [regex]::Matches($chunk, '(?:libcrypto|libssl)-\d+-x64\.dll')) {
                    [void]$found.Add($m.Value)
                }
            }
            $tail = if ($chunk.Length -gt $overlap) { $chunk.Substring($chunk.Length - $overlap) } else { $chunk }
        }
    } finally { $stream.Dispose() }
    return $found
}

# name -> the staged binaries importing it, so a failure below can say WHERE a
# name came from instead of leaving the next reader to grep 1.2 GB by hand.
$sslSources = @{}
foreach ($bin in Get-ChildItem (Join-Path $stageDir 'bin') -File -Recurse |
         Where-Object { $_.Extension -in '.exe', '.dll' }) {
    foreach ($name in Find-OpenSslImports $bin.FullName) {
        if (-not $sslSources.ContainsKey($name)) {
            $sslSources[$name] = [System.Collections.Generic.List[string]]::new()
        }
        $sslSources[$name].Add($bin.Name)
    }
}
if ($sslSources.Count -eq 0) {
    throw ("No OpenSSL import names found in ANY staged binary: import scan broken or llama.cpp " +
           "stopped linking OpenSSL; refusing to package a server that may not start on clean machines")
}
foreach ($name in ($sslSources.Keys | Sort-Object)) {
    $src = Join-Path $cfg.OpenSSLDir "bin\$name"
    if (-not (Test-Path $src)) {
        throw ("OpenSSL runtime $name not found in $($cfg.OpenSSLDir)\bin, but $($sslSources[$name] -join ', ') " +
               "imports it and would fail to load on machines without OpenSSL")
    }
    Copy-Item $src -Destination (Join-Path $stageDir 'bin') -Force
    Write-Host "Staged $name (OpenSSL runtime, imported by $($sslSources[$name] -join ', '))" -ForegroundColor DarkGray
}

# ── Stage the runtime-deps helper + shared dist pins ────────────────
# install-runtime-deps.ps1 installs on END-USER machines what we deliberately
# do not bundle (VC++ redist; ROCm/TheRock for AMD; cuBLAS redist for NVIDIA).
# The installer's finish page offers to run it; it stays in bin\ for later
# re-runs. dist-pins.psd1 is the same pins file 00-install-prerequisites.ps1
# reads: one source of truth for the pinned dist versions/URLs.
foreach ($f in 'installer\install-runtime-deps.ps1', 'installer\dist-pins.psd1') {
    Copy-Item (Join-Path $PSScriptRoot $f) -Destination (Join-Path $stageDir 'bin') -Force
    Write-Host "Staged $(Split-Path $f -Leaf)" -ForegroundColor DarkGray
}

# ── Stage llama-cpp-config (Rust binary) ────────────────────────────
# Straight from cargo's release output; 02-build.ps1 leaves it there, no copy.
$configExe = Join-Path $PSScriptRoot "llama-cpp-config\target\release\llama-cpp-config.exe"
if (-not (Test-Path $configExe)) {
    throw "llama-cpp-config.exe not found at $configExe. Run 02-build.ps1 first."
}
Copy-Item $configExe -Destination $stageDir -Force
Write-Host "Staged llama-cpp-config.exe" -ForegroundColor DarkGray

# Copy the icon for the installer. llama.ico is generated, not checked in:
# 02-build.ps1's cargo leg (build.rs) normally creates it; regenerate here if
# it has since gone missing.
$iconPath = Join-Path $PSScriptRoot "resources\llama.ico"
if (-not (Test-Path $iconPath)) {
    Write-Host "llama.ico missing - regenerating from the llama.cpp webui logo..." -ForegroundColor Cyan
    Push-Location (Join-Path $PSScriptRoot "resources")
    try {
        if (-not (Test-Path "node_modules\sharp-ico")) {
            npm install --no-save sharp sharp-ico | Out-Null
            if ($LASTEXITCODE -ne 0) { throw "npm install failed" }
        }
        node generate-llama-ico.mjs
        if ($LASTEXITCODE -ne 0) { throw "generate-llama-ico.mjs failed" }
    } finally { Pop-Location }
}
Copy-Item $iconPath -Destination $stageDir -Force

# ── Generate .nsi from template ─────────────────────────────────────
$templatePath = Join-Path $PSScriptRoot "installer\llama-cpp-framework.nsi.template"
$nsiPath      = Join-Path $PSScriptRoot "build\llama-cpp.nsi"
# e.g. llama-cpp-framework-v1.11.2-v0.2.0-x64-setup.exe. Two vX.Y.Z tokens, and
# the order names them: the framework's own version first, the bundled llama.cpp
# release second.
$installerName = "llama-cpp-framework-v$frameworkVersion-$llamaBuild-$arch-setup.exe"
$outputFile   = Join-Path $outputDir $installerName

$stageDirNsis = $stageDir -replace '/', '\'
$outputFileNsis = $outputFile -replace '/', '\'

# .Replace(): literal substitution; -replace would treat the pattern as a
# regex and expand $ sequences in the replacement (paths, versions).
$nsiContent = (Get-Content $templatePath -Raw).
    Replace('@VERSION@',     [string]$frameworkVersion).
    Replace('@LLAMA_BUILD@', [string]$llamaBuild).
    Replace('@STAGING_DIR@', [string]$stageDirNsis).
    Replace('@OUTPUT_FILE@', [string]$outputFileNsis)

Set-Content -Path $nsiPath -Value $nsiContent -Encoding UTF8
Write-Host "Generated: $nsiPath" -ForegroundColor Cyan

# ── Build installer ─────────────────────────────────────────────────
Write-Host "Building installer..." -ForegroundColor Cyan
& $nsisExe $nsiPath
if ($LASTEXITCODE -ne 0) { throw "makensis failed (exit code $LASTEXITCODE)" }

# ── Cleanup ─────────────────────────────────────────────────────────
Remove-Item $nsiPath -Force
Remove-Item $stageDir -Recurse -Force

$size = [math]::Round((Get-Item $outputFile).Length / 1MB, 1)
Write-Host ""
Write-Host "Installer created: $outputFile ($size MB)" -ForegroundColor Green
Write-Host ""
