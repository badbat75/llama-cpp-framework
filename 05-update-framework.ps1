# Update framework dependencies, llama.cpp source, and Open WebUI source.
# Prints a report of what changed and recommends a rebuild when llama.cpp or
# Open WebUI moved to a new commit (the only sources that actually require
# recompilation; the rest are tools and don't end up in the shipped binary).
#
# Tools updated via winget:  Microsoft.PowerShell, ShiningLight.OpenSSL.Dev,
#                            Python.Python.3.12, Schniz.fnm, NSIS.NSIS
# Sources updated via git:   $cfg.LlamaCppDir, $cfg.OpenWebUIDir
# Not touched (manual SDKs): CUDA Toolkit, Vulkan SDK, AMD HIP / ROCm
#
# Run from an elevated terminal for unattended winget upgrades; otherwise
# winget may pop a UAC prompt for system-wide packages (PowerShell, OpenSSL,
# NSIS) or skip them silently.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$cfg = Import-PowerShellDataFile "$PSScriptRoot\config.psd1"

# ── Helpers ──────────────────────────────────────────────────────────

function Get-WingetVersion {
    param([string]$Id)
    # Returns the installed version of a winget package, or $null if not
    # installed. winget's table output is locale-dependent; matching the row
    # by Id prefix sidesteps that.
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

# ── Pre-update state ─────────────────────────────────────────────────

Write-Host ""
Write-Host "  llama.cpp-framework — Update Dependencies" -ForegroundColor Cyan
Write-Host "  =========================================" -ForegroundColor Cyan
Write-Host ""

$wingetIds = @(
    'Microsoft.PowerShell'
    'ShiningLight.OpenSSL.Dev'
    'Python.Python.3.12'
    'Schniz.fnm'
    'NSIS.NSIS'
)

Write-Host "Capturing current versions..." -ForegroundColor DarkGray
$beforeWinget = @{}
foreach ($id in $wingetIds) { $beforeWinget[$id] = Get-WingetVersion $id }
$beforeLlama  = Get-GitCommit $cfg.LlamaCppDir
$beforeWebui  = Get-GitCommit $cfg.OpenWebUIDir

# ── Run winget upgrades ──────────────────────────────────────────────

Write-Host ""
Write-Host "Updating winget packages..." -ForegroundColor Cyan
foreach ($id in $wingetIds) {
    if (-not $beforeWinget[$id]) {
        Write-Host "  $id: not installed (skipping)" -ForegroundColor DarkGray
        continue
    }
    Write-Host "  $id..." -ForegroundColor DarkGray
    winget upgrade --id $id --exact --silent `
        --accept-source-agreements --accept-package-agreements 2>&1 |
        Where-Object { $_ -notmatch '^\s*[\\|/-]\s*$' -and $_ -notmatch '^\s*$' } |
        ForEach-Object { Write-Host "    $_" -ForegroundColor DarkGray }
}

# ── Update llama.cpp ─────────────────────────────────────────────────

Write-Host ""
Write-Host "Updating llama.cpp..." -ForegroundColor Cyan
if ($beforeLlama) {
    git -C $cfg.LlamaCppDir pull --ff-only
    if ($LASTEXITCODE -ne 0) {
        Write-Host "  git pull failed in $($cfg.LlamaCppDir)" -ForegroundColor Yellow
    }
} else {
    Write-Host "  Not cloned at $($cfg.LlamaCppDir) — run 02-build.ps1 to clone." -ForegroundColor Yellow
}

# ── Update Open WebUI ────────────────────────────────────────────────

Write-Host ""
Write-Host "Updating Open WebUI..." -ForegroundColor Cyan
if ($beforeWebui) {
    git -C $cfg.OpenWebUIDir pull --ff-only
    if ($LASTEXITCODE -ne 0) {
        Write-Host "  git pull failed in $($cfg.OpenWebUIDir)" -ForegroundColor Yellow
    }
} else {
    Write-Host "  Not cloned at $($cfg.OpenWebUIDir) — run 02-build-webui.ps1 to clone." -ForegroundColor Yellow
}

# ── Post-update state ────────────────────────────────────────────────

$afterWinget = @{}
foreach ($id in $wingetIds) { $afterWinget[$id] = Get-WingetVersion $id }
$afterLlama = Get-GitCommit $cfg.LlamaCppDir
$afterWebui = Get-GitCommit $cfg.OpenWebUIDir

# ── Report ───────────────────────────────────────────────────────────

Write-Host ""
Write-Host "  Update Report" -ForegroundColor Cyan
Write-Host "  =============" -ForegroundColor Cyan
Write-Host ""

function Write-ReportRow {
    param([string]$Marker, [ConsoleColor]$Color, [string]$Name, [string]$Detail)
    Write-Host ("  {0} {1,-32} {2}" -f $Marker, $Name, $Detail) -ForegroundColor $Color
}

foreach ($id in $wingetIds) {
    $b = $beforeWinget[$id]
    $a = $afterWinget[$id]
    if (-not $b -and -not $a) {
        Write-ReportRow "[--]" DarkGray $id "(not installed)"
    } elseif ($b -ne $a) {
        Write-ReportRow "[++]" Green    $id "$b -> $a"
    } else {
        Write-ReportRow "[OK]" DarkGray $id $b
    }
}

$rebuildLlama = $false
$rebuildWebui = $false

if (-not $beforeLlama) {
    Write-ReportRow "[--]" DarkGray "llama.cpp" "(not cloned)"
} elseif ($beforeLlama -ne $afterLlama) {
    Write-ReportRow "[++]" Green    "llama.cpp" "$beforeLlama -> $afterLlama"
    $rebuildLlama = $true
} else {
    Write-ReportRow "[OK]" DarkGray "llama.cpp" $beforeLlama
}

if (-not $beforeWebui) {
    Write-ReportRow "[--]" DarkGray "Open WebUI" "(not cloned)"
} elseif ($beforeWebui -ne $afterWebui) {
    Write-ReportRow "[++]" Green    "Open WebUI" "$beforeWebui -> $afterWebui"
    $rebuildWebui = $true
} else {
    Write-ReportRow "[OK]" DarkGray "Open WebUI" $beforeWebui
}

# ── Manual SDKs reminder ─────────────────────────────────────────────

Write-Host ""
Write-Host "  Manual SDKs (not auto-updated):" -ForegroundColor DarkGray
Write-Host "    CUDA Toolkit  — https://developer.nvidia.com/cuda-downloads" -ForegroundColor DarkGray
Write-Host "    Vulkan SDK    — https://vulkan.lunarg.com/sdk/home" -ForegroundColor DarkGray
Write-Host "    AMD HIP / ROCm — https://www.amd.com/en/developer/resources/rocm-hub/hip-sdk.html" -ForegroundColor DarkGray

# ── Recommendations ──────────────────────────────────────────────────

Write-Host ""
if ($rebuildLlama -or $rebuildWebui) {
    Write-Host "  Recommended actions:" -ForegroundColor Yellow
    if ($rebuildLlama) {
        Write-Host "    .\02-build.ps1         # llama.cpp source updated" -ForegroundColor Yellow
    }
    if ($rebuildWebui) {
        Write-Host "    .\02-build-webui.ps1   # Open WebUI source updated" -ForegroundColor Yellow
    }
    Write-Host "    .\03-package.ps1       # rebuild installer afterwards" -ForegroundColor Yellow
} else {
    Write-Host "  No rebuild needed — llama.cpp and Open WebUI sources unchanged." -ForegroundColor Green
}
Write-Host ""
