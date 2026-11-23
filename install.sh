#!/bin/sh
# Install sshtui to ~/.local/bin (or $SSHTUI_INSTALL_DIR) without sudo.
set -eu

REPO="hungryZoo/sshtui"
BIN_DIR="${SSHTUI_INSTALL_DIR:-${SSHTUI_BIN_DIR:-$HOME/.local/bin}}"
VERSION="${SSHTUI_VERSION:-}"
DOWNLOAD_BASE="${SSHTUI_DOWNLOAD_BASE:-}"
tmp_dir=
staged=

cleanup() {
    [ -z "$staged" ] || rm -f "$staged"
    [ -z "$tmp_dir" ] || rm -rf "$tmp_dir"
}
trap cleanup 0
trap 'exit 1' HUP INT TERM

err() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

info() {
    printf '==> %s\n' "$*"
}

usage() {
    cat <<'HELP'
Install sshtui without sudo.

Usage: install.sh [OPTIONS]
  --bin-dir DIR   Install directory (default: ~/.local/bin)
  --version VER   Release version (default: latest)
  -h, --help      Show this help

Environment:
  SSHTUI_INSTALL_DIR, SSHTUI_BIN_DIR, SSHTUI_VERSION
  SSHTUI_DOWNLOAD_BASE   Release asset URL (requires an explicit version)
HELP
}

unsupported() {
    err "$*. Supported platforms: macOS (Apple Silicon and Intel), Linux (x86_64, ARM64, ARMv7, ARMv6), and Windows (x64 and ARM64 via install.ps1).
Build from source with: cargo install --git https://github.com/$REPO --locked"
}

detect_triple() {
    os="$(uname -s)"
    arch="$(uname -m)"
    case "$os" in
        Darwin)
            if [ "$(sysctl -n hw.optional.arm64 2>/dev/null || :)" = 1 ]; then
                echo aarch64-apple-darwin
            else
                case "$arch" in
                    arm64|aarch64) echo aarch64-apple-darwin ;;
                    x86_64) echo x86_64-apple-darwin ;;
                    *) unsupported "Unsupported macOS architecture: $arch" ;;
                esac
            fi
            ;;
        Linux)
            case "$arch" in
                x86_64|amd64) echo x86_64-unknown-linux-musl ;;
                # A static aarch64 binary also runs with a 32-bit userland.
                aarch64|arm64) echo aarch64-unknown-linux-musl ;;
                armv7l|armv8l) echo armv7-unknown-linux-musleabihf ;;
                armv6l) echo arm-unknown-linux-musleabihf ;;
                *) unsupported "Unsupported Linux architecture: $arch" ;;
            esac
            ;;
        MSYS*|MINGW*|CYGWIN*) err "Run install.ps1 from PowerShell to install sshtui on Windows." ;;
        *) unsupported "Unsupported OS: $os" ;;
    esac
}

valid_version() {
    case "$1" in *[!0-9A-Za-z.+-]*) return 1 ;; esac
    printf '%s\n' "$1" | LC_ALL=C grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$'
}

fetch() {
    # wget has no file:// support. Use a direct copy for local mirrors.
    case "$1" in
        file://*)
            if [ -f "${1#file://}" ]; then
                cp "${1#file://}" "$2"
                return
            fi
            ;;
    esac
    case "$downloader" in
        curl) curl -q -fsSL -o "$2" -- "$1" ;;
        wget) wget -q -O "$2" -- "$1" ;;
        *) return 1 ;;
    esac
}

get_latest_version() {
    # The redirect avoids the unauthenticated API rate limit.
    if [ "$downloader" = curl ]; then
        final="$(curl -q -fsSLI -o /dev/null -w '%{url_effective}' \
            "https://github.com/$REPO/releases/latest" 2>/dev/null || :)"
        case "$final" in
            */tag/v*)
                tag="${final##*/tag/v}"
                if valid_version "$tag"; then
                    printf '%s\n' "$tag"
                    return
                fi
                ;;
        esac
    fi
    fetch "https://api.github.com/repos/$REPO/releases/latest" "$tmp_dir/latest.json" \
        || err "Could not fetch the latest release from GitHub."
    sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"v\([^"]*\)".*/\1/p' \
        "$tmp_dir/latest.json"
}

parse_args() {
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --bin-dir|--version)
                [ "$#" -ge 2 ] && [ -n "$2" ] || err "$1 needs a value."
                case "$1" in
                    --bin-dir) BIN_DIR="$2" ;;
                    --version) VERSION="$2" ;;
                esac
                shift 2
                ;;
            --bin-dir=*) BIN_DIR="${1#*=}"; shift ;;
            --version=*) VERSION="${1#*=}"; shift ;;
            -h|--help) usage; exit 0 ;;
            *) err "Unknown option: $1 (try --help)." ;;
        esac
    done
}

verify_checksum() {
    expected="$(LC_ALL=C awk -v asset="$asset" '
        { sub(/\r$/, ""); sub(/^\*/, "", $2) }
        $2 == asset && length($1) == 64 && $1 ~ /^[[:xdigit:]]+$/ {
            print tolower($1)
        }
    ' "$tmp_dir/SHA256SUMS")"
    [ "${#expected}" -eq 64 ] || err "Expected one SHA256 checksum for $asset."

    if command -v sha256sum >/dev/null 2>&1; then
        digest="$(sha256sum < "$tmp_dir/$asset")"
        digest="${digest%% *}"
    elif command -v shasum >/dev/null 2>&1; then
        digest="$(shasum -a 256 < "$tmp_dir/$asset")"
        digest="${digest%% *}"
    elif command -v openssl >/dev/null 2>&1; then
        digest="$(openssl dgst -sha256 < "$tmp_dir/$asset")"
        digest="${digest##* }"
    else
        err "SHA256 verification needs sha256sum, shasum, or openssl."
    fi
    [ "$digest" = "$expected" ] || err "SHA256 checksum mismatch for $asset."
}

print_path_hint() {
    case ":${PATH:-}:" in *":$BIN_DIR:"*) return 0 ;; esac
    info "$BIN_DIR is not on PATH. Run the following in your shell:"
    shell="${SHELL:-sh}"
    case "${shell##*/}" in
        fish)
            quoted="$(printf '%s' "$BIN_DIR" | sed "s/[\\\\']/\\\\&/g")"
            printf "fish_add_path '%s'\n" "$quoted"
            ;;
        csh|tcsh)
            quoted="$(printf '%s' "$BIN_DIR" | sed 's/[\\"$`!]/\\&/g')"
            # The printed command must expand $path in csh or tcsh.
            # shellcheck disable=SC2016
            printf 'set path = ("%s" $path)\n' "$quoted"
            ;;
        nu)
            quoted="$(printf '%s' "$BIN_DIR" | sed 's/[\\"]/\\&/g')"
            # The printed command refers to $env.PATH in Nushell.
            # shellcheck disable=SC2016
            printf '$env.PATH = ($env.PATH | prepend "%s")\n' "$quoted"
            ;;
        *)
            quoted="$(printf '%s' "$BIN_DIR" | sed 's/[\\"$`]/\\&/g')"
            # The printed command must expand $PATH in the user's shell.
            # shellcheck disable=SC2016
            printf 'export PATH="%s:$PATH"\n' "$quoted"
            ;;
    esac
}

main() {
    parse_args "$@"
    [ -n "$BIN_DIR" ] || err "The install directory must not be empty."
    if [ -n "$DOWNLOAD_BASE" ] && [ -z "$VERSION" ]; then
        err "SSHTUI_DOWNLOAD_BASE requires --version or SSHTUI_VERSION."
    fi
    if [ -n "$VERSION" ]; then
        VERSION="${VERSION#v}"
        valid_version "$VERSION" || err "Invalid release version: $VERSION"
    fi
    case "$BIN_DIR" in /*) ;; *) BIN_DIR="$PWD/$BIN_DIR" ;; esac

    triple="$(detect_triple)"
    downloader=
    if command -v curl >/dev/null 2>&1; then
        downloader=curl
    elif command -v wget >/dev/null 2>&1; then
        downloader=wget
    else
        case "$DOWNLOAD_BASE" in file://*) ;; *) err "Install curl or wget to download release assets." ;; esac
    fi

    tmp_dir="$(mktemp -d)"
    if [ -z "$VERSION" ]; then
        VERSION="$(get_latest_version)"
        valid_version "$VERSION" || err "Could not determine the latest release version."
    fi
    base="${DOWNLOAD_BASE:-https://github.com/$REPO/releases/download/v$VERSION}"
    asset="sshtui-$VERSION-$triple.tar.gz"
    info "Downloading $asset"
    fetch "${base%/}/SHA256SUMS" "$tmp_dir/SHA256SUMS" || err "Could not download SHA256SUMS."
    fetch "${base%/}/$asset" "$tmp_dir/$asset" || err "Could not download $asset."
    verify_checksum

    mkdir -p "$BIN_DIR"
    [ ! -d "$BIN_DIR/sshtui" ] || err "$BIN_DIR/sshtui is a directory."
    staged="$(mktemp "$BIN_DIR/.sshtui.XXXXXX")"
    tar -xOzf "$tmp_dir/$asset" sshtui > "$staged"
    chmod 755 "$staged"
    [ "$("$staged" --version)" = "sshtui $VERSION" ] \
        || err "The downloaded executable did not report sshtui $VERSION."
    mv -f "$staged" "$BIN_DIR/sshtui"
    staged=
    [ "$("$BIN_DIR/sshtui" --version)" = "sshtui $VERSION" ] \
        || err "The installed executable did not report sshtui $VERSION."

    info "Installed sshtui $VERSION to $BIN_DIR/sshtui"
    if ! command -v ssh >/dev/null 2>&1; then
        printf 'warning: ssh was not found on PATH. Install the OpenSSH client before running sshtui.\n' >&2
    fi
    print_path_hint
}

main "$@"
