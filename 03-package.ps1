# Package llama.cpp binaries into an NSIS installer
# Requires: a successful build (02-build.ps1) and NSIS

. "$PSScriptRoot\common.ps1"  # loads $cfg, activates VS Dev Shell + ROCm

$ErrorActionPreference = 'Stop'

# ── Resolve version from git ────────────────────────────────────────
Push-Location $cfg.LlamaCppDir
$version = (git describe --tags 2>$null) -replace '^v', ''
if (-not $version) { $version = "0.0.0-$(git rev-parse --short HEAD)" }
Pop-Location
Write-Host "Version: $version" -ForegroundColor Cyan

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
    # Refresh search
    foreach ($p in $nsisSearchPaths) {
        if (Test-Path $p) { $nsisExe = $p; break }
    }
    if (-not $nsisExe) { throw "NSIS installed but makensis.exe not found. Try restarting the shell." }
}
Write-Host "NSIS: $nsisExe" -ForegroundColor Cyan

# ── Stage files with cmake --install ────────────────────────────────
$buildDir  = Join-Path $PSScriptRoot "build"
$stageDir  = Join-Path $PSScriptRoot "staging"
$outputDir = Join-Path $PSScriptRoot "dist"

if (Test-Path $stageDir) { Remove-Item $stageDir -Recurse -Force }
New-Item -ItemType Directory -Path $stageDir -Force | Out-Null
New-Item -ItemType Directory -Path $outputDir -Force | Out-Null

Write-Host "Staging files..." -ForegroundColor Cyan
cmake --install $buildDir --prefix $stageDir
if ($LASTEXITCODE -ne 0) { throw "cmake --install failed" }

# Check Open WebUI venv and get version
$webuiVenv = Join-Path $PSScriptRoot "webui-venv"
$webuiVersion = ""
if (Test-Path "$webuiVenv\Scripts\open-webui.exe") {
    $venvPython = Join-Path $webuiVenv "Scripts\python.exe"
    $webuiVersion = & $venvPython -c "import importlib.metadata; print(importlib.metadata.version('open-webui'))" 2>$null
    Write-Host "Open WebUI: $webuiVersion ($webuiVenv)" -ForegroundColor Cyan
} else {
    $webuiVenv = ""
    Write-Host "Open WebUI venv not found — skipping (run 02-build-webui.ps1 first)" -ForegroundColor DarkGray
}

# ── Generate .nsi from template ─────────────────────────────────────
$templatePath = Join-Path $PSScriptRoot "llama-cpp.nsi.template"
$nsiPath      = Join-Path $PSScriptRoot "llama-cpp.nsi"
$installerName = "llama_cpp-$version"
if ($webuiVersion) { $installerName += "-webui-$webuiVersion" }
$installerName += "-win64-setup.exe"
$outputFile   = Join-Path $outputDir $installerName

$stageDirNsis = $stageDir -replace '/', '\'
$outputFileNsis = $outputFile -replace '/', '\'

$webuiDirNsis = if ($webuiVenv) { ($webuiVenv -replace '/', '\') } else { "" }

$nsiContent = (Get-Content $templatePath -Raw) `
    -replace '@VERSION@',     $version `
    -replace '@STAGING_DIR@', $stageDirNsis `
    -replace '@WEBUI_DIR@',   $webuiDirNsis `
    -replace '@OUTPUT_FILE@', $outputFileNsis

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
