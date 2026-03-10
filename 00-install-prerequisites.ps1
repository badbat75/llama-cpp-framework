# Install prerequisites for building llama.cpp: OpenSSL, CUDA, Vulkan SDK, AMD HIP (ROCm)
# Skips anything already installed. Self-elevates once via UAC for all installs.

function Test-IsAdmin {
    ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
    ).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

Write-Host ""
Write-Host "  llama.cpp-framework — Install Prerequisites" -ForegroundColor Cyan
Write-Host "  ============================================" -ForegroundColor Cyan
Write-Host ""

# ── Check what's missing ─────────────────────────────────────────────

$toInstall = @()

# OpenSSL
$opensslDir = "${env:ProgramFiles}\OpenSSL-Win64"
if (Test-Path "$opensslDir\include\openssl\ssl.h") {
    Write-Host "  [OK] OpenSSL already installed" -ForegroundColor Green
} else {
    Write-Host "  [..] OpenSSL not found" -ForegroundColor Yellow
    $toInstall += "OpenSSL"
}

# CUDA (manual install)
if (Test-Path "${env:ProgramFiles}\NVIDIA GPU Computing Toolkit\CUDA\*\bin\nvcc.exe") {
    Write-Host "  [OK] CUDA Toolkit already installed" -ForegroundColor Green
} else {
    Write-Host "  [--] CUDA Toolkit not found (manual install)" -ForegroundColor Yellow
    Write-Host "       https://developer.nvidia.com/cuda-downloads" -ForegroundColor DarkGray
}

# Vulkan (manual install)
if (($env:VULKAN_SDK -and (Test-Path $env:VULKAN_SDK)) -or (Test-Path "${env:ProgramFiles}\VulkanSDK\*\Bin\glslc.exe")) {
    Write-Host "  [OK] Vulkan SDK already installed" -ForegroundColor Green
} else {
    Write-Host "  [--] Vulkan SDK not found (manual install)" -ForegroundColor Yellow
    Write-Host "       https://vulkan.lunarg.com/sdk/home" -ForegroundColor DarkGray
}

# AMD HIP / ROCm (manual install)
if (Test-Path "${env:ProgramFiles}\AMD\ROCm\*\bin\hipcc.exe") {
    Write-Host "  [OK] AMD HIP SDK already installed" -ForegroundColor Green
} else {
    Write-Host "  [--] AMD HIP SDK not found (manual install)" -ForegroundColor Yellow
    Write-Host "       https://www.amd.com/en/developer/resources/rocm-hub/hip-sdk.html" -ForegroundColor DarkGray
}

Write-Host ""

# ── Install OpenSSL via winget (elevated) ────────────────────────────

if ($toInstall -contains "OpenSSL") {
    $script = @'
Write-Host "Installing OpenSSL..." -ForegroundColor Cyan
winget install --id ShiningLight.OpenSSL.Dev --accept-source-agreements --accept-package-agreements
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
Write-Host "Done." -ForegroundColor Green
Read-Host "Press Enter to close"
'@

    if (Test-IsAdmin) {
        $sb = [scriptblock]::Create($script)
        & $sb
    } else {
        Write-Host "  Requesting administrator privileges..." -ForegroundColor Yellow
        $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($script))
        $proc = Start-Process powershell -Verb RunAs -Wait -PassThru `
            -ArgumentList "-ExecutionPolicy Bypass -EncodedCommand $encoded"
        if ($proc.ExitCode -ne 0) {
            Write-Host "  [!!] Elevated installer exited with code $($proc.ExitCode)" -ForegroundColor Red
        }
    }

    # Verify
    Write-Host ""
    if (Test-Path "$opensslDir\include\openssl\ssl.h") {
        Write-Host "  [OK] OpenSSL installed" -ForegroundColor Green
    } else {
        Write-Host "  [!!] OpenSSL — may need to restart shell" -ForegroundColor Red
    }
}

Write-Host ""
Write-Host "  Run 01-configure.ps1 next to detect paths and generate config." -ForegroundColor DarkGray
Write-Host ""
