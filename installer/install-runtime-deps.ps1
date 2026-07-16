# Installs the runtime dependencies llama.cpp-framework needs but does not
# bundle. Ships inside the installer (staged into bin\ by 03-package.ps1,
# offered as a checkbox on the finish page) and can be re-run any time from
# <InstallDir>\bin. Runs on end-user machines: Windows PowerShell 5.1, no dev
# tools assumed. Idempotent - detects what is present and only offers what is
# missing for the GPUs actually in the machine.
#
# Components (pins in dist-pins.psd1, staged next to this script):
#   - VC++ Redistributable x64  - required by every shipped binary (~19 MB)
#   - ROCm/TheRock (AMD GPUs)   - HIP backend user-space: hipblas/rocblas +
#     kernels (~4.3 GB download, ~25 GB on disk). Requires the Adrenalin
#     driver (not installable from here). Without it AMD GPUs run on Vulkan.
#   - cuBLAS runtime (NVIDIA)   - CUDA backend math libs, official NVIDIA
#     per-component redist (~375 MB); the two DLLs land next to
#     llama-server.exe. Requires the NVIDIA driver. Without it NVIDIA GPUs
#     run on Vulkan.
#
# Drivers are prerequisites we can only point at: backends whose dependencies
# are missing are skipped silently by llama-server at runtime (the GPU then
# appears as Vulkan-only) - that is the symptom this script exists to fix.
#
# Component selection: the -VcRedist/-Amd/-Nvidia switches pick EXACTLY those
# components (the installer's component checkboxes call this script that way,
# one switch per section, combined with -Unattended so nothing prompts). With
# no component switch: VC++ is always considered, and the GPU legs are an
# explicit interactive choice ([A]MD / [N]VIDIA / [B]oth / [S]kip, detected
# GPUs pre-fill the default) - or follow detection under -Unattended/-Report.

[CmdletBinding()]
param(
    [switch]$Unattended,   # no prompts: install everything selected/missing
    [switch]$Report,       # detection only, change nothing (no elevation needed)
    [switch]$VcRedist,     # component switch: VC++ redistributable only
    [switch]$Amd,          # component switch: AMD leg (ROCm/TheRock)
    [switch]$Nvidia        # component switch: NVIDIA leg (cuBLAS)
)

$ErrorActionPreference = 'Stop'

function Test-IsAdmin {
    ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
    ).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

# Self-elevate (except in report mode, which only reads).
if (-not $Report -and -not (Test-IsAdmin)) {
    Write-Host "Requesting administrator privileges..." -ForegroundColor Yellow
    $argList = "-NoProfile -ExecutionPolicy Bypass -File `"$PSCommandPath`""
    if ($Unattended) { $argList += ' -Unattended' }
    if ($VcRedist)   { $argList += ' -VcRedist' }
    if ($Amd)        { $argList += ' -Amd' }
    if ($Nvidia)     { $argList += ' -Nvidia' }
    Start-Process powershell -Verb RunAs -ArgumentList $argList
    exit
}

$pins = Import-PowerShellDataFile (Join-Path $PSScriptRoot 'dist-pins.psd1')

function Ask([string]$Question) {
    if ($Unattended) { return $true }
    $a = Read-Host "$Question [Y/n]"
    return ($a -eq '' -or $a -match '^[yY]')
}

# curl.exe explicitly - PS 5.1 aliases `curl` to Invoke-WebRequest.
function Get-RemoteFileSize([string]$Url) {
    $prevEap = $ErrorActionPreference; $ErrorActionPreference = 'Continue'
    $head = curl.exe -sIL --fail --max-time 30 $Url 2>$null | Out-String
    $ErrorActionPreference = $prevEap
    if ($LASTEXITCODE -ne 0) { return $null }
    if ($head -match '(?im)^Content-Length:\s*(\d+)') { return [long]$Matches[1] }
    return $null
}

Write-Host ""
Write-Host "  llama.cpp-framework - runtime dependencies" -ForegroundColor Cyan
Write-Host "  ===========================================" -ForegroundColor Cyan
Write-Host ""

$gpuNames  = @(Get-CimInstance Win32_VideoController | ForEach-Object { $_.Name })
$hasAmd    = [bool]($gpuNames | Where-Object { $_ -match 'AMD|Radeon' })
$hasNvidia = [bool]($gpuNames | Where-Object { $_ -match 'NVIDIA' })
foreach ($g in $gpuNames) { Write-Host "  GPU: $g" -ForegroundColor DarkGray }
Write-Host ""

# Which component(s)? Explicit switches pick exactly those; otherwise VC++ is
# always considered and the GPU legs are an explicit user choice - the
# detected GPUs only set the default. -Unattended/-Report stay detection-based.
$explicit = [bool]($VcRedist -or $Amd -or $Nvidia)
$doVc = (-not $explicit) -or [bool]$VcRedist
$doAmd = $false; $doNvidia = $false
if ($explicit) {
    $doAmd = [bool]$Amd; $doNvidia = [bool]$Nvidia
} elseif ($Unattended -or $Report) {
    $doAmd = $hasAmd; $doNvidia = $hasNvidia
} else {
    $def = if ($hasAmd -and $hasNvidia) { 'B' } elseif ($hasAmd) { 'A' } elseif ($hasNvidia) { 'N' } else { 'S' }
    $ans = Read-Host "  Install GPU components for: [A]MD, [N]VIDIA, [B]oth, [S]kip (detected: $def)"
    if ($ans -eq '') { $ans = $def }
    switch -Regex ($ans) {
        '^[aA]' { $doAmd = $true }
        '^[nN]' { $doNvidia = $true }
        '^[bB]' { $doAmd = $true; $doNvidia = $true }
        default { }
    }
    Write-Host ""
}

$actions = @()

# -- 1) VC++ Redistributable x64 (required by every binary) ----------
if ($doVc) {
    $vcKey = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64' -ErrorAction SilentlyContinue
    if ($vcKey -and $vcKey.Installed -eq 1) {
        Write-Host "  [OK] VC++ Redistributable x64 ($($vcKey.Version))" -ForegroundColor Green
    } elseif ($Report) {
        Write-Host "  [--] VC++ Redistributable x64 MISSING (required by all binaries)" -ForegroundColor Yellow
    } elseif (Ask "  VC++ Redistributable x64 is MISSING (required, ~19 MB). Install?") {
        $exe = Join-Path $env:TEMP 'vc_redist.x64.exe'
        Write-Host "  downloading vc_redist.x64.exe..." -ForegroundColor DarkGray
        curl.exe --fail -L --retry 3 -o $exe $pins.VcRedist.Url
        if ($LASTEXITCODE -ne 0) { Write-Host "  download failed (curl exit $LASTEXITCODE)" -ForegroundColor Red }
        else {
            # 0 = ok, 3010 = ok + reboot required, 1638 = newer version already present
            $p = Start-Process $exe -ArgumentList '/install /quiet /norestart' -Wait -PassThru
            if ($p.ExitCode -in 0, 3010, 1638) {
                $actions += "VC++ redist installed$(if ($p.ExitCode -eq 3010) { ' (reboot required)' })"
                Write-Host "  [OK] VC++ redist installed" -ForegroundColor Green
            } else { Write-Host "  [!!] vc_redist exit code $($p.ExitCode)" -ForegroundColor Red }
            Remove-Item $exe -Force -ErrorAction SilentlyContinue
        }
    }
}

# -- 2) ROCm/TheRock (AMD GPUs - HIP backend) ------------------------
if ($doAmd) {
    $rocm = $pins.Rocm
    if (-not (Test-Path "$env:windir\System32\amdhip64_7.dll")) {
        Write-Host "  [!!] AMD GPU present but no Adrenalin driver (amdhip64_7.dll) - install it first:" -ForegroundColor Yellow
        Write-Host "       https://www.amd.com/en/support" -ForegroundColor Yellow
    } else {
        $hp = [Environment]::GetEnvironmentVariable('HIP_PATH', 'Machine')
        if ($hp -and (Test-Path "$hp\bin\hipblas.dll")) {
            Write-Host "  [OK] ROCm/TheRock at $hp" -ForegroundColor Green
        } elseif ($Report) {
            Write-Host "  [--] ROCm/TheRock MISSING - AMD GPUs will run on Vulkan, not HIP" -ForegroundColor Yellow
        } else {
            $dist = $null
            foreach ($d in $rocm.Dists) {
                $size = Get-RemoteFileSize $d.Url
                if ($size) { $dist = @{ Version = $d.Version; Url = $d.Url; Size = $size }; break }
            }
            if (-not $dist) {
                Write-Host "  [!!] ROCm dist not reachable (offline?)" -ForegroundColor Red
            } elseif (Ask "  ROCm/TheRock $($dist.Version) is MISSING (HIP backend for AMD GPUs; $([math]::Round($dist.Size/1GB,1)) GB download, ~25 GB on disk). Install to $($rocm.InstallDir)?") {
                $tar = Join-Path $env:TEMP (Split-Path $dist.Url -Leaf)
                $haveTar = (Test-Path $tar) -and ((Get-Item $tar).Length -eq $dist.Size)
                if (-not $haveTar) {
                    if ((Test-Path $tar) -and ((Get-Item $tar).Length -gt $dist.Size)) { Remove-Item $tar -Force }
                    curl.exe --fail -L -C - --retry 3 --retry-delay 5 -o $tar $dist.Url
                    $haveTar = ($LASTEXITCODE -eq 0) -and ((Get-Item $tar -ErrorAction SilentlyContinue).Length -eq $dist.Size)
                    if (-not $haveTar) { Write-Host "  download failed/incomplete - re-run to resume" -ForegroundColor Red }
                }
                if ($haveTar) {
                    if (Test-Path $rocm.InstallDir) { Remove-Item $rocm.InstallDir -Recurse -Force -ErrorAction SilentlyContinue }
                    if (Test-Path $rocm.InstallDir) {
                        Write-Host "  cannot clear $($rocm.InstallDir) (files in use?)" -ForegroundColor Red
                    } else {
                        New-Item -ItemType Directory -Force -Path $rocm.InstallDir | Out-Null
                        Write-Host "  extracting (takes a while)..." -ForegroundColor DarkGray
                        tar.exe -xzf $tar -C $rocm.InstallDir --strip-components=1
                        if ($LASTEXITCODE -eq 0) {
                            Set-Content -Path (Join-Path $rocm.InstallDir $rocm.Marker) -Value $dist.Version
                            Remove-Item $tar -Force
                            # Runtime env only: HIP_PATH (llama-cpp-config finds the DLLs
                            # through it) + PATH so bare llama-server runs too. NEVER set
                            # the compile-time vars (LLVM_PATH, HIP_DEVICE_LIB_PATH,
                            # HIP_PLATFORM) in a persistent scope: the Adrenalin driver's
                            # own HIP runtime reads LLVM_PATH at runtime and a TheRock
                            # LLVM there breaks it (hipMemGetInfo "invalid argument",
                            # 0xC0000005 during model load). Build machines get them
                            # per-process from common.ps1.
                            [Environment]::SetEnvironmentVariable('HIP_PATH', $rocm.InstallDir, 'Machine')
                            $key = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey('SYSTEM\CurrentControlSet\Control\Session Manager\Environment', $true)
                            $path = [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
                            $parts = @($path -split ';' | Where-Object { $_ })
                            $add = Join-Path $rocm.InstallDir 'bin'
                            if ($parts -notcontains $add) {
                                $key.SetValue('Path', (($parts + $add) -join ';'), [Microsoft.Win32.RegistryValueKind]::ExpandString)
                            }
                            $key.Close()
                            $actions += "ROCm/TheRock $($dist.Version) installed (HIP_PATH set)"
                            Write-Host "  [OK] ROCm/TheRock $($dist.Version) installed" -ForegroundColor Green
                        } else { Write-Host "  [!!] extraction failed (tar exit $LASTEXITCODE) - tarball kept for retry" -ForegroundColor Red }
                    }
                }
            }
        }
    }
}

# -- 3) cuBLAS runtime (NVIDIA GPUs - CUDA backend) ------------------
if ($doNvidia) {
    if (-not (Test-Path "$env:windir\System32\nvcuda.dll")) {
        Write-Host "  [!!] NVIDIA GPU present but no NVIDIA driver (nvcuda.dll) - install it first:" -ForegroundColor Yellow
        Write-Host "       https://www.nvidia.com/drivers" -ForegroundColor Yellow
    } else {
        # Resolvable if next to llama-server.exe (this script's dir), in
        # System32, or anywhere on the machine PATH (e.g. a CUDA Toolkit).
        $found = Test-Path (Join-Path $PSScriptRoot 'cublas64_13.dll')
        if (-not $found) {
            $dirs = @("$env:windir\System32") + (([Environment]::GetEnvironmentVariable('Path', 'Machine') -split ';') | Where-Object { $_ })
            foreach ($d in $dirs) { if (Test-Path (Join-Path $d.Trim() 'cublas64_13.dll')) { $found = $true; break } }
        }
        if ($found) {
            Write-Host "  [OK] cuBLAS runtime (cublas64_13.dll) found" -ForegroundColor Green
        } elseif ($Report) {
            Write-Host "  [--] cuBLAS runtime MISSING - NVIDIA GPUs will run on Vulkan, not CUDA" -ForegroundColor Yellow
        } elseif (Ask "  cuBLAS runtime is MISSING (CUDA backend for NVIDIA GPUs, ~375 MB). Install next to llama-server.exe?") {
            $zip = Join-Path $env:TEMP (Split-Path $pins.CudaBlas.Url -Leaf)
            curl.exe --fail -L -C - --retry 3 -o $zip $pins.CudaBlas.Url
            if ($LASTEXITCODE -ne 0) { Write-Host "  download failed (curl exit $LASTEXITCODE)" -ForegroundColor Red }
            elseif ((Get-FileHash $zip -Algorithm SHA256).Hash -ne $pins.CudaBlas.Sha256) {
                Write-Host "  [!!] SHA256 mismatch - corrupt download, removing" -ForegroundColor Red
                Remove-Item $zip -Force
            } else {
                $tmp = Join-Path $env:TEMP 'libcublas-extract'
                if (Test-Path $tmp) { Remove-Item $tmp -Recurse -Force }
                Expand-Archive $zip -DestinationPath $tmp
                $staged = 0
                foreach ($name in 'cublas64_13.dll', 'cublasLt64_13.dll') {
                    $src = Get-ChildItem $tmp -Recurse -Filter $name | Select-Object -First 1
                    if ($src) { Copy-Item $src.FullName -Destination $PSScriptRoot -Force; $staged++ }
                    else { Write-Host "  [!!] $name not found in the redist archive" -ForegroundColor Red }
                }
                Remove-Item $tmp -Recurse -Force
                Remove-Item $zip -Force
                if ($staged -eq 2) {
                    $actions += "cuBLAS runtime installed next to llama-server.exe"
                    Write-Host "  [OK] cuBLAS runtime installed" -ForegroundColor Green
                }
            }
        }
    }
}

# -- Summary ---------------------------------------------------------
Write-Host ""
if ($actions.Count) {
    Write-Host "  Done:" -ForegroundColor Cyan
    foreach ($a in $actions) { Write-Host "    - $a" -ForegroundColor Green }
    Write-Host "  Open a NEW terminal (or restart llama-cpp-config) to pick up environment changes." -ForegroundColor DarkGray
} elseif (-not $Report) {
    Write-Host "  Nothing to do." -ForegroundColor Green
}
Write-Host ""
if (-not $Unattended -and -not $Report) { Read-Host "Press Enter to close" | Out-Null }
