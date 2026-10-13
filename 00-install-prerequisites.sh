#!/usr/bin/env bash
# Idempotent toolchain bootstrapper for Linux (Debian/Ubuntu), the counterpart
# of 00-install-prerequisites.ps1. Safe to re-run any time.
#
#   - apt packages (git, g++, cmake, ninja, OpenSSL headers, node + npm):
#     the one leg that needs root, through sudo, and only when one is missing.
#   - sccache from its GitHub release into ~/.local/bin.
#   - Rust through rustup, when missing (for llama-cpp-config).
#   - the newest STABLE ROCm/TheRock dist, CUDA Toolkit and Vulkan SDK, each
#     into <Root>/<Version>/linux (installer/dist-sources.sh says where each
#     is looked up and installed; nothing is pinned). User scope, no root: the
#     roots must be writable by the current user.
#   - llama.cpp release check and a report.
#
# A source that cannot be reached keeps what is installed and the report says
# so; nothing is ever downgraded or deleted. Downloads resume: an interrupted
# run picks the partial file up from <Root>/.downloads.
#
# The dists go under the dist root: --dist-root, else LLAMA_FW_DIST_ROOT, else
# the one 01-configure.sh recorded, else the default in dist-sources.sh.
#
# Usage: ./00-install-prerequisites.sh [--dist-root <path>]
#                                      [--skip apt,sccache,rust,rocm,cuda,vulkan]

. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/common.sh"

SKIP=','
while (( $# )); do
    case "$1" in
        --skip)   SKIP=",${2:?--skip needs a list},"; shift 2 ;;
        --skip=*) SKIP=",${1#--skip=},"; shift ;;
        --dist-root)   use_dist_root "${2:?--dist-root needs a path}"; shift 2 ;;
        --dist-root=*) use_dist_root "${1#--dist-root=}"; shift ;;
        -h|--help) sed -n '2,/^$/s/^# \{0,1\}//p' "${BASH_SOURCE[0]}"; exit 0 ;;
        *) die "unknown argument: $1" ;;
    esac
done
skipped() { [[ "$SKIP" == *",$1,"* ]]; }

REPORT=()      # one line per leg, printed at the end
GAPS=()        # what still has to be fixed by hand
note() { REPORT+=("$1"); }
gap()  { GAPS+=("$1"); }

fetch() { curl -fsSL --retry 3 --connect-timeout 20 "$@"; }

# Resumable download into <dir>/<name>, with an optional sha256. A file that is
# already complete and matches is reused; one that fails the hash is deleted so
# the next run starts clean instead of resuming garbage.
download() {
    local url="$1" dest="$2" sha="${3:-}"
    mkdir -p "$(dirname "$dest")"
    if [[ -n "$sha" && -f "$dest" ]] && sha256sum "$dest" | grep -qi "^$sha "; then
        return 0
    fi
    say_dim "    downloading $(basename "$dest")"
    # -C - resumes; a server answering 416 means the file is already whole.
    local rc=0
    local progress=(-sS); [[ -t 2 ]] && progress=(--progress-bar)
    curl -fL --retry 5 --retry-delay 5 --connect-timeout 20 -C - "${progress[@]}" -o "$dest" "$url" || rc=$?
    # 22 with a file already on disk is the 416 case: the hash (or, for an
    # archive without one, the extract) decides whether it really is whole.
    if (( rc != 0 )) && ! { (( rc == 22 )) && [[ -s "$dest" ]]; }; then
        return 1
    fi
    if [[ -n "$sha" ]] && ! sha256sum "$dest" | grep -qi "^$sha "; then
        say_err "    sha256 mismatch on $(basename "$dest"), deleted"
        rm -f "$dest"
        return 1
    fi
    return 0
}

# Free space check before a big extract, in GiB.
need_space() {
    local dir="$1" need_gib="$2" avail
    mkdir -p "$dir"
    avail=$(( $(df --output=avail -k "$dir" | tail -n1) / 1048576 ))
    (( avail >= need_gib )) || { say_err "    only ${avail} GiB free under $dir, ${need_gib} GiB needed"; return 1; }
}

say ''
say_head '  llama.cpp-framework: prerequisites (Linux)'
say_head '  =========================================='
say ''

# ── apt packages ────────────────────────────────────────────────────
# g++ is needed even though the host compiler is the ROCm dist's clang: clang
# takes libstdc++, the C runtime start files and libgcc from the distro's GCC
# install, and nvcc drives g++ as its host compiler. nodejs + npm build the
# chat UI that llama-server embeds (LLAMA_BUILD_UI=ON).
# libfontconfig-dev is for llama-cpp-config: Slint's Skia renderer links the
# system fontconfig (and freetype, which it pulls in).
# patchelf, dpkg-dev and rpm are 03-package.sh's: the RUNPATH rewrite, the
# Depends computation (dpkg-shlibdeps) and rpmbuild.
APT_PACKAGES=(git g++ make cmake ninja-build pkg-config libssl-dev nodejs npm
              curl jq xz-utils ca-certificates libfontconfig-dev
              patchelf dpkg-dev rpm)
if skipped apt; then
    note 'apt packages   : skipped'
elif ! command -v apt-get >/dev/null; then
    note 'apt packages   : apt-get not found (not a Debian/Ubuntu system)'
    gap "install by hand the equivalents of: ${APT_PACKAGES[*]}"
else
    say_head '  System packages (apt)'
    missing=()
    for p in "${APT_PACKAGES[@]}"; do
        dpkg-query -W -f='${Status}' "$p" 2>/dev/null | grep -q 'install ok installed' || missing+=("$p")
    done
    if (( ${#missing[@]} == 0 )); then
        say_ok "    all ${#APT_PACKAGES[@]} present"
        note "apt packages   : all ${#APT_PACKAGES[@]} present"
    else
        say "    missing: ${missing[*]}"
        # Non-interactive when there is no terminal to type a password into, so
        # a scripted run fails here in one line rather than hanging on a prompt.
        sudo_flags=(); [[ -t 0 ]] || sudo_flags=(-n)
        if sudo "${sudo_flags[@]}" apt-get update -qq &&
           sudo "${sudo_flags[@]}" env DEBIAN_FRONTEND=noninteractive apt-get install -y "${missing[@]}"; then
            note "apt packages   : installed ${missing[*]}"
        else
            note "apt packages   : FAILED to install ${missing[*]}"
            gap "sudo apt-get install -y ${missing[*]}"
        fi
    fi
    say ''
fi

# ── sccache ─────────────────────────────────────────────────────────
if skipped sccache; then
    note 'sccache        : skipped'
else
    say_head '  sccache'
    have=''
    [[ -x "$SCCACHE_BIN" ]] && have="$("$SCCACHE_BIN" --version 2>/dev/null | awk '{print $2}')"
    case "$(uname -m)" in
        x86_64)  sc_arch='x86_64-unknown-linux-musl' ;;
        aarch64) sc_arch='aarch64-unknown-linux-musl' ;;
        *)       sc_arch='' ;;
    esac
    rel=''
    [[ -n "$sc_arch" ]] && rel="$(fetch "$SCCACHE_RELEASE_API" 2>/dev/null || true)"
    latest="$(jq -r '.tag_name // empty' <<<"$rel" 2>/dev/null | sed 's/^v//')"
    if [[ -z "$latest" ]]; then
        note "sccache        : ${have:-not installed} (release lookup failed, kept)"
    elif [[ -n "$have" ]] && ! version_gt "$latest" "$have"; then
        say_ok "    $have is current"
        note "sccache        : $have (current)"
    else
        asset="sccache-v$latest-$sc_arch.tar.gz"
        url="$(jq -r --arg n "$asset" '.assets[] | select(.name == $n) | .browser_download_url' <<<"$rel")"
        sha="$(jq -r --arg n "$asset" '.assets[] | select(.name == $n) | .digest // empty' <<<"$rel" | sed 's/^sha256://')"
        tmp="$(mktemp -d)"
        if [[ -n "$url" ]] && download "$url" "$tmp/$asset" "$sha" &&
           tar -xzf "$tmp/$asset" -C "$tmp" --strip-components=1 &&
           "$tmp/sccache" --version >/dev/null; then
            mkdir -p "$(dirname "$SCCACHE_BIN")"
            # Staged beside the target and MOVED over it: a rename replaces a
            # binary that is running without disturbing the process.
            install -m 755 "$tmp/sccache" "$SCCACHE_BIN.new" && mv -f "$SCCACHE_BIN.new" "$SCCACHE_BIN"
            say_ok "    ${have:-none} -> $latest ($SCCACHE_BIN)"
            note "sccache        : ${have:-none} -> $latest"
        else
            note "sccache        : ${have:-not installed} (download of $latest failed, kept)"
        fi
        rm -rf "$tmp"
    fi
    say ''
fi

# ── Rust (rustup) ───────────────────────────────────────────────────
# For llama-cpp-config. Installed when missing, user scope, without touching
# the shell profile (02-build.sh finds ~/.cargo/bin itself). An existing
# toolchain is left alone: a rustc upgrade recompiles the whole dependency
# tree, so `rustup update` stays the user's call.
if skipped rust; then
    note 'Rust           : skipped'
else
    say_head '  Rust'
    cargo_bin="$(command -v cargo || true)"
    [[ -z "$cargo_bin" && -x "$HOME/.cargo/bin/cargo" ]] && cargo_bin="$HOME/.cargo/bin/cargo"
    if [[ -n "$cargo_bin" ]]; then
        ver="$("$cargo_bin" --version | awk '{print $2}')"
        say_ok "    cargo $ver ($cargo_bin)"
        note "Rust           : cargo $ver"
    else
        tmp="$(mktemp -d)"
        if curl --proto '=https' --tlsv1.2 -fsSL "$RUSTUP_INIT_URL" -o "$tmp/rustup-init.sh" &&
           sh "$tmp/rustup-init.sh" -y --profile minimal --default-toolchain stable --no-modify-path >/dev/null 2>&1 &&
           [[ -x "$HOME/.cargo/bin/cargo" ]]; then
            ver="$("$HOME/.cargo/bin/cargo" --version | awk '{print $2}')"
            say_ok "    installed cargo $ver ($HOME/.cargo/bin)"
            note "Rust           : none -> cargo $ver"
        else
            note 'Rust           : rustup install FAILED'
            gap 'Rust: install it from https://rustup.rs'
        fi
        rm -rf "$tmp"
    fi
    say ''
fi

# ── ROCm / HIP (TheRock dist) ───────────────────────────────────────
rocm_latest() {   # prints "<version> <url>" of the newest stable in the indexes
    local idx name ver best_ver='' best_url=''
    for idx in "${ROCM_INDEXES[@]}"; do
        while read -r name; do
            ver="${name#therock-dist-linux-"$ROCM_FLAVOR"-}"; ver="${ver%.tar.gz}"
            if [[ -z "$best_ver" ]] || version_gt "$ver" "$best_ver"; then
                best_ver="$ver"; best_url="$idx$name"
            fi
        done < <(fetch "$idx" 2>/dev/null |
                 grep -oE "therock-dist-linux-$ROCM_FLAVOR-[0-9]+\.[0-9]+\.[0-9]+\.tar\.gz" | sort -u)
    done
    [[ -n "$best_ver" ]] && printf '%s %s\n' "$best_ver" "$best_url"
    return 0
}

if skipped rocm; then
    note 'ROCm (TheRock) : skipped'
else
    say_head '  ROCm / HIP (TheRock dist)'
    have_dir="$(find_rocm)"; have=''
    [[ -n "$have_dir" ]] && have="$(dist_version "$have_dir")"
    read -r latest url < <(rocm_latest; echo) || true
    if [[ -z "${latest:-}" ]]; then
        note "ROCm (TheRock) : ${have:-not installed} (indexes unreachable, kept)"
        [[ -z "$have" ]] && gap "ROCm: no dist under $ROCM_ROOT and the AMD indexes could not be read"
    elif [[ -n "$have" ]] && ! version_gt "$latest" "$have"; then
        say_ok "    $have is current ($have_dir)"
        note "ROCm (TheRock) : $have (current)"
    else
        dest="$ROCM_ROOT/$latest/$DIST_OS"
        tarball="$ROCM_ROOT/.downloads/$(basename "$url")"
        say "    ${have:-none} -> $latest, into $dest"
        # A tree without the marker is an interrupted extract of this same
        # version: cleared and redone. A complete one never reaches this point.
        if need_space "$ROCM_ROOT" 45 && download "$url" "$tarball" &&
           { rm -rf "$dest"; mkdir -p "$dest"; } &&
           { say_dim '    extracting (several minutes)'; tar -xzf "$tarball" -C "$dest"; } &&
           printf '%s\n' "$latest" > "$dest/$ROCM_MARKER" && [[ "$(find_rocm)" == "$dest" ]]; then
            rm -f "$tarball"
            say_ok "    installed $latest"
            note "ROCm (TheRock) : ${have:-none} -> $latest (re-run 01-configure.sh, then 02-build.sh)"
        else
            note "ROCm (TheRock) : install of $latest FAILED, ${have:-nothing} kept"
            gap "ROCm: re-run this script (the download resumes from $tarball)"
        fi
    fi
    say ''
fi

# ── CUDA Toolkit (redist archives) ──────────────────────────────────
if skipped cuda; then
    note 'CUDA Toolkit   : skipped'
else
    say_head '  CUDA Toolkit'
    have_dir="$(find_cuda)"; have=''
    [[ -n "$have_dir" ]] && have="$(dist_version "$have_dir")"
    latest="$(fetch "$CUDA_REDIST_INDEX" 2>/dev/null |
              grep -oE 'redistrib_[0-9]+\.[0-9]+\.[0-9]+\.json' | sort -uV | tail -n1 |
              sed -E 's/^redistrib_(.*)\.json$/\1/')" || true
    if [[ -z "$latest" ]]; then
        note "CUDA Toolkit   : ${have:-not installed} (redist index unreachable, kept)"
        [[ -z "$have" ]] && gap "CUDA: no toolkit under $CUDA_ROOT and the NVIDIA redist index could not be read"
    elif [[ -n "$have" ]] && ! version_gt "$latest" "$have"; then
        say_ok "    $have is current ($have_dir)"
        note "CUDA Toolkit   : $have (current)"
    else
        dest="$CUDA_ROOT/$latest/$DIST_OS"
        cache="$CUDA_ROOT/.downloads/$latest"
        say "    ${have:-none} -> $latest, into $dest"
        ok=1
        manifest="$(fetch "${CUDA_REDIST_INDEX}redistrib_$latest.json")" || ok=0
        if (( ok )) && need_space "$CUDA_ROOT" 8; then
            rm -rf "$dest"; mkdir -p "$dest"
            for comp in "${CUDA_COMPONENTS[@]}"; do
                rel="$(jq -r --arg c "$comp" --arg p "$CUDA_PLATFORM" '.[$c][$p].relative_path // empty' <<<"$manifest")"
                sha="$(jq -r --arg c "$comp" --arg p "$CUDA_PLATFORM" '.[$c][$p].sha256 // empty' <<<"$manifest")"
                if [[ -z "$rel" || -z "$sha" ]]; then
                    say_err "    component $comp is not in redistrib_$latest.json for $CUDA_PLATFORM"
                    ok=0; break
                fi
                # Every archive unpacks as <name>-archive/{bin,include,lib,...};
                # stripping that one level merges them into a single toolkit prefix.
                if ! { download "$CUDA_REDIST_INDEX$rel" "$cache/$(basename "$rel")" "$sha" &&
                       tar -xJf "$cache/$(basename "$rel")" -C "$dest" --strip-components=1; }; then
                    ok=0; break
                fi
            done
        else
            ok=0
        fi
        # The redist archives carry lib/, while nvcc links its own runtime
        # from <toolkit>/lib64 (the layout of the .run installer): alias it.
        if (( ok )) && ln -sfn lib "$dest/lib64" &&
           printf '%s\n' "$latest" > "$dest/$CUDA_MARKER" && [[ "$(find_cuda)" == "$dest" ]]; then
            rm -rf "$cache"
            say_ok "    installed $latest"
            note "CUDA Toolkit   : ${have:-none} -> $latest (re-run 02-build.sh)"
        else
            note "CUDA Toolkit   : install of $latest FAILED, ${have:-nothing} kept"
            gap "CUDA: re-run this script (finished downloads are reused from $cache)"
        fi
    fi
    say ''
fi

# ── Vulkan SDK ──────────────────────────────────────────────────────
if skipped vulkan; then
    note 'Vulkan SDK     : skipped'
else
    say_head '  Vulkan SDK'
    have_dir="$(find_vulkan)"; have=''
    [[ -n "$have_dir" ]] && have="$(dist_version "$have_dir")"
    latest="$(fetch "$VULKAN_LATEST_URL" 2>/dev/null | tr -d '[:space:]')" || true
    if [[ ! "$latest" =~ ^[0-9]+(\.[0-9]+)+$ ]]; then
        note "Vulkan SDK     : ${have:-not installed} (LunarG unreachable, kept)"
        [[ -z "$have" ]] && gap "Vulkan: no SDK under $VULKAN_ROOT and LunarG could not be reached"
    elif [[ -n "$have" ]] && ! version_gt "$latest" "$have"; then
        say_ok "    $have is current ($have_dir)"
        note "Vulkan SDK     : $have (current)"
    else
        dest="$VULKAN_ROOT/$latest/$DIST_OS"
        # shellcheck disable=SC2059
        url="$(printf "$VULKAN_TARBALL" "$latest" "$latest")"
        tarball="$VULKAN_ROOT/.downloads/$(basename "$url")"
        sha="$(fetch "$VULKAN_FILES_JSON" 2>/dev/null |
               jq -r --arg v "$latest" --arg f "$(basename "$url")" \
                  '.linux[$v].files[]? | select(.file_name == $f) | .sha // empty')" || true
        say "    ${have:-none} -> $latest, into $dest"
        if [[ -z "$sha" ]]; then
            say_err "    no sha256 for $(basename "$url") in files.json: not installing an unverified SDK"
            note "Vulkan SDK     : install of $latest FAILED (no checksum), ${have:-nothing} kept"
            gap 'Vulkan: re-run this script later'
        elif need_space "$VULKAN_ROOT" 4 && download "$url" "$tarball" "$sha" &&
             { rm -rf "$dest"; mkdir -p "$dest"; } &&
             tar -xJf "$tarball" -C "$dest" --strip-components=1 &&
             printf '%s\n' "$latest" > "$dest/$VULKAN_MARKER" && [[ "$(find_vulkan)" == "$dest/x86_64" ]]; then
            rm -f "$tarball"
            say_ok "    installed $latest"
            note "Vulkan SDK     : ${have:-none} -> $latest (re-run 02-build.sh)"
        else
            note "Vulkan SDK     : install of $latest FAILED, ${have:-nothing} kept"
            gap "Vulkan: re-run this script (the download resumes from $tarball)"
        fi
    fi
    say ''
fi

# ── llama.cpp release check ─────────────────────────────────────────
# Fetch only: the clone sits on a detached release tag and the checkout is
# 02-build.sh's job. Same selection as there (newest vX.Y.Z reachable from
# origin/master, the bNNNN nightlies ignored).
llama_dir="$BUILD_ROOT/llama.cpp"
if [[ -f "$CONFIG_FILE" ]]; then
    llama_dir="$(. "$CONFIG_FILE"; printf '%s' "${CFG_LLAMA_CPP_DIR:-$llama_dir}")"
fi
if [[ -d "$llama_dir/.git" ]] && command -v git >/dev/null; then
    cur="$(git -C "$llama_dir" describe --tags --exact-match 2>/dev/null || echo 'untagged')"
    if git -C "$llama_dir" fetch --quiet origin --tags 2>/dev/null; then
        new="$(git -C "$llama_dir" tag --list 'v[0-9]*' --merged origin/master --sort=-v:refname |
               grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | head -n1)"
        if [[ "$cur" == "$new" ]]; then note "llama.cpp      : $cur (latest release)"
        else note "llama.cpp      : $cur, release $new available (run 02-build.sh)"; fi
    else
        note "llama.cpp      : $cur (fetch failed)"
    fi
else
    note 'llama.cpp      : not cloned yet (02-build.sh clones it)'
fi

# ── Report ──────────────────────────────────────────────────────────
say_head '  Report'
say_head '  ------'
for line in "${REPORT[@]}"; do say "  $line"; done
# Inactive dists are listed, never deleted: a working rollback is the user's
# to keep or remove.
for root in "$ROCM_ROOT" "$CUDA_ROOT" "$VULKAN_ROOT"; do
    [[ -d "$root" ]] || continue
    n="$(find "$root" -mindepth 2 -maxdepth 2 -type d -name "$DIST_OS" | wc -l)"
    (( n > 1 )) && say_dim "  $root holds $n $DIST_OS dists; older ones are kept until you delete them"
done
say ''
if (( ${#GAPS[@]} )); then
    say_err "  To fix by hand (${#GAPS[@]}):"
    for g in "${GAPS[@]}"; do say_err "    - $g"; done
    say ''
    exit 1
fi
# 01-configure.sh has to be told the dist root only when it would not resolve
# this one by itself (a --dist-root given here and recorded nowhere yet).
configure='./01-configure.sh'
if [[ -n "${DIST_ROOT_OVERRIDE:-}" && -z "${LLAMA_FW_DIST_ROOT:-}" && "$DIST_ROOT" != "$CFG_DIST_ROOT" ]]; then
    configure+=" --dist-root $DIST_ROOT"
fi
say_ok "  All prerequisites in place. Next: $configure, then ./02-build.sh"
say ''
