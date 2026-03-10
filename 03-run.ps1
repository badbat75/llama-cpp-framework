# Run llama-server

. "$PSScriptRoot\common.ps1"  # loads $cfg, activates VS Dev Shell + ROCm

$env:LLAMA_CACHE = $cfg.CacheDir

$serverExe = Join-Path $cfg.LlamaCppDir "build\bin\llama-server.exe"
if (-not (Test-Path $serverExe)) {
    throw "llama-server.exe not found at $serverExe. Run 02-build.ps1 first."
}

$serverArgs = @(
    "-hf", $cfg.Model
    "--cache-type-k", $cfg.CacheTypeK
    "--cache-type-v", $cfg.CacheTypeV
    "-np", $cfg.Parallel
    "-ngl", $cfg.GpuLayers
    "--ctx-size", $cfg.CtxSize
    "--port", $cfg.Port
)

if ($cfg.FlashAttn) { $serverArgs += "-fa", "on" }
if ($cfg.Jinja)     { $serverArgs += "--jinja" }

# CPU threads for offloaded layers: all cores -2 if >8 cores, otherwise all -1
$cpuCores = [Environment]::ProcessorCount
$threads = if ($cpuCores -gt 8) { $cpuCores - 2 } else { $cpuCores - 1 }
$serverArgs += "-t", $threads

Write-Host "Starting llama-server on port $($cfg.Port)..." -ForegroundColor Cyan
& $serverExe @serverArgs
