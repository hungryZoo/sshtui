#!/usr/bin/env bash
# Read the executable name and version from Cargo.toml.
_meta_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

_manifest_value() {
    awk -F '"' -v key="$1" '
        /^\[package\]$/ { package = 1; next }
        /^\[/ && package { exit }
        package && $1 ~ ("^" key "[[:space:]]*=") { print $2; exit }
    ' "$_meta_root/Cargo.toml"
}

BIN="$(_manifest_value name)"
VERSION="$(_manifest_value version)"
if [[ -z "$BIN" || -z "$VERSION" ]]; then
    echo "Could not read the package name and version from Cargo.toml." >&2
    exit 1
fi
export BIN VERSION
