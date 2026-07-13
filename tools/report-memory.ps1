# Memory slicing report for the llama-server instance that is running right now.
#
# Two independent sources, cross-checked against each other:
#   1. llama-server's own load log (%LOCALAPPDATA%\llama.cpp\logs\llama-server.log)
#      — every buffer llama.cpp allocates announces itself there with its size and
#      its device, so the REQUESTED footprint can be rebuilt exactly.
#   2. Windows' GPU perf counters for the live process — what the driver ACTUALLY
#      handed out, split into dedicated VRAM vs shared (system RAM over PCIe).
#
# Divergence between the two is the whole point of the report: when a device is
# oversubscribed, WDDM does not fail the allocation, it silently pages the excess
# into shared memory, and the only symptom is that decoding got slow. (1) says how
# much was asked for, (2) says how much of it landed in real VRAM.
#
# TENSOR PLACEMENT is not a section of its own: it is an 'of which placed' COLUMN on
# each balance sheet, filled in on the 'model weights' row — because that is what an
# --override-tensor rule does. It moves a tensor INTO some device's model buffer; it
# allocates nothing extra. So it is already inside that row and inside 'requested', and
# giving it a row to be summed would push 'requested' past what the card really holds.
# The rules come from the router's spawn line, so the server-replaces-preset merge never
# has to be re-derived from the INI files; the per-tensor sizes come from a DEBUG line
# (verbosity 5), so at the usual -lv 4 the cell names the rules and reads '? (needs -lv
# 5)'. A rule that matched nothing belongs to no row at all, and gets the one line
# printed outside the balance sheets.
#
# Standalone on purpose: reads only the runtime tree under %LOCALAPPDATA%, so it
# works on an installed machine with no build config and no repo checkout.
#
# Usage:
#   .\report-memory.ps1
#   .\report-memory.ps1 -Json          # machine-readable, for piping
#   .\report-memory.ps1 -LogPath <path>

[CmdletBinding()]
param(
    [string] $LogPath = (Join-Path $env:LOCALAPPDATA 'llama.cpp\logs\llama-server.log'),
    [switch] $Json
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# ---------------------------------------------------------------- log block ---

# The last load block = everything from the final 'device_info' banner onwards.
# Read the tail in growing bites so a 20 MB log with -lv 4 per-request spam does
# not have to be slurped whole just to find a block that is usually near the end.
#
# Also returns the child's command line. The router (the llama-server the user
# launched, with --models-preset) prints the args it is about to spawn a child
# with, one per line, JUST BEFORE that child starts logging — so the args sit
# outside the block and have to be picked up separately. They are the only place
# the EFFECTIVE options appear: the router overlays its own CLI args onto every
# preset, so what a child actually gets is neither file alone.
function Get-LastLoadBlock {
    param([string] $Path)

    if (-not (Test-Path $Path)) { throw "Log not found: $Path" }

    foreach ($tail in 5000, 50000, 200000, 0) {
        $lines = if ($tail -eq 0) { Get-Content -LiteralPath $Path } else { Get-Content -LiteralPath $Path -Tail $tail }
        # llama-server's own colouring, if any, would break every regex below.
        $lines = $lines -replace "`e\[[0-9;]*m", ''
        $starts = @($lines | Select-String -SimpleMatch 'common_param: device_info' | ForEach-Object { $_.LineNumber })
        if ($starts.Count -gt 0) {
            $from = $starts[-1] - 1
            # The child prints its build and its verbosity JUST ABOVE the device_info
            # banner, and verbosity decides whether the per-tensor override lines exist
            # at all — so the block has to reach back over them. Scan a small window
            # rather than walking contiguously backwards: the two banners are not always
            # adjacent to device_info (at -lv 5 a blank line lands between them).
            $edge = [math]::Max(0, $from - 10)
            for ($i = $from - 1; $i -ge $edge; $i--) {
                if ($lines[$i] -match 'common_params_print_info:') { $from = $i }
            }
            $block = $lines[$from..($lines.Count - 1)]
            # A block that never reached load_model is a crashed/aborted start.
            if ($block | Select-String -SimpleMatch 'srv    load_model: initializing' -Quiet) {
                return [pscustomobject]@{
                    Lines     = $block
                    # @() or PowerShell unrolls 'no args' to $null on the way out.
                    SpawnArgs = @(Get-SpawnArgs -Lines $lines -Before $from -Block $block)
                }
            }
        }
        if ($tail -eq 0) { break }
    }
    throw "No completed model load found in $Path. Is llama-server running?"
}

# Every child line carries its own port as a `[56996]` prefix, and the router
# names that port when it announces the spawn — so the args are matched to the
# block by PORT, not by "the spawn header nearest above", which would pick the
# wrong one whenever the router has several models up. A llama-server started
# WITHOUT the router has no prefix and no spawn line: then there are simply no
# args to read, and the placement section says so rather than guessing.
function Get-SpawnArgs {
    param([string[]] $Lines, [int] $Before, [string[]] $Block)

    $port = $null
    foreach ($l in $Block) { if ($l -match '^\[(?<p>\d+)\]') { $port = $Matches.p; break } }
    if (-not $port) { return @() }

    $header = @($Lines[0..$Before] | Select-String -Pattern "spawning server instance with name=.* on port $port\b") |
        Select-Object -Last 1
    if (-not $header) { return @() }

    # The router prints 'load: spawning ... with args:' with ONE space after the
    # colon, then one arg per line indented under it with two or more. That indent
    # is the only thing separating an arg (which can be any string at all — regexes,
    # JSON, paths with spaces) from the router's next log line. argv[0], the exe
    # path, comes through as the first 'arg'; harmless, nothing looks it up.
    $argv = @()
    for ($i = $header.LineNumber; $i -lt $Lines.Count; $i++) {
        if ($Lines[$i] -match 'srv\s+load: spawning server instance with args:') { continue }
        if ($Lines[$i] -notmatch 'srv\s+load:\s{2,}(?<arg>.+?)\s*$')             { break }
        $argv += $Matches.arg
    }
    $argv
}

# ------------------------------------------------------------------- parsing ---

function ConvertFrom-LoadBlock {
    param([string[]] $Block)

    $info = [ordered]@{
        Model      = $null
        Arch       = $null
        ParamsB    = $null
        FileType   = $null
        FileSizeGiB= $null
        NCtx       = $null
        NSeqMax    = $null
        FlashAttn  = $null
        CacheTypes = $null
        Verbosity  = $null
        MmprojMiB  = 0.0
        Devices    = [ordered]@{}   # log device name -> @{ TotalMiB; FreeMiB; Desc }
        Buffers    = @()            # one row per allocation
        Hits       = @()            # one row per tensor moved by an --override-tensor rule
    }

    # Contexts are announced in order: ctx 1 = the model, ctx 2 (if any) = the
    # speculative/MTP draft. Buffers are attributed to whichever context is open.
    $ctxIndex = 0

    # Which of a sliding-window model's two KV caches is currently being built.
    $swa = $false

    foreach ($line in $Block) {

        # - ROCm0   : AMD Radeon AI PRO R9700 (32624 MiB, 32462 MiB free)
        if ($line -match 'common_param:\s+-\s+(?<dev>\S+)\s*:\s+(?<desc>.+?)\s+\((?<total>\d+)\s+MiB,\s+(?<free>\d+)\s+MiB free\)') {
            $info.Devices[$Matches.dev] = [ordered]@{
                Desc     = $Matches.desc.Trim()
                TotalMiB = [double] $Matches.total
                FreeMiB  = [double] $Matches.free
            }
            continue
        }

        # verbosity = 5 (adjust with the `-lv N` CLI arg)
        if ((-not $info.Verbosity) -and $line -match 'common_params_print_info: verbosity = (?<v>\d+)') {
            $info.Verbosity = [int] $Matches.v
            continue
        }

        # D tensor token_embd.weight (1030 MiB q8_0) buffer type overridden to ROCm0
        #
        # The one line that says an --override-tensor rule DID something, and the
        # only per-tensor accounting in the whole log. It is LLAMA_LOG_DEBUG, and
        # DEBUG is verbosity 5 (common/log.h: LOG_LEVEL_DEBUG = 5, printed when
        # `verbosity <= thold`), so at the usual -lv 4 it is absent — silence here
        # means "not logged", never "no tensor matched".
        #
        # 'buffer type' is the RESOLVED one, not the one the rule asked for: a rule
        # ending in =CPU comes back as CUDA_Host/ROCm_Host whenever a GPU backend is
        # loaded and mmap is off (llama.cpp picks the GPU's pinned host buffer for
        # CPU-side weights) — i.e. it lands in exactly the memory Windows charts as
        # 'Shared GPU'. Print what the log says, never what the rule asked for.
        if ($line -match 'tensor (?<name>\S+) \((?<mib>\d+) MiB (?<type>[^)]+)\) buffer type overridden to (?<buft>\S+)') {
            $info.Hits += [pscustomobject]@{
                Tensor = $Matches.name
                MiB    = [double] $Matches.mib   # %zu in llama.cpp: truncated, so a sub-MiB tensor reads 0
                Type   = $Matches.type
                Buft   = $Matches.buft
            }
            continue
        }

        # First occurrence only, all four: a separate draft/MTP head file (gemma4-assistant
        # and friends) prints its own print_info block AFTER the model's, and it would
        # otherwise win — reporting the head's arch and its nonsense parameter count
        # ('422.86 B params' for a 12B model) as if they were the model's.
        if ((-not $info.Arch)        -and $line -match 'print_info: arch\s+=\s+(?<v>\S+)')             { $info.Arch        = $Matches.v; continue }
        if ((-not $info.ParamsB)     -and $line -match 'print_info: model params\s+=\s+(?<v>[\d.]+)')  { $info.ParamsB     = [double] $Matches.v; continue }
        if ((-not $info.FileType)    -and $line -match 'print_info: file type\s+=\s+(?<v>.+?)\s*$')    { $info.FileType    = $Matches.v; continue }
        if ((-not $info.FileSizeGiB) -and $line -match 'print_info: file size\s+=\s+(?<v>[\d.]+) GiB') { $info.FileSizeGiB = [double] $Matches.v; continue }
        if ($line -match 'llama_context: n_ctx\s+=\s+(?<v>\d+)'      -and -not $info.NCtx)      { $info.NCtx      = [int] $Matches.v; continue }
        if ($line -match 'llama_context: n_seq_max\s+=\s+(?<v>\d+)'  -and -not $info.NSeqMax)   { $info.NSeqMax   = [int] $Matches.v; continue }
        if ($line -match 'llama_context: flash_attn\s+=\s+(?<v>\S+)' -and -not $info.FlashAttn) { $info.FlashAttn = $Matches.v; continue }

        # K (q8_0): 4352.00 MiB, V (q8_0): 4352.00 MiB
        if ((-not $info.CacheTypes) -and $line -match 'llama_kv_cache: size.*K \((?<k>[^)]+)\).*V \((?<v>[^)]+)\)') {
            $info.CacheTypes = "K=$($Matches.k) V=$($Matches.v)"
            continue
        }

        # The model id llama-server reports to its router once the load succeeded.
        if ((-not $info.Model) -and $line -match '"state":"ready".*?"id":"(?<id>[^"]+)"') {
            $info.Model = $Matches.id
            continue
        }

        # A sliding-window model builds TWO KV caches (llama_kv_cache_iswa: a full one
        # for the global-attention layers, a small one for the sliding-window layers),
        # and each announces its buffers with the SAME 'llama_kv_cache: <dev> KV buffer
        # size' line — same source, same device, same kind. Gemma-4-12B, for one:
        #     creating non-SWA KV cache, size = 98304 cells
        #     llama_kv_cache:  CUDA0 KV buffer size = 816.00 MiB
        #     creating     SWA KV cache, size = 1536 cells
        #     llama_kv_cache:  CUDA0 KV buffer size = 255.00 MiB
        # Both are allocated and both are on the card. Without this latch they collapse
        # into one (see the de-duplication below) and the report quietly loses the SWA
        # cache — 255 MiB in that example. A latch is safe here where it was not for the
        # clip graph: llama.cpp prints the banner immediately before each cache is built,
        # in both the fit dry run and the real load.
        if ($line -match 'llama_kv_cache_iswa: creating\s+(?<w>non-SWA|SWA) KV cache') {
            $swa = $Matches.w -eq 'SWA'
            continue
        }

        if ($line -match 'llama_context: constructing llama_context') {
            $ctxIndex++
            $swa = $false   # the draft context builds its own caches, starting at base
            continue
        }

        # The vision encoder (clip/mtmd) loads last and prints its own weight total.
        if ($line -match 'load_hparams: model size:\s+(?<v>[\d.]+) MiB') {
            $info.MmprojMiB = [double] $Matches.v
            continue
        }

        # load_tensors:  CUDA0 model buffer size = 7921.00 MiB
        # llama_kv_cache:  ROCm0 KV buffer size = 6528.00 MiB
        # llama_memory_recurrent:  CUDA0 RS buffer size = 498.75 MiB
        # sched_reserve:  ROCm_Host compute buffer size = 266.28 MiB
        # reserve_compute_meta:  ROCm0 compute buffer size = 248.10 MiB   (mmproj)
        # llama_adapter_lora_init_impl:  CUDA0 LoRA buffer size = 42.00 MiB
        # llama_kv_cache_dsv4:  CUDA0 DSV4 kv state buffer size = 12.00 MiB
        #
        # Those are ALL the 'buffer size' lines llama.cpp can print (grep the source: it
        # is model / KV / RS / compute / output / LoRA / DSV4 <name> state, and nothing
        # else). Anything not matched here is not reported as smaller — it vanishes from
        # the balance sheet entirely, which is why the list is exhaustive rather than
        # just the kinds the machine at hand happens to allocate.
        if ($line -match '(?<src>[\w:]+):\s+(?<dev>\S+)\s+(?<kind>model|KV|RS|compute|output|LoRA|DSV4 \S+ state)\s+buffer size\s*=\s*(?<mib>[\d.]+) MiB') {
            # reserve_compute_meta is the clip/mtmd graph and ONLY that. The vision
            # encoder loads after the draft context, and llama.cpp then re-emits the
            # draft's sched_reserve lines: those still belong to the draft, so the
            # owner cannot be a "we have seen the clip banner" latch — it has to be
            # the source that printed the line.
            $owner =
                if     ($Matches.src -eq 'reserve_compute_meta') { 'mmproj' }
                elseif ($ctxIndex -ge 2)                         { 'draft' }
                else                                             { 'model' }

            $kind = $Matches.kind
            if ($kind -eq 'KV' -and $swa) { $kind = 'KV-SWA' }

            $info.Buffers += [pscustomobject]@{
                Owner  = $owner
                Kind   = $kind
                Device = $Matches.dev
                MiB    = [double] $Matches.mib
                Src    = $Matches.src
            }
            continue
        }
    }

    # llama.cpp re-reserves the compute graph after the slots come up: the same
    # buffer announced twice, NOT a second allocation. Collapse duplicates by
    # keeping the largest value per (owner, kind, device) — summing double-counts.
    $info.Buffers = $info.Buffers |
        Group-Object Owner, Kind, Device |
        ForEach-Object { $_.Group | Sort-Object MiB -Descending | Select-Object -First 1 }

    # Same trap one level down: with --fit on (the default) llama.cpp LOADS THE
    # MODEL TWICE — a no_alloc dry run to size the fit, then the real one — and the
    # loader re-emits an override line per tensor each time. Collapse by tensor name;
    # counting the raw lines double-counts every rule's hits.
    $info.Hits = $info.Hits | Group-Object Tensor | ForEach-Object { $_.Group[-1] }

    # The mmproj weights are not announced as a "buffer size" line; attribute them
    # to the device its compute buffer landed on (that is the backend clip chose).
    if ($info.MmprojMiB -gt 0) {
        $clipDev = $info.Buffers |
            Where-Object { $_.Owner -eq 'mmproj' -and $_.Device -notmatch '_Host$|^CPU' } |
            Select-Object -First 1 -ExpandProperty Device
        if ($clipDev) {
            $info.Buffers += [pscustomobject]@{
                Owner = 'mmproj'; Kind = 'model'; Device = $clipDev; MiB = $info.MmprojMiB; Src = 'load_hparams'
            }
        }
    }

    [pscustomobject] $info
}

# --------------------------------------------------------- tensor placement ---

# The --override-tensor rules that actually reached this child, taken from the
# spawn line rather than from the config files, because the files do not say what
# ran: the router merges its own CLI args into every preset as a KEY->VALUE map,
# so a server-wide rule REPLACES the preset's own rather than adding to it, and
# exactly one --override-tensor is ever handed to a child. The spawn line is that
# winner, already resolved.
function Get-OverrideRules {
    param([string[]] $SpawnArgs)

    if (-not $SpawnArgs) { return @() }   # a llama-server started without the router

    $i = [array]::IndexOf($SpawnArgs, '--override-tensor')
    if ($i -lt 0 -or $i + 1 -ge $SpawnArgs.Count) { return @() }

    # `<pattern>=<buffer type>` joined by ',', split at the FIRST '='. This is
    # llama.cpp's own grammar and it has no escaping — which is why a `{1,2}`
    # quantifier in a pattern tears the rule in half before anyone parses it.
    @($SpawnArgs[$i + 1] -split ',' | Where-Object { $_.Trim() } | ForEach-Object {
        $eq = $_.IndexOf('=')
        [pscustomobject]@{
            Pattern = if ($eq -ge 0) { $_.Substring(0, $eq) } else { $_ }
            Device  = if ($eq -ge 0) { $_.Substring($eq + 1) } else { '(no device)' }
        }
    })
}

# Attribute each moved tensor to the rule that moved it. llama.cpp walks the rules
# in order and stops at the FIRST whose pattern matches (std::regex_search, i.e.
# unanchored — .NET's IsMatch is the same), so a later rule never gets a tensor an
# earlier one already claimed. Replicate that, or an overlapping pair of rules
# reads as if both fired.
#
# A rule with zero tensors is the failure mode this whole section exists for: an
# unknown DEVICE is loud (the child dies with 'unknown buffer type'), but a pattern
# that matches nothing is completely silent — it just quietly does nothing forever.
function Join-OverrideHits {
    param($Rules, $Hits)

    $claimed = @{}
    foreach ($rule in $Rules) {
        $mine = @()
        foreach ($hit in $Hits) {
            if ($claimed.ContainsKey($hit.Tensor)) { continue }
            $match = try { [regex]::IsMatch($hit.Tensor, $rule.Pattern) } catch { $false }
            if ($match) {
                $claimed[$hit.Tensor] = $true
                $mine += $hit
            }
        }

        [pscustomobject]@{
            Pattern  = $rule.Pattern
            Device   = $rule.Device
            Tensors  = $mine.Count
            MiB      = if ($mine) { ($mine | Measure-Object MiB -Sum).Sum } else { 0 }
            # What the buffer ACTUALLY ended up as, which is not always what was asked
            # for: =CPU resolves to CUDA_Host/ROCm_Host (pinned) or CPU_Mapped (mmap).
            Resolved = @($mine | Select-Object -ExpandProperty Buft -Unique)
            Names    = @($mine | Select-Object -ExpandProperty Tensor)
            Hits     = $mine
        }
    }
}

# The placement cell for one row of a balance sheet: what the --override-tensor rules
# put THERE. It is an 'of which', never an allocation of its own — an overridden tensor
# is moved INSIDE the model buffer of the device it landed on, so it is already counted
# in that device's 'model weights' row and in every total built from it. Giving it a row
# of its own and summing it would inflate 'requested' past what the card actually holds,
# and the whole report hangs on comparing that figure with 'free at load'.
#
# Hence: only the model-weights row can carry it, and only the main model's — llama.cpp
# names the tensor but not which load it came from, so a draft file's weights (same
# tensor names) are left out of the attribution rather than guessed at.
#
# Keyed on where each tensor LANDED (hit.Buft), not on the device the rule named: those
# differ whenever llama.cpp resolves the request to something else (=CPU becoming
# ROCm_Host is the standard case). With the hits unlogged (-lv < 5) there is nothing to
# count, so the cell names the rules that TARGET the device and says why the figure is
# missing — a rule aimed at a card whose weights never moved is the whole diagnosis.
function Get-Placement {
    param($Placed, $Row, [bool] $HitsLogged)

    if (-not $Placed)                                     { return $null }
    if ($Row.Owner -ne 'model' -or $Row.Kind -ne 'model')  { return $null }

    if (-not $HitsLogged) {
        $aimed = @($Placed | Where-Object { $_.Device -eq $Row.Device })
        if (-not $aimed) { return $null }
        return [pscustomobject]@{ Rules = @($aimed | ForEach-Object { $_.Pattern }); MiB = $null }
    }

    $here = @($Placed | ForEach-Object {
        $mine = @($_.Hits | Where-Object { $_.Buft -eq $Row.Device })
        if ($mine) { [pscustomobject]@{ Pattern = $_.Pattern; MiB = ($mine | Measure-Object MiB -Sum).Sum } }
    })
    if (-not $here) { return $null }

    # An 'of which' larger than the row it hangs off is not a rounding artefact, it is
    # the log contradicting itself: llama.cpp names the buffer at override time, and with
    # mmap on it then serves the tensor out of the mapped file instead, leaving the named
    # buffer empty. Say nothing here — Format-Ghosts explains it below the table rather
    # than printing '1,302 MiB of which' against a 0 MiB row. (Hit sizes are %zu, i.e.
    # truncated, so a real placement can undershoot the row but never overshoot it.)
    $sum = ($here | Measure-Object MiB -Sum).Sum
    if ($sum -gt $Row.MiB + 1) { return $null }

    [pscustomobject]@{ Rules = @($here | ForEach-Object { $_.Pattern }); MiB = $sum }
}

# The rows of one balance sheet, with an 'of which tensor override' sub-row folded in
# under the model weights it is part of. A sub-row, deliberately: it is a breakdown of
# the row above, not an allocation, so it must not be summed into anything.
function Expand-Rows {
    param($Rows, $Placed, [bool] $HitsLogged, [scriptblock] $Label)

    foreach ($r in ($Rows | Sort-Object MiB -Descending)) {
        [pscustomobject]@{ Allocation = (& $Label $r); MiB = '{0,9:N2}' -f $r.MiB }

        $p = Get-Placement -Placed $Placed -Row $r -HitsLogged $HitsLogged
        if ($p) {
            $which = if ($p.Rules.Count -eq 1) { $p.Rules[0] } else { "$($p.Rules.Count) rules" }
            [pscustomobject]@{
                Allocation = "  of which tensor override ($which)"
                MiB        = if ($null -ne $p.MiB) { '{0,9:N2}' -f $p.MiB } else { '        ? (-lv 5)' }
            }
        }
    }
}

# The rules whose tensors were logged into a buffer that never materialised (see above).
# Returns one line per (rule, buffer), or nothing — which is the normal case.
function Format-Ghosts {
    param($Placed, $Buffers, [bool] $HitsLogged)

    if (-not $HitsLogged) { return }

    foreach ($p in $Placed) {
        foreach ($g in @($p.Hits | Group-Object Buft)) {
            $row = @($Buffers | Where-Object { $_.Device -eq $g.Name -and $_.Owner -eq 'model' -and $_.Kind -eq 'model' })
            $rowMiB = if ($row) { ($row | Measure-Object MiB -Sum).Sum } else { 0 }
            $sum    = ($g.Group | Measure-Object MiB -Sum).Sum
            if ($sum -gt $rowMiB + 1) {
                '{0} -> {1:N0} MiB logged into {2}, but that buffer holds {3:N2} MiB: mmap is on, so llama.cpp served those tensors from the mapped file instead. --no-mmap if you meant to pin them.' -f $p.Pattern, $sum, $g.Name, $rowMiB
            }
        }
    }
}

# ------------------------------------------------------------ live counters ---

# Windows exposes GPU memory per (process, adapter LUID) but never names the
# adapter. nvidia-smi pins the NVIDIA one; the rest are matched by size against
# what the log says we asked for, which is unambiguous here (tens of GiB apart).
function Get-LiveGpuUsage {
    param([int[]] $ProcessIds)

    $samples = @()
    try {
        $counter = Get-Counter '\GPU Process Memory(*)\Dedicated Usage', '\GPU Process Memory(*)\Shared Usage' -ErrorAction Stop
        $samples = $counter.CounterSamples
    } catch {
        Write-Warning "GPU perf counters unavailable ($($_.Exception.Message)); reporting log figures only."
        return @()
    }

    $byLuid = @{}
    foreach ($s in $samples) {
        if ($s.CookedValue -le 0) { continue }
        if ($s.InstanceName -notmatch '^pid_(?<pid>\d+)_luid_(?<luid>0x[0-9a-f]+_0x[0-9a-f]+)') { continue }
        if ([int] $Matches.pid -notin $ProcessIds) { continue }

        $luid = $Matches.luid
        if (-not $byLuid.ContainsKey($luid)) { $byLuid[$luid] = @{ Dedicated = 0.0; Shared = 0.0 } }
        $kind = if ($s.Path -match 'dedicated') { 'Dedicated' } else { 'Shared' }
        $byLuid[$luid][$kind] += $s.CookedValue / 1MB
    }

    $byLuid.GetEnumerator() | ForEach-Object {
        [pscustomobject]@{
            Luid         = $_.Key
            DedicatedMiB = [math]::Round($_.Value.Dedicated, 0)
            SharedMiB    = [math]::Round($_.Value.Shared, 0)
            Device       = $null
        }
    } | Sort-Object DedicatedMiB -Descending
}

# ---------------------------------------------------------------------- main ---

$procs = @(Get-Process llama-server -ErrorAction SilentlyContinue)
if ($procs.Count -eq 0) { throw 'llama-server is not running.' }

$loaded = Get-LastLoadBlock -Path $LogPath
$load   = ConvertFrom-LoadBlock -Block $loaded.Lines
$live   = @(Get-LiveGpuUsage -ProcessIds $procs.Id)
$rules  = @(Get-OverrideRules -SpawnArgs $loaded.SpawnArgs)
$placed = @(Join-OverrideHits -Rules $rules -Hits $load.Hits)

# Per-device totals. Everything the CPU backend owns is RAM, not VRAM: it gets its
# own bucket, never charged to a GPU. Match '^CPU' with no anchor at the end — the
# plain 'CPU' backend is only one of its buffer types, and the others are NOT
# cosmetic variants:
#   CPU_Mapped  weights served straight out of the mmap'd file (the default!)
#   CPU_REPACK  weights re-tiled for the CPU kernels
#   *_Host      PINNED host memory owned by a GPU backend (ROCm_Host, CUDA_Host)
# Anchoring on '^CPU$' — as this did — leaves CPU_Mapped looking like a device, and
# with mmap on (i.e. unless --no-mmap) the entire CPU-side model lands there: the
# report would then invent a GPU named CPU_Mapped and try to match a LUID to it.
$isHost   = { param($d) $d -match '_Host$' -or $d -match '^CPU' }
$isPinned = { param($d) $d -match '_Host$' }

$deviceRows = @()
foreach ($dev in ($load.Buffers | Where-Object { -not (& $isHost $_.Device) } | Select-Object -ExpandProperty Device -Unique)) {
    $rows      = @($load.Buffers | Where-Object Device -eq $dev)
    $requested = ($rows | Measure-Object MiB -Sum).Sum
    $known     = if ($load.Devices.Contains($dev)) { $load.Devices[$dev] } else { $null }

    $deviceRows += [pscustomobject]@{
        Device       = $dev
        Desc         = if ($known) { $known.Desc } else { '(unknown)' }
        TotalMiB     = if ($known) { $known.TotalMiB } else { $null }
        FreeAtLoadMiB= if ($known) { $known.FreeMiB } else { $null }
        RequestedMiB = [math]::Round($requested, 0)
        Rows         = $rows
    }
}

# Match each LUID to a device by footprint: dedicated+shared should land within a
# few percent of what the log asked for. Anything further off is another adapter
# (an iGPU compositing the desktop, say) and stays unlabelled rather than guessed.
foreach ($l in $live) {
    $total = $l.DedicatedMiB + $l.SharedMiB
    $best  = $deviceRows |
        Where-Object { $_.RequestedMiB -gt 0 } |
        Sort-Object { [math]::Abs($_.RequestedMiB - $total) / $_.RequestedMiB } |
        Select-Object -First 1
    if ($best -and ([math]::Abs($best.RequestedMiB - $total) / $best.RequestedMiB) -le 0.15) {
        $l.Device = $best.Device
    }
}

$hostRows = @($load.Buffers | Where-Object { & $isHost $_.Device })
$hostMiB  = if ($hostRows) { ($hostRows | Measure-Object MiB -Sum).Sum } else { 0 }

# Model weights that did NOT make it onto a GPU, split by what kind of host memory
# they landed in — pinned (*_Host, owned by a GPU backend, charted by Windows as
# shared GPU memory) vs ordinary RAM (CPU, CPU_Mapped). The distinction is the whole
# point: only the pinned half explains a card showing GBs of shared allocation.
$modelHost    = @($hostRows | Where-Object { $_.Kind -eq 'model' })
$hostModelMiB = if ($modelHost)  { ($modelHost  | Measure-Object MiB -Sum).Sum } else { 0 }
$pinnedRows   = @($modelHost | Where-Object { & $isPinned $_.Device })
$pinnedMiB    = if ($pinnedRows) { ($pinnedRows | Measure-Object MiB -Sum).Sum } else { 0 }

# Below 5 the per-tensor override lines are not emitted at all, so 'no tensor moved'
# would be an artefact of the log level rather than a finding. Say which it is.
$hitsLogged = $load.Verbosity -ge 5

if ($Json) {
    [pscustomobject]@{
        Model     = $load.Model
        Arch      = $load.Arch
        NCtx      = $load.NCtx
        Verbosity = $load.Verbosity
        Buffers   = $load.Buffers
        Devices   = $deviceRows | Select-Object Device, Desc, TotalMiB, FreeAtLoadMiB, RequestedMiB
        Live      = $live
        HostMiB   = [math]::Round($hostMiB, 2)
        Placement = $placed | Select-Object Pattern, Device, Tensors, MiB, Resolved, Names
        Processes = $procs | Select-Object Id, StartTime, @{n='WorkingSetMiB';e={[math]::Round($_.WorkingSet64/1MB,0)}}
    } | ConvertTo-Json -Depth 6
    return
}

Write-Host ''
Write-Host "Model      : $($load.Model)" -ForegroundColor Cyan
Write-Host "Weights    : $($load.ParamsB) B params, $($load.FileType), $($load.FileSizeGiB) GiB on disk, arch $($load.Arch)"
Write-Host "Context    : n_ctx $($load.NCtx), n_seq_max $($load.NSeqMax), flash_attn $($load.FlashAttn), $($load.CacheTypes)"
Write-Host "Process    : PID $($procs.Id -join ', ') (started $(($procs | Sort-Object StartTime | Select-Object -First 1).StartTime))"

foreach ($d in ($deviceRows | Sort-Object RequestedMiB -Descending)) {
    Write-Host ''
    Write-Host "$($d.Device) — $($d.Desc)" -ForegroundColor Cyan

    $label = {
        param($r)
        switch ("$($r.Owner)/$($r.Kind)") {
            'model/model'   { 'model weights' }
            'model/KV'      { 'KV cache' }
            'model/KV-SWA'  { 'KV cache (sliding window)' }
            'model/RS'      { 'recurrent state' }
            'model/compute' { 'compute buffer' }
            'model/LoRA'    { 'LoRA adapter' }
            'draft/KV'      { 'draft KV cache (MTP)' }
            'draft/KV-SWA'  { 'draft KV cache (sliding window)' }
            'draft/compute' { 'draft compute buffer' }
            'mmproj/model'  { 'mmproj weights' }
            'mmproj/compute'{ 'mmproj compute buffer' }
            default         { "$($r.Owner) $($r.Kind)" }
        }
    }

    Expand-Rows -Rows $d.Rows -Placed $placed -HitsLogged $hitsLogged -Label $label |
        Format-Table -AutoSize | Out-String | Write-Host -NoNewline

    $liveRow = $live | Where-Object Device -eq $d.Device | Select-Object -First 1
    Write-Host ("  requested {0,9:N0} MiB   |   card {1:N0} MiB total, {2:N0} MiB free at load" -f $d.RequestedMiB, $d.TotalMiB, $d.FreeAtLoadMiB)
    if ($liveRow) {
        Write-Host ("  live      {0,9:N0} MiB dedicated + {1:N0} MiB shared (system RAM)" -f $liveRow.DedicatedMiB, $liveRow.SharedMiB)
    }

    # The diagnosis: asked for more than the card had free, so WDDM paged the rest
    # out to system memory and every access to it now crosses PCIe. Note the log
    # never accounts for the driver's own context (a few hundred MiB per backend),
    # so 'requested' just under 'free at load' can still spill — hence the second
    # arm, which trusts the live counter over the arithmetic. A few hundred MiB of
    # shared is normal driver staging on both backends; only flag a real overflow.
    if ($d.FreeAtLoadMiB -and $d.RequestedMiB -gt $d.FreeAtLoadMiB) {
        $over = $d.RequestedMiB - $d.FreeAtLoadMiB
        Write-Host ("  OVERSUBSCRIBED by {0:N0} MiB — the excess lives in shared system memory, not VRAM." -f $over) -ForegroundColor Red
    } elseif ($liveRow -and $liveRow.SharedMiB -gt 1024) {
        Write-Host ("  {0:N0} MiB in shared system memory though the log fits — driver context overhead pushed it over." -f $liveRow.SharedMiB) -ForegroundColor Yellow
    }
}

if ($hostRows) {
    Write-Host ''
    Write-Host 'Host RAM (not VRAM)' -ForegroundColor Cyan
    $hostLabel = { param($r) "$($r.Device) $($r.Owner) $(if ($r.Kind -eq 'model') { 'weights' } else { $r.Kind })" }

    Expand-Rows -Rows $hostRows -Placed $placed -HitsLogged $hitsLogged -Label $hostLabel |
        Format-Table -AutoSize | Out-String | Write-Host -NoNewline
    Write-Host ("  total     {0,9:N2} MiB" -f $hostMiB)

    Write-Host '  *_Host is PINNED host memory owned by a GPU backend — Windows charts it under the card, as'
    Write-Host '  shared GPU memory. CPU / CPU_Mapped is ordinary RAM and shows up nowhere on the GPU.'

    if ($pinnedMiB -gt 0) {
        # The one everybody hits: 'offloaded N/N layers to GPU' and yet GBs of shared
        # memory. token_embd.weight is parked in a host buffer BY DESIGN (an embedding
        # lookup is a get_rows over a handful of tokens — cheap on the CPU, and it saves
        # VRAM), and with a GPU backend loaded that buffer is pinned. Nothing overflowed.
        #
        # Whether an =CPU rule PUT it there is read off the pinned buffer actually
        # existing, never off the override line alone: with mmap on, llama.cpp logs
        # 'overridden to CUDA_Host' and then serves the tensor straight out of the mapped
        # file (CPU_Mapped) — same debug line, ordinary RAM after all.
        $cpuToPinned = @($placed | Where-Object {
            $hitsLogged -and $_.Device -match '^CPU' -and (@($_.Resolved) -match '_Host$')
        })
        $target = ($deviceRows | Sort-Object RequestedMiB -Descending | Select-Object -First 1)

        Write-Host ''
        Write-Host ("  {0:N2} MiB of MODEL WEIGHTS are pinned ({1}) — that is your shared GPU memory, and it is" -f $pinnedMiB, (($pinnedRows | Select-Object -ExpandProperty Device -Unique) -join ', ')) -ForegroundColor Yellow
        Write-Host '  by design, not an overflow: llama.cpp parks the embedding table in a host buffer even at'
        Write-Host '  N/N layers offloaded.'
        if ($cpuToPinned) {
            Write-Host ("  That is where your =CPU rule put it ({0}). Aim it at a GPU device instead: the VRAM is" -f (($cpuToPinned | ForEach-Object { $_.Pattern }) -join ', '))
            Write-Host '  freed either way, but =CPU leaves the allocation in the shared bucket.'
        } elseif ($rules) {
            Write-Host '  No --override-tensor rule of yours claimed it — check the patterns against the names above.'
        } elseif ($target) {
            Write-Host ("  Reclaim it with a Tensor placement rule:  token_embd\.weight={0}" -f $target.Device)
        }
    }
}

# The two placement outcomes that hang off no row at all, so they get the only lines
# printed outside the balance sheets.
#
# A rule that matched NOTHING is the failure mode worth shouting about: a bad device
# name is loud (the server refuses to start with 'unknown buffer type'), while a
# pattern that matches nothing is perfectly silent and stays silent forever.
foreach ($p in @($placed | Where-Object { $hitsLogged -and $_.Tensors -eq 0 })) {
    Write-Host ''
    Write-Host ("DEAD RULE — '{0}={1}' matched no tensor at all." -f $p.Pattern, $p.Device) -ForegroundColor Red
    Write-Host 'A bad device name stops the server; a pattern that matches nothing just quietly does nothing.'
}

# And a rule whose tensors were logged into a buffer that never materialised.
foreach ($g in @(Format-Ghosts -Placed $placed -Buffers $load.Buffers -HitsLogged $hitsLogged)) {
    Write-Host ''
    Write-Host $g -ForegroundColor Yellow
}

$unlabelled = @($live | Where-Object { -not $_.Device })
if ($unlabelled) {
    Write-Host ''
    Write-Host 'Other adapters holding memory for this process' -ForegroundColor DarkGray
    $unlabelled | Format-Table -AutoSize Luid, DedicatedMiB, SharedMiB | Out-String | Write-Host -NoNewline
}

Write-Host ''
