#!/usr/bin/env bash
# Migrates a llama-cpp-config 1.x configuration to the 2.0 layout, offline.
#
# 2.0 moved these knobs from server.ini to the presets, because set server-wide
# they rode the router's command line and overrode every preset's own value:
#
#   server.ini LoadMode (or the older Mlock / NoMmap)  -> preset load-mode
#   server.ini Device + TensorSplit                    -> preset device + tensor-split
#   server.ini OverrideTensor                          -> preset override-tensor
#   server.ini SplitMode                               -> preset split-mode
#   server.ini Fit                                     -> preset fit
#   server.ini PrefillAssistant                        -> preset prefill-assistant
#   server.ini CacheReuse                              -> preset cache-reuse
#   server.ini MmprojDevice                            -> preset mmproj-device
#
# 2.0 ignores those server.ini keys, so without this a model silently changes
# how it loads. The rewrite reproduces what 1.x actually launched:
#
#   load-mode      1.x always passed -lm (auto when nothing was set), so a mode
#                  other than auto is written into EVERY preset, replacing the
#                  preset's own; under auto only a preset key that disagrees is
#                  rewritten (to auto), since an absent key is auto already.
#                  Skipped when server.ini has none of the three keys.
#   device         a server-wide list won over the preset's, and took the
#   tensor-split   split with it: written into every preset, and the preset's
#                  own tensor-split is dropped when the server had none (its
#                  weights were positional over a device list no longer its).
#                  A TensorSplit without a Device is written alone.
#   override-tensor  server-wide rules REPLACED a preset's: written into
#                  every preset, overwriting its own.
#   split-mode     a server-wide none/tensor/row won over the preset, so it is
#                  written into every preset; layer or blank was never on the
#                  command line and changes nothing.
#   fit            1.x always passed -fit (off unless Fit = true): written
#                  into every preset as on|off, replacing the preset's own.
#   prefill-assistant  a server-wide false (--no-prefill-assistant) applied to
#                  every model: written as false into every preset; true was
#                  llama.cpp's default and changes nothing.
#   cache-reuse    a server-wide value above 0 applied to every model: written
#                  into every preset, replacing the preset's own.
#   mmproj-device  rode the env MTMD_BACKEND_DEVICE, which a preset's own key
#                  beats: written only into presets that have an mmproj and no
#                  mmproj-device of their own.
#
# Both presets.ini and presets.ini.disabled are rewritten, then the keys (and
# their comment blocks) are deleted from server.ini. Each file is backed up
# beside itself as <file>.v1.bak before it is touched. Running it twice is
# harmless: with none of those keys left in server.ini it changes nothing.
#
# Linux only: a config still in /etc/llama.cpp (the pre-2.0 location) is first
# copied to ~/.config/llama.cpp, and settings.ini's BenchPromptFile follows it.
#
# Close llama-cpp-config first (a Save from an open window would write the old
# values back over the presets). llama-server reads the files only when it
# starts, so restart it afterwards.
#
# Usage: tools/migrate-config-v1-to-v2.sh [--dry-run] [--config-dir DIR]
set -euo pipefail

dry_run=0
config_dir=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run) dry_run=1 ;;
        --config-dir) config_dir="${2:?--config-dir needs a directory}"; shift ;;
        -h|--help) sed -n '2,/^set -euo/p' "$0" | sed '$d; s/^# \{0,1\}//'; exit 0 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
    shift
done

# paths::config_dir: $XDG_CONFIG_HOME/llama.cpp when that is absolute.
if [[ -z "$config_dir" ]]; then
    if [[ "${XDG_CONFIG_HOME:-}" == /* ]]; then
        config_dir="$XDG_CONFIG_HOME/llama.cpp"
    else
        config_dir="$HOME/.config/llama.cpp"
    fi
fi

say() { printf '%s\n' "$*"; }
act() { if (( dry_run )); then say "  (dry run) $*"; else say "  $*"; fi; }

# Writes $2 (a temp file) over $1 after backing $1 up; in a dry run shows the
# diff instead. A file that comes out identical is left alone.
commit() {
    local file="$1" new="$2"
    if cmp -s "$file" "$new"; then
        rm -f "$new"
        return 1
    fi
    if (( dry_run )); then
        diff -u --label "$file" --label "$file (migrated)" "$file" "$new" | sed 's/^/    /' || true
        rm -f "$new"
    else
        local bak="$file.v1.bak"
        [[ -e "$bak" ]] && bak="$file.v1.$(date +%Y%m%d-%H%M%S).bak"
        cp -p "$file" "$bak"
        cat "$new" > "$file"
        rm -f "$new"
        say "  wrote $file (backup: $bak)"
    fi
}

# ── /etc/llama.cpp -> ~/.config/llama.cpp ────────────────────────────────
old_dir=/etc/llama.cpp
if [[ -d "$old_dir" && ! -e "$config_dir/server.ini" && ! -e "$config_dir/presets.ini" ]]; then
    say "Config found in $old_dir, none in $config_dir:"
    (( dry_run )) || mkdir -p "$config_dir"
    for f in server.ini presets.ini presets.ini.disabled settings.ini bench-prompt.txt bench-prompt-long.txt; do
        if [[ -f "$old_dir/$f" && ! -e "$config_dir/$f" ]]; then
            act "copy $old_dir/$f -> $config_dir/$f"
            (( dry_run )) || cp "$old_dir/$f" "$config_dir/$f"
        fi
    done
    if (( ! dry_run )) && [[ -f "$config_dir/settings.ini" ]]; then
        tmp="$(mktemp)"
        sed -E "s#^([[:space:]]*BenchPromptFile[[:space:]]*=[[:space:]]*)$old_dir/#\\1$config_dir/#" \
            "$config_dir/settings.ini" > "$tmp"
        commit "$config_dir/settings.ini" "$tmp" || true
    fi
fi

server_ini="$config_dir/server.ini"
if [[ ! -f "$server_ini" ]]; then
    say "No $server_ini: nothing to migrate."
    exit 0
fi
say "Config: $config_dir"

# ── Read the [Server] keys 1.x launched with ─────────────────────────────
# Exact-case keys, case-insensitive section, value trimmed and cut at the
# first ; or # (ini.rs read_all).
server_key() {
    awk -v want="$1" '
        { sub(/\r$/, "") }
        /^[ \t]*\[.*\][ \t]*$/ { s = $0; gsub(/[ \t\[\]]/, "", s); in_srv = (tolower(s) == "server"); next }
        /^[ \t]*[;#]/ { next }
        in_srv && index($0, "=") {
            k = substr($0, 1, index($0, "=") - 1); gsub(/^[ \t]+|[ \t]+$/, "", k)
            if (k != want) next
            v = substr($0, index($0, "=") + 1); sub(/[;#].*/, "", v); gsub(/^[ \t]+|[ \t]+$/, "", v)
            val = v; found = 1
        }
        END { if (found) print val }
    ' "$server_ini"
}

# No moved key left (a 2.0 file, or a second run): the presets already carry
# the launch, and reading the absence as 1.x's auto would rewrite them.
if ! grep -Eq '^[[:space:]]*(LoadMode|Mlock|NoMmap|Device|TensorSplit|OverrideTensor|SplitMode|Fit|PrefillAssistant|CacheReuse|MmprojDevice)[[:space:]]*=' "$server_ini"; then
    say "$server_ini has no 1.x key left: already migrated, nothing to do."
    exit 0
fi

raw_load="$(server_key LoadMode)"
raw_mlock="$(server_key Mlock)"
raw_nommap="$(server_key NoMmap)"
raw_split="$(server_key SplitMode)"
raw_mmdev="$(server_key MmprojDevice)"
raw_device="$(server_key Device)"
raw_ts="$(server_key TensorSplit)"
raw_ot="$(server_key OverrideTensor)"
fit=off
[[ "$(server_key Fit)" == "true" ]] && fit=on
prefill=""
[[ "$(server_key PrefillAssistant)" == "false" ]] && prefill=false
cache_reuse="$(server_key CacheReuse)"
[[ "$cache_reuse" =~ ^[0-9]+$ && "$cache_reuse" -gt 0 ]] || cache_reuse=""

# LoadMode as server_cfg::normalize_load_mode, else the legacy pair
# (migrated_load_mode), else the 1.x default.
# None of the three keys (a file already past that step): nothing to apply.
load_mode=""
for m in auto none mmap mlock mmap+mlock dio; do
    [[ "${raw_load,,}" == "$m" ]] && load_mode="$m"
done
if ! grep -Eq '^[[:space:]]*(LoadMode|Mlock|NoMmap)[[:space:]]*=' "$server_ini"; then
    load_mode=""
elif [[ -z "$load_mode" ]]; then
    if [[ "$raw_nommap" == "true" && "$raw_mlock" == "true" ]]; then
        load_mode=mlock
    elif [[ "$raw_nommap" == "true" ]]; then
        load_mode=none
    else
        load_mode=auto
    fi
fi

# A server-wide layer (or "default") was dropped on read in 1.x.
split_mode="$raw_split"
case "${split_mode,,}" in ""|layer|default) split_mode="" ;; esac

say "1.x launched with: load mode ${load_mode:-"(each preset's own)"}; fit $fit; prefill assistant ${prefill:-"(llama.cpp's default)"}; cache reuse ${cache_reuse:-"(each preset's own)"}; devices ${raw_device:-"(each preset's own)"}; tensor split ${raw_ts:-"(each preset's own)"}; tensor placement ${raw_ot:-"(each preset's own)"}; split mode ${split_mode:-"(each preset's own)"}; vision device ${raw_mmdev:-"(llama.cpp's default)"}"

# ── Presets ──────────────────────────────────────────────────────────────
migrate_presets() {
    local file="$1" tmp
    tmp="$(mktemp)"
    # OT rides the environment, not -v: awk expands backslash escapes in a -v
    # value, and a tensor regex is full of them (`token_embd\.weight`).
    MIGRATE_OT="$raw_ot" awk -v LM="$load_mode" -v SM="$split_mode" -v MD="$raw_mmdev" -v DEV="$raw_device" -v TS="$raw_ts" -v FIT="$fit" -v PA="$prefill" -v CRU="$cache_reuse" '
        function val(line,   v) {
            v = substr(line, index(line, "=") + 1); sub(/[;#].*/, "", v)
            gsub(/^[ \t]+|[ \t]+$/, "", v); return v
        }
        function key(line,   k) {
            if (line ~ /^[ \t]*[;#]/ || !index(line, "=")) return ""
            k = substr(line, 1, index(line, "=") - 1); gsub(/^[ \t]+|[ \t]+$/, "", k); return k
        }
        # One section: lines first..last of L[]. Edits go into L[], insertions
        # into ADD[after-line], both printed at the end.
        function section(first, last,   i, k, id, at, lm_i, sm_i, md_i, dv_i, ts_i, ot_i, fi_i, pa_i, cr_i, has_mm) {
            if (first < 1) return
            id = L[first]; gsub(/^[ \t]*\[|\][ \t]*$/, "", id)
            at = first; lm_i = sm_i = md_i = dv_i = ts_i = ot_i = fi_i = pa_i = cr_i = 0; has_mm = 0
            for (i = first + 1; i <= last; i++) {
                k = key(L[i]); if (k == "") continue
                at = i
                if (k == "load-mode") lm_i = i
                else if (k == "device") dv_i = i
                else if (k == "tensor-split") ts_i = i
                else if (k == "override-tensor") ot_i = i
                else if (k == "split-mode") sm_i = i
                else if (k == "fit") fi_i = i
                else if (k == "prefill-assistant") pa_i = i
                else if (k == "cache-reuse") cr_i = i
                else if (k == "mmproj-device") md_i = i
                else if (k == "mmproj" && val(L[i]) != "") has_mm = 1
            }
            if (LM != "" && (LM != "auto" || (lm_i && val(L[lm_i]) != "auto"))) set(id, at, lm_i, "load-mode", LM)
            if (DEV != "") set(id, at, dv_i, "device", DEV)
            if (TS != "") set(id, at, ts_i, "tensor-split", TS)
            else if (DEV != "" && ts_i) drop(id, ts_i, "tensor-split")
            if (ENVIRON["MIGRATE_OT"] != "") set(id, at, ot_i, "override-tensor", ENVIRON["MIGRATE_OT"])
            if (SM != "") set(id, at, sm_i, "split-mode", SM)
            set(id, at, fi_i, "fit", FIT)
            if (PA != "") set(id, at, pa_i, "prefill-assistant", PA)
            if (CRU != "") set(id, at, cr_i, "cache-reuse", CRU)
            if (MD != "" && has_mm && !md_i) set(id, at, 0, "mmproj-device", MD)
        }
        function drop(id, i, k) {
            printf "  [%s] %s: %s -> (unset)\n", id, k, val(L[i]) > "/dev/stderr"
            GONE[i] = 1
        }
        function set(id, at, i, k, v) {
            if (i) {
                if (val(L[i]) == v) return
                printf "  [%s] %s: %s -> %s\n", id, k, val(L[i]), v > "/dev/stderr"
                L[i] = k " = " v
            } else {
                printf "  [%s] %s: (unset) -> %s\n", id, k, v > "/dev/stderr"
                ADD[at] = ADD[at] k " = " v EOL
            }
        }
        {
            if (NR == 1) EOL = /\r$/ ? "\r\n" : "\n"
            CR[NR] = /\r$/; sub(/\r$/, ""); L[NR] = $0
            if ($0 ~ /^[ \t]*\[.*\][ \t]*$/) { section(start, NR - 1); start = NR }
        }
        END {
            section(start, NR)
            for (i = 1; i <= NR; i++) {
                if (!(i in GONE)) printf "%s%s", L[i], (CR[i] ? "\r\n" : "\n")
                if (i in ADD) printf "%s", ADD[i]
            }
        }
    ' "$file" > "$tmp"
    commit "$file" "$tmp" || say "  $file: nothing to change"
}

for f in "$config_dir/presets.ini" "$config_dir/presets.ini.disabled"; do
    if [[ -f "$f" ]]; then
        say "$f:"
        migrate_presets "$f"
    fi
done

# ── server.ini: drop the moved keys ──────────────────────────────────────
# The key line, its commented-out placeholder (`; SplitMode = layer  ; ...`),
# and the comment block right above either, up to a blank line or another
# key's placeholder. The configurator rewrites the file's comments on its next
# save anyway; this only keeps the file from describing keys nothing reads.
say "$server_ini:"
tmp="$(mktemp)"
awk '
    BEGIN { split("LoadMode Mlock NoMmap Device TensorSplit OverrideTensor SplitMode Fit PrefillAssistant CacheReuse MmprojDevice", K, " "); for (i in K) MOVED[K[i]] = 1 }
    function ckey(line,   k) {
        k = line; sub(/^[ \t]*[;#]?[ \t]*/, "", k)
        if (!match(k, /^[A-Za-z]+[ \t]*=/)) return ""
        k = substr(k, 1, RLENGTH); sub(/[ \t]*=$/, "", k); return k
    }
    { L[NR] = $0; t = $0; sub(/\r$/, "", t); T[NR] = t }
    END {
        for (i = 1; i <= NR; i++) {
            if (!(ckey(T[i]) in MOVED)) continue
            DROP[i] = 1
            for (j = i - 1; j >= 1 && T[j] ~ /^[ \t]*[;#]/ && ckey(T[j]) == ""; j--) DROP[j] = 1
        }
        for (i = 1; i <= NR; i++) if (!(i in DROP)) print L[i]
    }
' "$server_ini" > "$tmp"
commit "$server_ini" "$tmp" || say "  nothing to remove"

if (( dry_run )); then
    say "Dry run: nothing was written."
else
    say "Done. Restart llama-server to launch with the migrated presets."
fi
