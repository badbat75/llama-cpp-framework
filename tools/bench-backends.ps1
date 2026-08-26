#requires -Version 7.0
<#
.SYNOPSIS
    Comparative llama-bench harness: the SAME AMD card driven through ROCm/HIP
    against Vulkan, with the NVIDIA half of the split held constant.

.DESCRIPTION
    Mirrors a production preset (device order, tensor split, ubatch, KV quant) so
    the numbers describe the configuration actually in use, and varies exactly one
    thing: which backend drives the AMD card. The NVIDIA card stays on CUDA in both
    legs, because switching it to Vulkan as well would change two variables at once.

    THREE THINGS llama-bench CANNOT DO, and they matter when reading the output:

    - It takes no prompts and no sampler settings. There is no `temp` to set to 0
      because it never samples: it times the graph over synthetic token counts. That
      is a stronger reproducibility guarantee than a fixed prompt at temperature 0,
      not a weaker one, since the result cannot depend on what the model says. The
      `-Prompts` parameter here is therefore prompt LENGTHS, not texts.
    - It has no speculative decoding. The preset's FastMTP drafter is absent, so the
      tg numbers are the unassisted decode rate and will read roughly HALF of what
      the same model does in llama-server with MTP on (see the MTP notes in AGENTS.md).
      The ROCm-versus-Vulkan RATIO is still valid; the absolute tg is not a production
      figure.
    - It ignores the mmproj, the chat template and the reasoning flags. Irrelevant to
      throughput, listed so nobody hunts for them in the results.

    Reliability measures baked in, each one earned:

    - Devices are resolved BY NAME, never by index. ROCm and Vulkan enumerate in
      different orders and the ids are not stable across driver states; on this box
      `Vulkan0` is the iGPU, and a bench that silently ran there would produce
      numbers that look merely disappointing rather than wrong.
    - The runs are INTERLEAVED across passes (pass 1 runs A then B, pass 2 runs B
      then A). A sustained bench heats the card and clocks drift downward, which
      would otherwise be charged entirely to whichever backend ran second.
    - The inter-test delay is deliberately kept BELOW the ~9.5 s idle threshold of
      the WDDM VRAM eviction this machine is subject to (see the eviction notes in
      the project memory). A longer settle time between tests risks measuring a
      page-in instead of a kernel.
    - `ROCBLAS_USE_HIPBLASLT=0` is exported for the ROCm leg only, process-scoped,
      matching `server.ini`'s `RocblasUseHipblaslt = false`. Never set it machine-wide.
    - Flash attention is PINNED rather than left on `auto`, because `auto` may resolve
      differently per backend and would confound the comparison. The aggregator checks
      the resolved value in the CSV and warns if the two legs still disagree.
    - The environment is stamped into the results file: driver version per adapter,
      boot time, and any Windows Update display-driver activity in the last 24 h.
      That last one exists because a WU driver churn silently invalidated a full
      day of measurements on this machine on 2026-08-24.

.EXAMPLE
    .\tools\bench-backends.ps1 -StopServer

.EXAMPLE
    .\tools\bench-backends.ps1 -Prompts 2048 -Depths 0 -Reps 1 -Passes 1 -DryRun
#>
[CmdletBinding()]
param(
    [string]   $Model       = 'E:\llama.cpp\models\Qwen3.8-27B-Uncensored-HauhauCS-Aggressive-Q6_K_P.gguf',

    # Prompt LENGTHS for the prefill sweep, and KV depths for the decode sweep.
    [int[]]    $Prompts     = @(2048, 8192, 32768),
    [int[]]    $Depths      = @(0, 8192, 32768),
    [int]      $Gen         = 128,

    # Preset mirror. TensorSplit uses llama-bench's '/' separator, not the INI's ','
    # (a comma there means "run these as separate configurations" and would silently
    # turn one split into two benchmarks).
    [string]   $TensorSplit = '54/12',
    [int]      $Ubatch      = 384,
    [string]   $CacheType   = 'q8_0',
    [ValidateSet('on', 'off', 'auto')]
    [string]   $FlashAttn   = 'on',

    # Which physical card is under test, matched against the device NAME.
    [string]   $AmdMatch    = 'R9700',

    [int]      $Reps        = 3,
    [int]      $Passes      = 2,
    [int]      $Delay       = 2,
    [ValidateRange(-1, 3)]
    [int]      $Prio        = 1,

    [string]   $OutDir      = (Join-Path $PSScriptRoot '..\build\bench'),
    [switch]   $StopServer,
    [switch]   $DryRun
)

$ErrorActionPreference = 'Stop'

# ---------------------------------------------------------------- locate the exe

$benchCandidates = @(
    'C:\Program Files\llama.cpp\bin\llama-bench.exe'
    (Join-Path $PSScriptRoot '..\build\llama.cpp-cmake\bin\llama-bench.exe')
)
$bench = $benchCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $bench) {
    throw "llama-bench.exe not found. Looked in:`n  " + ($benchCandidates -join "`n  ")
}
$bench = (Resolve-Path $bench).Path

if (-not (Test-Path $Model)) { throw "Model not found: $Model" }

# ------------------------------------------------------------------- preflight

# llama-server holds the whole model resident. Benching alongside it does not fail
# cleanly, it spills into shared memory and reports numbers that look like a backend
# regression, so this is a hard stop rather than a warning.
$running = Get-Process llama-server -ErrorAction SilentlyContinue
if ($running) {
    if ($DryRun) {
        Write-Warning ("llama-server is running (PID $($running.Id -join ', ')). " +
                       'A real run needs it stopped: pass -StopServer.')
    }
    elseif (-not $StopServer) {
        throw ("llama-server is running (PID $($running.Id -join ', ')) and holds the GPU. " +
               'Stop it first, or re-run with -StopServer.')
    }
}
if ($running -and -not $DryRun) {
    Write-Host 'Stopping llama-server...' -ForegroundColor Yellow
    $running | Stop-Process -Force
    # The driver does not release the allocations synchronously with the process exit.
    $deadline = (Get-Date).AddSeconds(30)
    while ((Get-Process llama-server -ErrorAction SilentlyContinue) -and (Get-Date) -lt $deadline) {
        Start-Sleep -Milliseconds 500
    }
}

function Get-BenchDevices {
    $out = & $bench --list-devices 2>&1
    $devs = foreach ($line in $out) {
        if ("$line" -match '^\s*(?<id>(?<backend>ROCm|Vulkan|CUDA|SYCL|BLAS)\d+):\s+(?<name>.+?)\s+\((?<total>\d+)\s*MiB,\s*(?<free>\d+)\s*MiB free\)') {
            [pscustomobject]@{
                Id       = $Matches.id
                Backend  = $Matches.backend
                Name     = $Matches.name.Trim()
                TotalMiB = [int]$Matches.total
                FreeMiB  = [int]$Matches.free
            }
        }
    }
    if (-not $devs) { throw 'Could not parse any device from llama-bench --list-devices.' }
    $devs
}

$devices = Get-BenchDevices

function Select-One {
    param([object[]]$Pool, [string]$Backend, [string]$NamePattern, [string]$What)
    $hit = @($Pool | Where-Object { $_.Backend -eq $Backend -and $_.Name -match $NamePattern })
    if ($hit.Count -eq 0) {
        throw ("No $Backend device whose name matches '$NamePattern' ($What). Seen: " +
               (($Pool | ForEach-Object { "$($_.Id)=$($_.Name)" }) -join ', '))
    }
    if ($hit.Count -gt 1) {
        throw ("Ambiguous $Backend match for '$NamePattern': " + (($hit.Id) -join ', ') +
               '. Narrow -AmdMatch.')
    }
    $hit[0]
}

$amdRocm = Select-One -Pool $devices -Backend 'ROCm'   -NamePattern $AmdMatch -What 'AMD card under test'
$amdVk   = Select-One -Pool $devices -Backend 'Vulkan' -NamePattern $AmdMatch -What 'AMD card under test'
$nvidia  = @($devices | Where-Object Backend -eq 'CUDA') | Select-Object -First 1
if (-not $nvidia) { throw 'No CUDA device found for the partial-offload half of the split.' }

# ------------------------------------------------------------------ the two legs

$legs = @(
    [pscustomobject]@{
        Label  = 'ROCm'
        Device = "$($amdRocm.Id)/$($nvidia.Id)"
        Env    = @{ ROCBLAS_USE_HIPBLASLT = '0' }   # mirrors server.ini
    }
    [pscustomobject]@{
        Label  = 'Vulkan'
        Device = "$($amdVk.Id)/$($nvidia.Id)"
        Env    = @{}
    }
)

$common = @(
    '-m',   $Model
    '-ts',  $TensorSplit
    '-ub',  "$Ubatch"
    '-ctk', $CacheType
    '-ctv', $CacheType
    '-fa',  $FlashAttn
    '-ngl', '99'
    '-r',   "$Reps"
    '--delay', "$Delay"
    '--prio',  "$Prio"
    '--progress'
    '-o',   'csv'
)

# Two sweeps per leg: prefill only, then decode at depth. Kept separate because
# `-p N -n M` in one invocation benchmarks them as independent tests anyway, and
# splitting makes a partial run still produce a usable half.
$sweeps = @(
    [pscustomobject]@{ Name = 'prefill'; Args = @('-p', ($Prompts -join ','), '-n', '0') }
    [pscustomobject]@{ Name = 'decode';  Args = @('-p', '0', '-n', "$Gen", '-d', ($Depths -join ',')) }
)

# --------------------------------------------------------------------- plan

$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
if (-not (Test-Path $OutDir)) { New-Item -ItemType Directory -Path $OutDir -Force | Out-Null }
$OutDir  = (Resolve-Path $OutDir).Path
$csvPath = Join-Path $OutDir "bench-$stamp.csv"
$mdPath  = Join-Path $OutDir "bench-$stamp.md"
# llama-bench's own chatter (backend init, model load, per-test progress) goes to
# stderr. It is captured rather than discarded so a run in progress can be watched
# and a run that dies leaves evidence: results written only at the end are results
# lost on the first crash, which is exactly how a day of GPU measurements was lost
# here on 2026-08-24.
$logPath = Join-Path $OutDir "bench-$stamp.log"

Write-Host ''
Write-Host 'Backend comparison plan' -ForegroundColor Cyan
Write-Host ("  model        : {0}" -f (Split-Path $Model -Leaf))
Write-Host ("  AMD card     : {0}" -f $amdRocm.Name)
Write-Host ("  ROCm leg     : -dev {0}" -f $legs[0].Device)
Write-Host ("  Vulkan leg   : -dev {0}" -f $legs[1].Device)
Write-Host ("  split / ub   : {0} / {1}" -f $TensorSplit, $Ubatch)
Write-Host ("  KV / fa      : {0} / {1}" -f $CacheType, $FlashAttn)
Write-Host ("  prefill      : -p {0}" -f ($Prompts -join ','))
Write-Host ("  decode       : -n {0} at depths {1}" -f $Gen, ($Depths -join ','))
Write-Host ("  reps x passes: {0} x {1} = {2} samples per point" -f $Reps, $Passes, ($Reps * $Passes))
Write-Host ("  output       : {0}" -f $csvPath)
Write-Host ''

if ($DryRun) { Write-Host 'DryRun: nothing executed.' -ForegroundColor Yellow; return }

# ------------------------------------------------------------------- execution

$rows = [System.Collections.Generic.List[object]]::new()

for ($pass = 1; $pass -le $Passes; $pass++) {
    # Alternate the order every pass so thermal drift does not accumulate against
    # whichever backend always ran last.
    $order = if ($pass % 2 -eq 1) { $legs } else { $legs[($legs.Count - 1)..0] }

    foreach ($leg in $order) {
        foreach ($sweep in $sweeps) {
            Write-Host ("[pass $pass] $($leg.Label) / $($sweep.Name)") -ForegroundColor Green

            $saved = @{}
            foreach ($k in $leg.Env.Keys) {
                $saved[$k] = [Environment]::GetEnvironmentVariable($k, 'Process')
                [Environment]::SetEnvironmentVariable($k, $leg.Env[$k], 'Process')
            }
            try {
                $argv = $common + $sweep.Args + @('-dev', $leg.Device)
                "=== pass $pass / $($leg.Label) / $($sweep.Name) / $(Get-Date -Format 'HH:mm:ss') ===" |
                    Add-Content -Path $logPath -Encoding utf8
                $out  = & $bench @argv 2>>$logPath
                $csv  = $out | Where-Object { $_ -match ',' }
                if ($csv) {
                    $parsed = $csv | ConvertFrom-Csv
                    foreach ($r in $parsed) {
                        Add-Member -InputObject $r -NotePropertyName 'leg'  -NotePropertyValue $leg.Label -Force
                        Add-Member -InputObject $r -NotePropertyName 'pass' -NotePropertyValue $pass      -Force
                        $rows.Add($r)
                    }
                    # Flush to disk per invocation, not at the end: a killed run keeps
                    # every leg it completed.
                    if (Test-Path $csvPath) {
                        $parsed | Export-Csv -Path $csvPath -NoTypeInformation -Encoding utf8 -Append
                    } else {
                        $parsed | Export-Csv -Path $csvPath -NoTypeInformation -Encoding utf8
                    }
                } else {
                    Write-Warning "no CSV rows from $($leg.Label)/$($sweep.Name)"
                }
            } finally {
                foreach ($k in $saved.Keys) {
                    [Environment]::SetEnvironmentVariable($k, $saved[$k], 'Process')
                }
            }
        }
    }
}

if ($rows.Count -eq 0) { throw 'No results collected.' }
# The CSV is already on disk: it was appended per invocation above.

# ------------------------------------------------------------------ aggregation

function Get-TestLabel {
    param($Row)
    if ([int]$Row.n_prompt -gt 0) { "pp$($Row.n_prompt)" }
    elseif ([int]$Row.n_depth -gt 0) { "tg$($Row.n_gen) @d$($Row.n_depth)" }
    else { "tg$($Row.n_gen)" }
}

$agg = $rows | Group-Object { Get-TestLabel $_ }, leg | ForEach-Object {
    $vals = @($_.Group | ForEach-Object { [double]$_.avg_ts })
    $mean = ($vals | Measure-Object -Average).Average
    $sd   = if ($vals.Count -gt 1) {
        [math]::Sqrt((($vals | ForEach-Object { ($_ - $mean) * ($_ - $mean) } | Measure-Object -Sum).Sum) / ($vals.Count - 1))
    } else { 0 }
    [pscustomobject]@{
        Test    = $_.Group[0] | ForEach-Object { Get-TestLabel $_ }
        Leg     = $_.Group[0].leg
        Mean    = [math]::Round($mean, 2)
        Sd      = [math]::Round($sd, 2)
        Samples = $vals.Count
    }
}

# A resolved flash_attn that differs per leg makes the comparison meaningless, so
# say so loudly rather than letting it hide in a column nobody reads.
$faSeen = $rows | Group-Object leg | ForEach-Object {
    [pscustomobject]@{ Leg = $_.Name; Fa = (($_.Group.flash_attn | Sort-Object -Unique) -join '/') }
}
$faWarn = ($faSeen.Fa | Sort-Object -Unique).Count -gt 1

# ------------------------------------------------------------- environment stamp

$boot = (Get-CimInstance Win32_OperatingSystem).LastBootUpTime
$gpus = Get-CimInstance Win32_VideoController |
    Where-Object { $_.Name -notmatch 'Remote Display' } |
    Select-Object Name, DriverVersion, DriverDate
$wu = Get-WinEvent -FilterHashtable @{ LogName = 'System'; StartTime = (Get-Date).AddHours(-24) } -ErrorAction SilentlyContinue |
    Where-Object { $_.ProviderName -match 'WindowsUpdateClient' -and $_.Message -match 'Display|Advanced Micro' } |
    Select-Object -First 5 TimeCreated, Id

$md = [System.Collections.Generic.List[string]]::new()
$md.Add("# Backend comparison: ROCm vs Vulkan")
$md.Add('')
$md.Add("Generated $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss'). Model ``$(Split-Path $Model -Leaf)``.")
$md.Add('')
$md.Add('| setting | value |')
$md.Add('|---|---|')
$md.Add("| AMD card | $($amdRocm.Name) |")
$md.Add("| ROCm leg | ``-dev $($legs[0].Device)`` + ``ROCBLAS_USE_HIPBLASLT=0`` |")
$md.Add("| Vulkan leg | ``-dev $($legs[1].Device)`` |")
$md.Add("| tensor-split / ubatch | ``$TensorSplit`` / $Ubatch |")
$md.Add("| KV cache / flash-attn | $CacheType / $FlashAttn |")
$md.Add("| reps x passes | $Reps x $Passes |")
$md.Add("| llama-bench | build $($rows[0].build_number) ($($rows[0].build_commit)) |")
$md.Add('')
if ($faWarn) {
    $md.Add('> **WARNING** the resolved `flash_attn` differs between legs: ' +
            (($faSeen | ForEach-Object { "$($_.Leg)=$($_.Fa)" }) -join ', ') +
            '. The comparison is confounded; pin `-FlashAttn on` or `off`.')
    $md.Add('')
}
$md.Add('## Results (tokens/s, mean of all samples, +- sample stddev across passes)')
$md.Add('')
$md.Add('| test | ROCm | Vulkan | ROCm / Vulkan |')
$md.Add('|---|---:|---:|---:|')
foreach ($t in ($agg.Test | Select-Object -Unique)) {
    $r = $agg | Where-Object { $_.Test -eq $t -and $_.Leg -eq 'ROCm' }
    $v = $agg | Where-Object { $_.Test -eq $t -and $_.Leg -eq 'Vulkan' }
    $ratio = if ($r -and $v -and $v.Mean -gt 0) { '{0:N2}x' -f ($r.Mean / $v.Mean) } else { '-' }
    $rc = if ($r) { '{0:N1} +- {1:N1}' -f $r.Mean, $r.Sd } else { '-' }
    $vc = if ($v) { '{0:N1} +- {1:N1}' -f $v.Mean, $v.Sd } else { '-' }
    $md.Add("| $t | $rc | $vc | $ratio |")
}
$md.Add('')
$md.Add('## Environment')
$md.Add('')
$md.Add("- boot: $boot (a driver installed after this boot has not settled; see below)")
foreach ($g in $gpus) { $md.Add("- $($g.Name): driver $($g.DriverVersion), dated $($g.DriverDate)") }
if ($wu) {
    # What invalidates a measurement is a driver install the machine has not rebooted
    # into, not the mere existence of one in the last day. Comparing against the boot
    # time is the whole point: an install that predates the boot has settled.
    $unsettled = @($wu | Where-Object { $_.TimeCreated -gt $boot })
    $md.Add('- Windows Update display-driver activity in the last 24 h:')
    foreach ($e in $wu) { $md.Add("  - $($e.TimeCreated) (event $($e.Id))") }
    if ($unsettled) {
        $md.Add('  **PROVISIONAL: the newest of these post-dates the last boot.** The driver has not' +
                ' settled; reboot and re-run before trusting these numbers.')
    } else {
        $md.Add('  All of them pre-date the last boot, so the driver has settled and the results stand.')
    }
} else {
    $md.Add('- no Windows Update display-driver activity in the last 24 h')
}
$md.Add('')
$md.Add('## Caveats')
$md.Add('')
$md.Add('- No speculative decoding: llama-bench cannot load the preset''s MTP drafter, so `tg` here is the unassisted rate and reads well below production.')
$md.Add('- No sampling at all, hence no temperature: results are content-independent by construction.')
$md.Add('- `pp` numbers are batch prefill, not time-to-first-token.')

Set-Content -Path $mdPath -Value $md -Encoding utf8

Write-Host ''
$md | Where-Object { $_ -match '^\|' -or $_ -match '^#' } | ForEach-Object { Write-Host $_ }
Write-Host ''
Write-Host "CSV : $csvPath" -ForegroundColor Cyan
Write-Host "Report: $mdPath" -ForegroundColor Cyan
if ($faWarn) { Write-Warning 'flash_attn resolved differently per leg: see the report.' }
