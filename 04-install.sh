#!/usr/bin/env bash
# Install the newest package 03-package.sh left in ./dist/ on this machine:
# a first install, an upgrade (a newer framework or a newer bundled llama.cpp:
# the package version is <framework>+llama.cpp.<tag>), a reinstall of the
# same version (a repackage, which the package manager would otherwise call
# "already installed") or a downgrade, all the same command. Linux only; the Windows installer is run by hand.
#
# The format follows the distribution (/etc/os-release, then the package
# managers on PATH): the .deb through apt-get (which also pulls the Depends),
# the .rpm through dnf, zypper or bare rpm. A package file given on the
# command line brings its own format.
# "Newest" is the last one written of the files named for this architecture.
#
# Refuses to run while a program from /opt/llama.cpp is running: rocBLAS and
# hipBLASLt load their kernel files lazily, so a server that outlives the
# upgrade would read the NEW kernel files with the OLD libraries.
#
# Usage: ./04-install.sh [--deb | --rpm] [--force] [--dry-run] [<package file>]
#   --deb / --rpm    pick the format (default: by the host's package manager)
#   <package file>   install that file instead of the newest in ./dist/
#   --force          install even while llama-server or llama-cpp-config runs
#   --dry-run        print the command instead of running it

. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/common.sh"

FORMAT=''; FORCE=0; DRY_RUN=0; PKG_FILE=''
while (( $# )); do
    case "$1" in
        --deb) FORMAT='deb'; shift ;;
        --rpm) FORMAT='rpm'; shift ;;
        --force) FORCE=1; shift ;;
        --dry-run) DRY_RUN=1; shift ;;
        -h|--help) sed -n '2,/^$/s/^# \{0,1\}//p' "${BASH_SOURCE[0]}"; exit 0 ;;
        -*) die "unknown argument: $1" ;;
        *) [[ -z "$PKG_FILE" ]] || die "more than one package file given"; PKG_FILE="$1"; shift ;;
    esac
done

PKG_NAME='llama-cpp-framework'
OPT_DIR='/opt/llama.cpp'
OUT_DIR="$REPO_ROOT/dist"

# ── Format and file ─────────────────────────────────────────────────
if [[ -n "$PKG_FILE" ]]; then
    [[ -f "$PKG_FILE" ]] || die "no such file: $PKG_FILE"
    PKG_FILE="$(readlink -f "$PKG_FILE")"
    case "$PKG_FILE" in
        *.deb) FORMAT="${FORMAT:-deb}" ;;
        *.rpm) FORMAT="${FORMAT:-rpm}" ;;
        *) die "not a .deb or an .rpm: $PKG_FILE" ;;
    esac
    [[ "$PKG_FILE" == *."$FORMAT" ]] || die "--$FORMAT given for $(basename "$PKG_FILE")"
fi
# The distribution decides, from ID and ID_LIKE in /etc/os-release. The tools
# on PATH come second and are not enough alone: a Debian build host has rpm
# too (03-package.sh's rpmbuild), and rpm there has no database.
detect_format() {
    local ids='' id
    if [[ -r /etc/os-release ]]; then
        ids="$( . /etc/os-release; printf '%s %s' "${ID:-}" "${ID_LIKE:-}" )"
    fi
    for id in $ids; do
        case "$id" in
            debian|ubuntu) echo deb; return ;;
            fedora|rhel|centos|rocky|almalinux|suse|opensuse*|sles|mageia|openmandriva) echo rpm; return ;;
        esac
    done
    if command -v apt-get >/dev/null && command -v dpkg >/dev/null; then echo deb
    elif command -v dnf >/dev/null || command -v zypper >/dev/null; then echo rpm
    elif command -v rpm >/dev/null && ! command -v dpkg >/dev/null; then echo rpm
    fi
}
if [[ -z "$FORMAT" ]]; then
    FORMAT="$(detect_format)"
    [[ -n "$FORMAT" ]] || die 'cannot tell whether this machine takes a .deb or an .rpm: pass --deb or --rpm'
    say_dim "Format: .$FORMAT (detected)"
fi

if [[ -z "$PKG_FILE" ]]; then
    # Each format names the architecture its own way, as in 03-package.sh.
    case "$(uname -m)" in
        x86_64)  arch="$([[ $FORMAT == deb ]] && echo amd64 || echo x86_64)" ;;
        aarch64) arch="$([[ $FORMAT == deb ]] && echo arm64 || echo aarch64)" ;;
        *) die "unsupported architecture: $(uname -m)" ;;
    esac
    PKG_FILE="$(ls -t "$OUT_DIR/$PKG_NAME"-v*-"$arch.$FORMAT" 2>/dev/null | head -n1 || true)"
    [[ -n "$PKG_FILE" ]] || die "no $PKG_NAME .$FORMAT for $arch in $OUT_DIR. Run 03-package.sh first."
fi

# A package written before the last build is not what the tree holds now.
built="$BUILD_ROOT/llama.cpp-cmake/bin/llama-server"
if [[ -e "$built" && "$built" -nt "$PKG_FILE" ]]; then
    say_warn "$(basename "$PKG_FILE") predates the last 02-build.sh: run 03-package.sh to package the new build"
fi

# ── Versions ────────────────────────────────────────────────────────
if [[ "$FORMAT" == deb ]]; then
    new_version="$(dpkg-deb -f "$PKG_FILE" Version)"
    old_version="$(dpkg-query -W -f='${db:Status-Status} ${Version}' "$PKG_NAME" 2>/dev/null |
                   sed -n 's/^installed //p' || true)"
else
    new_version="$(rpm -qp --qf '%{VERSION}-%{RELEASE}' "$PKG_FILE" 2>/dev/null)"
    old_version="$(rpm -q --qf '%{VERSION}-%{RELEASE}' "$PKG_NAME" 2>/dev/null || true)"
    [[ "$old_version" == *' '* || "$old_version" == *'not installed'* ]] && old_version=''
fi
[[ -n "$new_version" ]] || die "could not read the version of $PKG_FILE"

if [[ -z "$old_version" ]]; then action='install'
elif [[ "$old_version" == "$new_version" ]]; then action='reinstall'
elif [[ "$(printf '%s\n%s\n' "$old_version" "$new_version" | sort -V | tail -n1)" == "$new_version" ]]; then action='upgrade'
else action='downgrade'; fi

say_head "Package:   $(basename "$PKG_FILE") ($(du -m "$PKG_FILE" | cut -f1) MiB)"
say_head "Installed: ${old_version:-none}"
say_head "Action:    $action to $new_version"

# ── Running programs ────────────────────────────────────────────────
running="$(pgrep -a -f "^$OPT_DIR/bin/" || true)"
if [[ -n "$running" ]]; then
    say_warn "Running from $OPT_DIR:"
    while read -r line; do say_dim "  $line"; done <<<"$running"
    (( FORCE )) || die "stop them first (or pass --force): a server that outlives the $action loads the new kernel files with its old libraries"
    say_warn 'Continuing (--force): restart them once the install is done'
fi

# ── Install ─────────────────────────────────────────────────────────
SUDO=()
(( EUID == 0 )) || SUDO=(sudo)

if [[ "$FORMAT" == deb ]]; then
    # --reinstall makes the same version count; --allow-downgrades the older.
    cmd=(apt-get install -y --reinstall --allow-downgrades "$PKG_FILE")
elif command -v dnf >/dev/null; then
    case "$action" in
        reinstall) cmd=(dnf reinstall -y "$PKG_FILE") ;;
        downgrade) cmd=(dnf downgrade -y "$PKG_FILE") ;;
        *)         cmd=(dnf install -y "$PKG_FILE") ;;
    esac
elif command -v zypper >/dev/null; then
    # The package is unsigned; --force covers reinstall and downgrade alike.
    cmd=(zypper --non-interactive install --force --allow-unsigned-rpm "$PKG_FILE")
else
    # Bare rpm checks the dependencies but cannot fetch them.
    cmd=(rpm -Uvh --replacepkgs --oldpackage "$PKG_FILE")
fi

say_head "Running: ${SUDO[*]}${SUDO:+ }${cmd[*]}"
if (( DRY_RUN )); then
    say_dim '(dry run: nothing installed)'
    exit 0
fi
"${SUDO[@]}" "${cmd[@]}"

# ── Check ───────────────────────────────────────────────────────────
version="$("$OPT_DIR/bin/llama-cpp-config" --version 2>/dev/null | awk '{print $NF}' || true)"
[[ "$new_version" == "$version"[+-]* ]] ||
    die "$OPT_DIR/bin/llama-cpp-config reports '${version:-nothing}', the package is $new_version"
say ''
say_ok "$PKG_NAME $new_version installed in $OPT_DIR ($action)"
[[ -n "$running" ]] && say_warn 'Restart the programs listed above to use it'
say ''
