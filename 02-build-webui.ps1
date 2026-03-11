# Build Open WebUI from source (GitHub) in a self-contained Python venv
# Requires Python 3.11 or 3.12 and Node.js 18-22 (installs via fnm if needed)

$ErrorActionPreference = 'Stop'

$cfg = Import-PowerShellDataFile "$PSScriptRoot\config.psd1"

$repoDir = if ($cfg.OpenWebUIDir) { $cfg.OpenWebUIDir } else { Join-Path $PSScriptRoot "open-webui" }
$venvDir = Join-Path $PSScriptRoot "webui-venv"

# ── Find compatible Python (3.11 or 3.12) ──────────────────────────
$pythonExe = $null

$candidates = @(
    (Get-Command python3.12 -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source)
    (Get-Command python3.11 -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source)
    "${env:ProgramFiles}\Python312\python.exe"
    "${env:ProgramFiles}\Python311\python.exe"
    "${env:LocalAppData}\Programs\Python\Python312\python.exe"
    "${env:LocalAppData}\Programs\Python\Python311\python.exe"
)

foreach ($c in $candidates) {
    if ($c -and (Test-Path $c)) {
        $ver = (& $c --version 2>&1) -replace 'Python\s+', ''
        $major, $minor = $ver.Split('.')[0..1] | ForEach-Object { [int]$_ }
        if ($major -eq 3 -and $minor -ge 11 -and $minor -le 12) {
            $pythonExe = $c
            break
        }
    }
}

# Fallback: check default python
if (-not $pythonExe) {
    $defaultPython = Get-Command python -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source
    if ($defaultPython) {
        $ver = (& $defaultPython --version 2>&1) -replace 'Python\s+', ''
        $major, $minor = $ver.Split('.')[0..1] | ForEach-Object { [int]$_ }
        if ($major -eq 3 -and $minor -ge 11 -and $minor -le 12) {
            $pythonExe = $defaultPython
        }
    }
}

if (-not $pythonExe) {
    Write-Host "No compatible Python found. Open WebUI requires Python 3.11 or 3.12." -ForegroundColor Red
    Write-Host "Installing Python 3.12 via winget..." -ForegroundColor Yellow
    winget install --id Python.Python.3.12 --accept-source-agreements --accept-package-agreements
    if ($LASTEXITCODE -ne 0) { throw "Failed to install Python 3.12" }
    $pythonExe = "${env:LocalAppData}\Programs\Python\Python312\python.exe"
    if (-not (Test-Path $pythonExe)) {
        $pythonExe = "${env:ProgramFiles}\Python312\python.exe"
    }
    if (-not (Test-Path $pythonExe)) {
        throw "Python 3.12 installed but not found. Restart your shell and try again."
    }
}

$pyVersion = (& $pythonExe --version 2>&1) -replace 'Python\s+', ''
Write-Host "Python: $pythonExe ($pyVersion)" -ForegroundColor Cyan

# ── Ensure compatible Node.js (>=18, <=22) ────────────────────────
$nodeOk = $false
$nodeExe = Get-Command node -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source
if ($nodeExe) {
    $nodeVer = (& node --version 2>&1) -replace 'v', ''
    $nodeMajor = [int]($nodeVer.Split('.')[0])
    if ($nodeMajor -ge 18 -and $nodeMajor -le 22) {
        $nodeOk = $true
    } else {
        Write-Host "Node.js v$nodeVer found but Open WebUI requires v18-v22." -ForegroundColor Yellow
    }
}

if (-not $nodeOk) {
    # Use fnm (Fast Node Manager) to install and activate Node 22 LTS
    $fnmExe = Get-Command fnm -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source
    if (-not $fnmExe) {
        # Refresh PATH first — fnm may already be installed but not in this session's PATH
        $env:Path = [System.Environment]::GetEnvironmentVariable("Path", "Machine") + ";" + [System.Environment]::GetEnvironmentVariable("Path", "User")
        $fnmExe = Get-Command fnm -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source
    }
    if (-not $fnmExe) {
        Write-Host "Installing fnm (Fast Node Manager)..." -ForegroundColor Yellow
        winget install --id Schniz.fnm --accept-source-agreements --accept-package-agreements
        # Refresh PATH after install
        $env:Path = [System.Environment]::GetEnvironmentVariable("Path", "Machine") + ";" + [System.Environment]::GetEnvironmentVariable("Path", "User")
        $fnmExe = Get-Command fnm -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source
        if (-not $fnmExe) { throw "fnm installed but not found in PATH. Restart your shell and try again." }
    }

    Write-Host "Installing Node.js 22 LTS via fnm..." -ForegroundColor Yellow
    fnm install 22
    if ($LASTEXITCODE -ne 0) { throw "fnm install 22 failed" }

    # Activate fnm environment in current session
    fnm env --use-on-cd --shell power-shell | Out-String | Invoke-Expression
    fnm use 22
    if ($LASTEXITCODE -ne 0) { throw "fnm use 22 failed" }

    $nodeExe = Get-Command node -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source
    if (-not $nodeExe) { throw "Node.js 22 installed via fnm but not found in PATH." }
}

$nodeVersion = (& node --version 2>&1)
Write-Host "Node.js: $nodeExe ($nodeVersion)" -ForegroundColor Cyan

# ── Clone or update Open WebUI repository ─────────────────────────
if (-not (Test-Path "$repoDir\package.json")) {
    Write-Host "Open WebUI not found at $repoDir, cloning..." -ForegroundColor Yellow
    git clone https://github.com/open-webui/open-webui.git $repoDir
    if ($LASTEXITCODE -ne 0) { throw "git clone failed" }
} else {
    $prevCommit = (git -C $repoDir rev-parse HEAD 2>&1)
    Write-Host "Pulling latest Open WebUI..." -ForegroundColor Cyan
    git -C $repoDir pull
    if ($LASTEXITCODE -ne 0) { throw "git pull failed" }
    $newCommit = (git -C $repoDir rev-parse HEAD 2>&1)

    # If repo updated, wipe venv so it gets rebuilt cleanly
    if ($prevCommit -ne $newCommit -and (Test-Path $venvDir)) {
        Write-Host "New version detected — removing old venv..." -ForegroundColor Yellow
        Remove-Item $venvDir -Recurse -Force
    }
}

$gitHash = (git -C $repoDir rev-parse --short HEAD 2>&1)
Write-Host "Open WebUI commit: $gitHash" -ForegroundColor Cyan

# ── Build frontend ────────────────────────────────────────────────
$buildMarker = Join-Path $repoDir ".build-commit"
$currentCommit = (git -C $repoDir rev-parse HEAD 2>&1)
$lastBuildCommit = if (Test-Path $buildMarker) { (Get-Content $buildMarker -Raw).Trim() } else { "" }

if ($lastBuildCommit -eq $currentCommit.Trim()) {
    Write-Host "Frontend already built for $gitHash — skipping." -ForegroundColor Green
} else {
    Push-Location $repoDir
    if (-not (Test-Path ".env")) {
        Copy-Item ".env.example" ".env" -ErrorAction SilentlyContinue
    }

    Write-Host "Installing Node.js dependencies..." -ForegroundColor Cyan
    npm install --legacy-peer-deps
    if ($LASTEXITCODE -ne 0) { throw "npm install failed" }

    Write-Host "Building frontend..." -ForegroundColor Cyan
    npm run build
    if ($LASTEXITCODE -ne 0) { throw "npm run build failed" }

    Set-Content -Path $buildMarker -Value $currentCommit -Encoding UTF8
    Pop-Location
}

# ── Create or update Python venv ──────────────────────────────────
$needsRecreate = $false
if (-not (Test-Path "$venvDir\Scripts\python.exe")) {
    $needsRecreate = $true
} elseif (-not (& "$venvDir\Scripts\python.exe" -m pip --version 2>$null)) {
    $needsRecreate = $true
} else {
    $venvVer = (& "$venvDir\Scripts\python.exe" --version 2>&1) -replace 'Python\s+', ''
    $venvMinor = [int]($venvVer.Split('.')[1])
    if ($venvMinor -lt 11 -or $venvMinor -gt 12) {
        Write-Host "Existing venv has Python $venvVer (incompatible) — recreating..." -ForegroundColor Yellow
        $needsRecreate = $true
    }
}

if ($needsRecreate) {
    if (Test-Path $venvDir) { Remove-Item $venvDir -Recurse -Force }
    Write-Host "Creating virtual environment..." -ForegroundColor Cyan
    & $pythonExe -m venv --clear $venvDir
    if ($LASTEXITCODE -ne 0) { throw "Failed to create venv" }
    & "$venvDir\Scripts\python.exe" -m ensurepip --upgrade
    if ($LASTEXITCODE -ne 0) { throw "Failed to install pip in venv" }
} else {
    Write-Host "Using existing venv: $venvDir" -ForegroundColor Cyan
}

$venvPython = Join-Path $venvDir "Scripts\python.exe"

# ── Install Open WebUI from source ────────────────────────────────
Write-Host "Installing open-webui from source..." -ForegroundColor Cyan
& $venvPython -m pip install --upgrade pip 2>&1 | Select-Object -Last 1

# Install dependencies with relaxed pins (== → >=) to handle yanked versions on PyPI
$reqFile = Join-Path $repoDir "backend\requirements.txt"
if (Test-Path $reqFile) {
    Write-Host "Installing backend dependencies..." -ForegroundColor Cyan
    $relaxedReq = Join-Path $PSScriptRoot "webui-requirements-relaxed.txt"
    (Get-Content $reqFile) -replace '==', '>=' | Set-Content $relaxedReq -Encoding UTF8
    & $venvPython -m pip install -r $relaxedReq
    Remove-Item $relaxedReq -Force -ErrorAction SilentlyContinue
    if ($LASTEXITCODE -ne 0) { Write-Host "Some dependencies failed — continuing..." -ForegroundColor Yellow }
}

# Install package without re-resolving deps
& $venvPython -m pip install --no-deps $repoDir
if ($LASTEXITCODE -ne 0) { throw "Failed to install open-webui from source" }

# ── Verify installation ──────────────────────────────────────────
$webuiVersion = & $venvPython -c "import importlib.metadata; print(importlib.metadata.version('open-webui'))" 2>&1
Write-Host ""
Write-Host "Open WebUI $webuiVersion (git $gitHash) installed in: $venvDir" -ForegroundColor Green
Write-Host ""
