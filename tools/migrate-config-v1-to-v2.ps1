<#
.SYNOPSIS
    Migrates a llama-cpp-config 1.x configuration to the 2.0 layout, offline.

.DESCRIPTION
    2.0 moved these knobs from server.ini to the presets, because set server-wide
    they rode the router's command line and overrode every preset's own value:

      server.ini LoadMode (or the older Mlock / NoMmap)  -> preset load-mode
      server.ini Device + TensorSplit                    -> preset device + tensor-split
      server.ini OverrideTensor                          -> preset override-tensor
      server.ini SplitMode                               -> preset split-mode
      server.ini Fit                                     -> preset fit
      server.ini PrefillAssistant                        -> preset prefill-assistant
      server.ini CacheReuse                              -> preset cache-reuse
      server.ini MmprojDevice                            -> preset mmproj-device

    2.0 ignores those server.ini keys, so without this a model silently changes
    how it loads. The rewrite reproduces what 1.x actually launched:

      load-mode      1.x always passed -lm (auto when nothing was set), so a mode
                     other than auto is written into EVERY preset, replacing the
                     preset's own; under auto only a preset key that disagrees is
                     rewritten (to auto), since an absent key is auto already.
                     Skipped when server.ini has none of the three keys.
      device         a server-wide list won over the preset's, and took the
      tensor-split   split with it: written into every preset, and the preset's
                     own tensor-split is dropped when the server had none (its
                     weights were positional over a device list no longer its).
                     A TensorSplit without a Device is written alone.
      override-tensor  server-wide rules REPLACED a preset's: written into
                     every preset, overwriting its own.
      split-mode     a server-wide none/tensor/row won over the preset, so it is
                     written into every preset; layer or blank was never on the
                     command line and changes nothing.
      fit            1.x always passed -fit (off unless Fit = true): written
                     into every preset as on|off, replacing the preset's own.
      prefill-assistant  a server-wide false (--no-prefill-assistant) applied to
                     every model: written as false into every preset; true was
                     llama.cpp's default and changes nothing.
      cache-reuse    a server-wide value above 0 applied to every model: written
                     into every preset, replacing the preset's own.
      mmproj-device  rode the env MTMD_BACKEND_DEVICE, which a preset's own key
                     beats: written only into presets that have an mmproj and no
                     mmproj-device of their own.

    Both presets.ini and presets.ini.disabled are rewritten, then the keys (and
    their comment blocks) are deleted from server.ini. Each file is backed up
    beside itself as <file>.v1.bak before it is touched. Running it twice is
    harmless: with none of those keys left in server.ini it changes nothing.

    Close llama-cpp-config first (a Save from an open window would write the old
    values back over the presets). llama-server reads the files only when it
    starts, so restart it afterwards.

    The Linux twin is migrate-config-v1-to-v2.sh; keep the two in step.

.PARAMETER DryRun
    Show what would change, write nothing.

.PARAMETER ConfigDir
    The config directory. Default: %LOCALAPPDATA%\llama.cpp\config.
#>
[CmdletBinding()]
param(
    [switch]$DryRun,
    [string]$ConfigDir = (Join-Path $env:LOCALAPPDATA 'llama.cpp\config')
)
$ErrorActionPreference = 'Stop'

$utf8 = New-Object System.Text.UTF8Encoding($false)

# A file as lines without their endings, plus the ending it uses (ini.rs keeps
# a file's own; CRLF is what the configurator writes on Windows).
function Read-Lines([string]$Path) {
    $text = [IO.File]::ReadAllText($Path, $utf8)
    $eol = if ($text.Contains("`r`n")) { "`r`n" } else { "`n" }
    $lines = [Collections.Generic.List[string]]::new()
    foreach ($l in ($text -split "`r?`n")) { $lines.Add($l) }
    # A trailing newline leaves one empty element: not a line.
    if ($lines.Count -gt 0 -and $lines[$lines.Count - 1] -eq '') { $lines.RemoveAt($lines.Count - 1) }
    [pscustomobject]@{ Text = $text; Lines = $lines; Eol = $eol }
}

# Writes $Lines over $Path after backing it up; in a dry run lists the changed
# lines instead. A file that comes out identical is left alone.
function Save-Lines([string]$Path, $Orig, [Collections.Generic.List[string]]$Lines) {
    if ($Lines.Count -eq 0 -and $Orig.Lines.Count -eq 0) { return $false }
    $new = ($Lines -join $Orig.Eol) + $Orig.Eol
    if ($new -ceq $Orig.Text) { return $false }
    if ($DryRun) {
        Compare-Object -CaseSensitive @($Orig.Lines) @($Lines) | ForEach-Object {
            $mark = if ($_.SideIndicator -eq '=>') { '+' } else { '-' }
            Write-Host "    $mark $($_.InputObject)"
        }
    } else {
        $bak = "$Path.v1.bak"
        if (Test-Path -LiteralPath $bak) { $bak = "$Path.v1.$(Get-Date -Format 'yyyyMMdd-HHmmss').bak" }
        Copy-Item -LiteralPath $Path -Destination $bak
        [IO.File]::WriteAllText($Path, $new, $utf8)
        Write-Host "  wrote $Path (backup: $bak)"
    }
    return $true
}

# key = value as ini.rs reads it: comment lines skipped, key trimmed, value cut
# at the first ; or # and trimmed. $null for anything else.
function Split-KeyLine([string]$Line) {
    $t = $Line.Trim()
    if ($t -eq '' -or $t.StartsWith(';') -or $t.StartsWith('#')) { return $null }
    $eq = $t.IndexOf('=')
    if ($eq -lt 0) { return $null }
    $v = $t.Substring($eq + 1)
    $c = $v.IndexOfAny([char[]]';#')
    if ($c -ge 0) { $v = $v.Substring(0, $c) }
    [pscustomobject]@{ Key = $t.Substring(0, $eq).Trim(); Value = $v.Trim() }
}

function Test-Header([string]$Line) { $Line.Trim() -match '^\[.*\]$' }

$serverIni = Join-Path $ConfigDir 'server.ini'
if (-not (Test-Path -LiteralPath $serverIni)) {
    Write-Host "No ${serverIni}: nothing to migrate."
    exit 0
}
Write-Host "Config: $ConfigDir"

# ── Read the [Server] keys 1.x launched with ─────────────────────────────
# Exact-case keys, case-insensitive section (ini.rs read_all).
$server = Read-Lines $serverIni
# Ordinal: a PowerShell @{} would also match `loadmode`, which ini.rs does not.
$keys = New-Object Collections.Hashtable ([StringComparer]::Ordinal)
$inServer = $false
foreach ($l in $server.Lines) {
    if (Test-Header $l) { $inServer = ($l.Trim() -replace '^\[\s*|\s*\]$', '') -ieq 'server'; continue }
    if (-not $inServer) { continue }
    $kv = Split-KeyLine $l
    if ($kv) { $keys[$kv.Key] = $kv.Value }
}
$moved = 'LoadMode', 'Mlock', 'NoMmap', 'Device', 'TensorSplit', 'OverrideTensor', 'SplitMode', 'Fit', 'PrefillAssistant', 'CacheReuse', 'MmprojDevice'

# No moved key left (a 2.0 file, or a second run): the presets already carry
# the launch, and reading the absence as 1.x's auto would rewrite them.
if (-not ($moved | Where-Object { $keys.ContainsKey($_) })) {
    Write-Host "$serverIni has no 1.x key left: already migrated, nothing to do."
    exit 0
}

# LoadMode as server_cfg::normalize_load_mode, else the legacy pair
# (migrated_load_mode), else the 1.x default.
$loadMode = 'auto', 'none', 'mmap', 'mlock', 'mmap+mlock', 'dio' |
    Where-Object { $_ -ieq "$($keys['LoadMode'])" } | Select-Object -First 1
if (-not ('LoadMode', 'Mlock', 'NoMmap' | Where-Object { $keys.ContainsKey($_) })) {
    # None of the three keys (a file already past that step): nothing to apply.
    $loadMode = ''
} elseif (-not $loadMode) {
    $loadMode = if ($keys['NoMmap'] -ceq 'true' -and $keys['Mlock'] -ceq 'true') { 'mlock' }
                elseif ($keys['NoMmap'] -ceq 'true') { 'none' }
                else { 'auto' }
}
# A server-wide layer (or "default") was dropped on read in 1.x.
$splitMode = "$($keys['SplitMode'])"
if ($splitMode -in '', 'layer', 'default') { $splitMode = '' }
$mmprojDevice = "$($keys['MmprojDevice'])"
$device = "$($keys['Device'])"
$tensorSplit = "$($keys['TensorSplit'])"
$overrideTensor = "$($keys['OverrideTensor'])"
$fit = if ($keys['Fit'] -ceq 'true') { 'on' } else { 'off' }
$prefill = if ($keys['PrefillAssistant'] -ceq 'false') { 'false' } else { '' }
$cacheReuse = if ("$($keys['CacheReuse'])" -match '^\d+$' -and [int]$keys['CacheReuse'] -gt 0) { "$($keys['CacheReuse'])" } else { '' }

$splitShown = if ($splitMode) { $splitMode } else { "(each preset's own)" }
$deviceShown = if ($mmprojDevice) { $mmprojDevice } else { "(llama.cpp's default)" }
$devicesShown = if ($device) { $device } else { "(each preset's own)" }
$tsShown = if ($tensorSplit) { $tensorSplit } else { "(each preset's own)" }
$otShown = if ($overrideTensor) { $overrideTensor } else { "(each preset's own)" }
$lmShown = if ($loadMode) { $loadMode } else { "(each preset's own)" }
$paShown = if ($prefill) { $prefill } else { "(llama.cpp's default)" }
$crShown = if ($cacheReuse) { $cacheReuse } else { "(each preset's own)" }
Write-Host "1.x launched with: load mode $lmShown; fit $fit; prefill assistant $paShown; cache reuse $crShown; devices $devicesShown; tensor split $tsShown; tensor placement $otShown; split mode $splitShown; vision device $deviceShown"

# ── Presets ──────────────────────────────────────────────────────────────
function Update-Presets([string]$Path) {
    $f = Read-Lines $Path
    $out = [Collections.Generic.List[string]]::new()
    $lines = $f.Lines
    $i = 0
    while ($i -lt $lines.Count) {
        if (-not (Test-Header $lines[$i])) { $out.Add($lines[$i]); $i++; continue }
        # One section: header at $i, body up to the next header.
        $end = $i + 1
        while ($end -lt $lines.Count -and -not (Test-Header $lines[$end])) { $end++ }
        $id = $lines[$i].Trim() -replace '^\[\s*|\s*\]$', ''
        $body = [Collections.Generic.List[string]]::new()
        for ($j = $i; $j -lt $end; $j++) { $body.Add($lines[$j]) }

        $last = 0; $at = @{}; $hasMmproj = $false
        for ($j = 1; $j -lt $body.Count; $j++) {
            $kv = Split-KeyLine $body[$j]
            if (-not $kv) { continue }
            $last = $j
            if ($kv.Key -cin 'load-mode', 'device', 'tensor-split', 'override-tensor', 'split-mode', 'fit', 'prefill-assistant', 'cache-reuse', 'mmproj-device') { $at[$kv.Key] = $j }
            if ($kv.Key -ceq 'mmproj' -and $kv.Value) { $hasMmproj = $true }
        }
        $adds = [Collections.Generic.List[string]]::new()
        $gone = @{}
        $set = {
            param($k, $v)
            if ($at.ContainsKey($k)) {
                $old = (Split-KeyLine $body[$at[$k]]).Value
                if ($old -ceq $v) { return }
                Write-Host "  [$id] ${k}: $old -> $v"
                $body[$at[$k]] = "$k = $v"
            } else {
                Write-Host "  [$id] ${k}: (unset) -> $v"
                $adds.Add("$k = $v")
            }
        }
        $lmNow = if ($at.ContainsKey('load-mode')) { (Split-KeyLine $body[$at['load-mode']]).Value } else { $null }
        if ($loadMode -and ($loadMode -ne 'auto' -or ($null -ne $lmNow -and $lmNow -cne 'auto'))) { & $set 'load-mode' $loadMode }
        if ($device) { & $set 'device' $device }
        if ($tensorSplit) {
            & $set 'tensor-split' $tensorSplit
        } elseif ($device -and $at.ContainsKey('tensor-split')) {
            Write-Host "  [$id] tensor-split: $((Split-KeyLine $body[$at['tensor-split']]).Value) -> (unset)"
            $gone[$at['tensor-split']] = $true
        }
        if ($overrideTensor) { & $set 'override-tensor' $overrideTensor }
        if ($splitMode) { & $set 'split-mode' $splitMode }
        & $set 'fit' $fit
        if ($prefill) { & $set 'prefill-assistant' $prefill }
        if ($cacheReuse) { & $set 'cache-reuse' $cacheReuse }
        if ($mmprojDevice -and $hasMmproj -and -not $at.ContainsKey('mmproj-device')) { & $set 'mmproj-device' $mmprojDevice }

        # New keys go right after the section's last key line.
        for ($j = 0; $j -lt $body.Count; $j++) {
            if (-not $gone.ContainsKey($j)) { $out.Add($body[$j]) }
            if ($j -eq $last) { foreach ($a in $adds) { $out.Add($a) } }
        }
        $i = $end
    }
    if (-not (Save-Lines $Path $f $out)) { Write-Host "  ${Path}: nothing to change" }
}

foreach ($name in 'presets.ini', 'presets.ini.disabled') {
    $p = Join-Path $ConfigDir $name
    if (Test-Path -LiteralPath $p) {
        Write-Host "${p}:"
        Update-Presets $p
    }
}

# ── server.ini: drop the moved keys ──────────────────────────────────────
# The key line, its commented-out placeholder (`; SplitMode = layer  ; ...`),
# and the comment block right above either, up to a blank line or another
# key's placeholder. The configurator rewrites the file's comments on its next
# save anyway; this only keeps the file from describing keys nothing reads.
function Get-CommentKey([string]$Line) {
    if ($Line -match '^\s*[;#]?\s*([A-Za-z]+)\s*=') { return $Matches[1] }
    return ''
}
$lines = $server.Lines
$drop = New-Object bool[] $lines.Count
for ($i = 0; $i -lt $lines.Count; $i++) {
    if ((Get-CommentKey $lines[$i]) -cnotin $moved) { continue }
    $drop[$i] = $true
    for ($j = $i - 1; $j -ge 0 -and $lines[$j] -match '^\s*[;#]' -and (Get-CommentKey $lines[$j]) -eq ''; $j--) {
        $drop[$j] = $true
    }
}
$kept = [Collections.Generic.List[string]]::new()
for ($i = 0; $i -lt $lines.Count; $i++) { if (-not $drop[$i]) { $kept.Add($lines[$i]) } }
Write-Host "${serverIni}:"
if (-not (Save-Lines $serverIni $server $kept)) { Write-Host '  nothing to remove' }

if ($DryRun) {
    Write-Host 'Dry run: nothing was written.'
} else {
    Write-Host 'Done. Restart llama-server to launch with the migrated presets.'
}
