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
#     The leg also stages the dist's own HIP runtime (amdhip64_*.dll, ~17 MB)
#     plus its code-object manager (amd_comgr*.dll, ~122 MB) next to
#     llama-server.exe, which is what makes BF16/F16 models run on gfx1201
#     and what keeps HIP enumeration alive once an iGPU is visible - see
#     Invoke-HipRuntimeStaging for the why. That copy is
#     reachable on its own as -StageHipRuntime (leg 2b), which downloads
#     nothing and is what the installer's hidden always-run section calls, so
#     a machine that already has ROCm gets it without ticking a 4.3 GB box.
#   - cuBLAS runtime (NVIDIA)   - CUDA backend math libs, official NVIDIA
#     per-component redist (~375 MB); the two DLLs land next to
#     llama-server.exe. Requires the NVIDIA driver. Without it NVIDIA GPUs
#     run on Vulkan.
#
# Drivers are prerequisites we can only point at: backends whose dependencies
# are missing are skipped silently by llama-server at runtime (the GPU then
# appears as Vulkan-only) - that is the symptom this script exists to fix.
#
# Component selection: the -VcRedist/-Amd/-Nvidia/-StageHipRuntime switches
# pick EXACTLY those components (the installer's component checkboxes call this
# script that way, one switch per section, combined with -Unattended so nothing
# prompts; -StageHipRuntime is the hidden always-run section, see leg 2b). With
# no component switch: VC++ is always considered, and the GPU legs are an
# explicit interactive choice ([A]MD / [N]VIDIA / [B]oth / [S]kip, detected
# GPUs pre-fill the default) - or follow detection under -Unattended/-Report.

[CmdletBinding()]
param(
    [switch]$Unattended,   # no prompts: install everything selected/missing
    [switch]$Report,       # detection only, change nothing (no elevation needed)
    [switch]$VcRedist,     # component switch: VC++ redistributable only
    [switch]$Amd,          # component switch: AMD leg (ROCm/TheRock)
    [switch]$Nvidia,       # component switch: NVIDIA leg (cuBLAS)
    [switch]$StageHipRuntime  # component switch: ONLY stage the HIP runtime
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
    if ($StageHipRuntime) { $argList += ' -StageHipRuntime' }
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

# The dist's own HIP runtime, staged next to llama-server.exe. This is what
# makes BF16/F16 models run on gfx1201, and it is a load-order fix, not an
# extra dependency: Windows resolves a static import from the EXE's own
# directory and then System32, BOTH before PATH, and the AMD driver keeps its
# own amdhip64_7.dll in System32. So without this copy every binary here runs
# the dist's rocblas.dll + libhipblaslt.dll against the DRIVER's HIP runtime,
# never the dist's, and that mix is the one that aborts at the first prefill
# batch with "ROCm error: invalid argument" (the second 16-bit GEMM in a
# process fails at kernel launch; ROCm#6461, TheRock#7271). Measured
# 2026-09-01 as a same-directory A/B with this file as the only variable:
# present -> passes, removed -> fails, identically on 7.14.0, 10.0.0,
# 7.15.0a20260728 and 10.1.0a20260901, with ROCBLAS_USE_HIPBLASLT forced to 1
# as well - so hipBLASLt itself is healthy and the dist version was never the
# variable. rocblas and hipblaslt keep resolving from HIP_PATH\bin over PATH
# (System32 has no copy to shadow them with), but amd_comgr does NOT: the
# runtime asks for it by bare name and System32 keeps the DRIVER's own
# amd_comgr.dll, which precedes PATH - a different build the dist runtime was
# never tested against. The mismatch went unnoticed while only a dGPU was
# visible, but once an iGPU (gfx1036) enumerates ahead of it,
# hipGetDeviceCount itself dies with "no ROCm-capable device is detected" and
# the whole HIP backend disappears (root-caused 2026-09-03 by A/B/A: the
# dist's comgr beside the staged runtime, both devices enumerate; without it,
# none; amdocl64.dll is NOT needed). So amd_comgr*.dll is staged beside the
# runtime too, and deleting both copies reverts to the driver's runtime.
#
# Freshness is decided by CONTENT, and it has to be: every dist stamps the
# same 10.0.3581.0 into the DLL's VERSIONINFO, and 10.0.0 and
# 7.15.0a20260728 are even the same 16,555,520 bytes while being different
# binaries. So a (size, FileVersion) check would call a stale copy current the
# moment HIP_PATH moves between two such dists, and leave the install running
# another version's runtime. One SHA256 each of a ~17 MB and a ~122 MB file
# per run is the cheap way to be right.
function Invoke-HipRuntimeStaging([string]$HipPath) {
    # The runtime AND its code-object manager, both content-checked: see the
    # block above for why amd_comgr*.dll must be the dist's own.
    $srcs = @(Get-ChildItem (Join-Path $HipPath 'bin') -Filter 'amdhip64_*.dll' -ErrorAction SilentlyContinue) +
            @(Get-ChildItem (Join-Path $HipPath 'bin') -Filter 'amd_comgr*.dll' -ErrorAction SilentlyContinue)
    foreach ($src in $srcs) {
        $dst = Join-Path $PSScriptRoot $src.Name
        $cur = Get-Item $dst -ErrorAction SilentlyContinue
        $ver = $src.VersionInfo.FileVersion
        $same = $cur -and $cur.Length -eq $src.Length -and
                (Get-FileHash $dst -Algorithm SHA256).Hash -eq (Get-FileHash $src.FullName -Algorithm SHA256).Hash
        $why = if ($src.Name -like 'amdhip64_*') { '- BF16/F16 models abort on gfx1201' }
               else { '- HIP enumerates no device once an iGPU is enabled' }
        if ($same) {
            Write-Host "  [OK] HIP runtime $($src.Name) $ver next to llama-server.exe" -ForegroundColor Green
        } elseif ($Report) {
            $state = if ($cur) { 'is a copy of another dist' } else { 'not staged' }
            Write-Host "  [--] HIP runtime $($src.Name) $state $why" -ForegroundColor Yellow
        } else {
            try {
                Copy-Item $src.FullName -Destination $dst -Force
                $script:actions += "HIP runtime $($src.Name) $ver staged next to llama-server.exe"
                Write-Host "  [OK] HIP runtime $($src.Name) $ver staged next to llama-server.exe" -ForegroundColor Green
            } catch {
                Write-Host "  [!!] cannot write $dst - close llama-server and re-run" -ForegroundColor Red
            }
        }
    }
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
$explicit = [bool]($VcRedist -or $Amd -or $Nvidia -or $StageHipRuntime)
$doVc = (-not $explicit) -or [bool]$VcRedist
$doAmd = $false; $doNvidia = $false
# -StageHipRuntime is a component switch like the others, and it is the only
# one that also happens INSIDE another leg: the AMD leg stages the runtime as
# its last step, so with both switches the stage-only leg stands down rather
# than hashing the same file twice.
$doStageOnly = [bool]$StageHipRuntime -and -not [bool]$Amd
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
        # TWO landmarks, not one. hipblas.dll alone proves nothing: it is the
        # entry point ggml-hip imports, but rocBLAS then reads its Tensile
        # kernels from bin\rocblas\library, and without that directory the
        # backend loads, the device shows up, and the FIRST matrix multiply dies
        # with "Could not initialize Tensile host". An interrupted extraction
        # lands exactly there, and the old one-file check waved it through.
        # Both landmarks are version-stable (present in 7.14.0 and 10.0.0
        # alike), unlike the rest of the file list: origami.dll for instance
        # exists only from 10.0, so a fuller check would report a healthy 7.14
        # install as broken.
        $rocmOk = $hp -and (Test-Path "$hp\bin\hipblas.dll") -and (Test-Path "$hp\bin\rocblas\library")
        if ($rocmOk) {
            Write-Host "  [OK] ROCm/TheRock at $hp" -ForegroundColor Green
        } elseif ($Report) {
            Write-Host "  [--] ROCm/TheRock MISSING - AMD GPUs will run on Vulkan, not HIP" -ForegroundColor Yellow
        } else {
            if ($hp -and (Test-Path "$hp\bin\hipblas.dll")) {
                Write-Host "  [!!] ROCm/TheRock at $hp is INCOMPLETE (bin\rocblas\library absent) - reinstalling" -ForegroundColor Yellow
            }
            $dist = $null
            foreach ($d in $rocm.Dists) {
                $size = Get-RemoteFileSize $d.Url
                if ($size) { $dist = @{ Version = $d.Version; Url = $d.Url; Size = $size }; break }
            }
            # One directory per version under Root, the same layout the build
            # machine uses: HIP_PATH names the active one, so a later version
            # installs beside this one instead of over it.
            $distDir = if ($dist) { Join-Path $rocm.Root $dist.Version } else { $null }
            if (-not $dist) {
                Write-Host "  [!!] ROCm dist not reachable (offline?)" -ForegroundColor Red
            } elseif (Ask "  ROCm/TheRock $($dist.Version) is MISSING (HIP backend for AMD GPUs; $([math]::Round($dist.Size/1GB,1)) GB download, ~25 GB on disk). Install to $distDir?") {
                $tar = Join-Path $env:TEMP (Split-Path $dist.Url -Leaf)
                $haveTar = (Test-Path $tar) -and ((Get-Item $tar).Length -eq $dist.Size)
                if (-not $haveTar) {
                    if ((Test-Path $tar) -and ((Get-Item $tar).Length -gt $dist.Size)) { Remove-Item $tar -Force }
                    curl.exe --fail -L -C - --retry 3 --retry-delay 5 -o $tar $dist.Url
                    $haveTar = ($LASTEXITCODE -eq 0) -and ((Get-Item $tar -ErrorAction SilentlyContinue).Length -eq $dist.Size)
                    if (-not $haveTar) { Write-Host "  download failed/incomplete - re-run to resume" -ForegroundColor Red }
                }
                if ($haveTar) {
                    # Only ever an incomplete tree of THIS version: another
                    # version owns its own directory and is left alone.
                    if (Test-Path $distDir) { Remove-Item $distDir -Recurse -Force -ErrorAction SilentlyContinue }
                    if (Test-Path $distDir) {
                        Write-Host "  cannot clear $distDir (files in use?)" -ForegroundColor Red
                    } else {
                        New-Item -ItemType Directory -Force -Path $distDir | Out-Null
                        Write-Host "  extracting (takes a while)..." -ForegroundColor DarkGray
                        tar.exe -xzf $tar -C $distDir --strip-components=1
                        if ($LASTEXITCODE -eq 0) {
                            Set-Content -Path (Join-Path $distDir $rocm.Marker) -Value $dist.Version
                            Remove-Item $tar -Force
                            # Runtime env only: HIP_PATH (llama-cpp-config finds the DLLs
                            # through it) + PATH so bare llama-server runs too. NEVER set
                            # the compile-time vars (LLVM_PATH, HIP_DEVICE_LIB_PATH,
                            # HIP_PLATFORM) in a persistent scope: the Adrenalin driver's
                            # own HIP runtime reads LLVM_PATH at runtime and a TheRock
                            # LLVM there breaks it (hipMemGetInfo "invalid argument",
                            # 0xC0000005 during model load). Build machines get them
                            # per-process from common.ps1.
                            [Environment]::SetEnvironmentVariable('HIP_PATH', $distDir, 'Machine')
                            $key = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey('SYSTEM\CurrentControlSet\Control\Session Manager\Environment', $true)
                            $path = [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
                            # Any other version dir left on PATH goes: two
                            # amdhip64_*.dll directories are an ambiguous load
                            # order, the classic silent-crash cause.
                            $add = Join-Path $distDir 'bin'
                            $under = $rocm.Root.TrimEnd('\') + '\'
                            $parts = @()
                            foreach ($p in @($path -split ';' | Where-Object { $_ })) {
                                $t = $p.TrimEnd('\')
                                if ($t.StartsWith($under, [StringComparison]::OrdinalIgnoreCase) -and $t -ne $add) { continue }
                                $parts += $p
                            }
                            if ($parts -notcontains $add) { $parts += $add }
                            $key.SetValue('Path', ($parts -join ';'), [Microsoft.Win32.RegistryValueKind]::ExpandString)
                            $key.Close()
                            $actions += "ROCm/TheRock $($dist.Version) installed (HIP_PATH set)"
                            Write-Host "  [OK] ROCm/TheRock $($dist.Version) installed" -ForegroundColor Green
                            $hp = $distDir; $rocmOk = $true
                        } else { Write-Host "  [!!] extraction failed (tar exit $LASTEXITCODE) - tarball kept for retry" -ForegroundColor Red }
                    }
                }
            }
        }

        if ($rocmOk) { Invoke-HipRuntimeStaging $hp }
    }
}

# -- 2b) HIP runtime only (the installer's hidden always-run step) ---
# The same copy the AMD leg makes, reachable WITHOUT asking for the 4.3 GB
# component, and that separation is the point: a machine that already has ROCm
# needs the ~17 MB file and nothing else, while the checkbox that would have
# delivered it announces a 4.3 GB download, i.e. it is exactly the one such a
# user never ticks. So the installer runs this on every install and upgrade
# (hidden section, nothing to choose) and the component keeps its own job,
# installing ROCm when it is missing. Silent when there is nothing to do (no
# HIP_PATH, an incomplete dist): it also runs on NVIDIA-only machines.
if ($doStageOnly) {
    $hp = [Environment]::GetEnvironmentVariable('HIP_PATH', 'Machine')
    if ($hp -and (Test-Path "$hp\bin\hipblas.dll") -and (Test-Path "$hp\bin\rocblas\library")) {
        Invoke-HipRuntimeStaging $hp
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
