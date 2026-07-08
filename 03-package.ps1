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

# llama build = git describe of the bundled llama.cpp checkout (e.g. b3456).
Push-Location $cfg.LlamaCppDir
$llamaBuild = (git describe --tags 2>$null) -replace '^v', ''
if (-not $llamaBuild) { $llamaBuild = "b0-$(git rev-parse --short HEAD)" }
Pop-Location

# Architecture token for the package name (native 64-bit build).
$arch = switch ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture) {
    'Arm64' { 'arm64' }
    default { 'x64' }
}

Write-Host "Framework version: $frameworkVersion" -ForegroundColor Cyan
Write-Host "llama build:       $llamaBuild" -ForegroundColor Cyan
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

# ── Stage llama-cpp-config (Rust binary) ────────────────────────────
# Straight from cargo's release output — 02-build.ps1 leaves it there, no copy.
$configExe = Join-Path $PSScriptRoot "llama-cpp-config\target\release\llama-cpp-config.exe"
if (-not (Test-Path $configExe)) {
    throw "llama-cpp-config.exe not found at $configExe. Run 02-build.ps1 first."
}
Copy-Item $configExe -Destination $stageDir -Force
Write-Host "Staged llama-cpp-config.exe" -ForegroundColor DarkGray

# Copy the icon for the installer. llama.ico is generated, not checked in —
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
$templatePath = Join-Path $PSScriptRoot "llama-cpp.nsi.template"
$nsiPath      = Join-Path $PSScriptRoot "build\llama-cpp.nsi"
# e.g. llama-cpp-framework-v1.0.0-b3456-x64-setup.exe
$installerName = "llama-cpp-framework-v$frameworkVersion-$llamaBuild-$arch-setup.exe"
$outputFile   = Join-Path $outputDir $installerName

$stageDirNsis = $stageDir -replace '/', '\'
$outputFileNsis = $outputFile -replace '/', '\'

# .Replace() — literal substitution; -replace would treat the pattern as a
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
