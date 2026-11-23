#!/usr/bin/env bash
# Package available target binaries and write the release checksum manifest.
set -euo pipefail

cd "$(dirname "$0")/.."
# shellcheck source=scripts/meta.sh
source scripts/meta.sh

DIST_DIR=dist
mkdir -p "$DIST_DIR"

write_checksums() (
    cd "$DIST_DIR"
    shopt -s nullglob dotglob
    checksum_dir="$(mktemp -d .checksums.XXXXXX)"
    trap 'rm -rf "$checksum_dir"' EXIT
    checksums="$checksum_dir/SHA256SUMS"
    if command -v sha256sum >/dev/null 2>&1; then
        hash=(sha256sum)
    else
        hash=(shasum -a 256)
    fi
    for asset in *; do
        [[ -f "$asset" && "$asset" != SHA256SUMS ]] || continue
        "${hash[@]}" -- "$asset"
    done > "$checksums"
    chmod 644 "$checksums"
    mv -f "$checksums" SHA256SUMS
)

case "$#:${1:-}" in
    0:) ;;
    1:--checksums-only) write_checksums; exit ;;
    *) echo "Usage: $0 [--checksums-only]" >&2; exit 1 ;;
esac

package_archive() (
    target="$1"
    extension="$2"
    executable="$3"
    binary="target/$target/release/$executable"
    if [[ ! -f "$binary" ]]; then
        echo "Skipping $target (binary missing)"
        exit 0
    fi

    stage="$(mktemp -d "$DIST_DIR/.stage.XXXXXX")"
    trap 'rm -rf "$stage"' EXIT
    cp "$binary" README.md LICENSE "$stage/"
    archive="$DIST_DIR/$BIN-$VERSION-$target.$extension"
    if [[ "$extension" == zip ]]; then
        if command -v zip >/dev/null 2>&1; then
            (cd "$stage" && zip -q package.zip "$executable" README.md LICENSE)
        else
            # Use relative paths so PowerShell can consume them from Git Bash.
            # PowerShell expands the variables in the command.
            # shellcheck disable=SC2016
            ZIP_STAGE="$stage" ZIP_BIN="$executable" pwsh -NoProfile -NonInteractive -Command '
                $ErrorActionPreference = "Stop"
                $files = $env:ZIP_BIN, "README.md", "LICENSE" | ForEach-Object { Join-Path $env:ZIP_STAGE $_ }
                Compress-Archive -LiteralPath $files -DestinationPath (Join-Path $env:ZIP_STAGE "package.zip")
            '
        fi
        mv -f "$stage/package.zip" "$archive"
    else
        chmod 755 "$stage/$executable"
        COPYFILE_DISABLE=1 tar -czf "$archive" -C "$stage" "$executable" README.md LICENSE
    fi
    echo "$archive"
)

for target in aarch64-apple-darwin x86_64-apple-darwin \
              x86_64-unknown-linux-musl aarch64-unknown-linux-musl \
              armv7-unknown-linux-musleabihf arm-unknown-linux-musleabihf; do
    package_archive "$target" tar.gz "$BIN"
done
for target in x86_64-pc-windows-msvc aarch64-pc-windows-msvc; do
    package_archive "$target" zip "$BIN.exe"
done

# armhf uses the ARMv6 binary for Raspberry Pi 1 and Zero.
for entry in x86_64-unknown-linux-musl:amd64 aarch64-unknown-linux-musl:arm64 \
             arm-unknown-linux-musleabihf:armhf; do
    target="${entry%:*}"
    arch="${entry#*:}"
    [[ -f "target/$target/release/$BIN" ]] || continue
    package="$DIST_DIR/${BIN}_${VERSION}_$arch.deb"
    cargo deb --locked --target "$target" --no-build --no-strip --output "$package"
    cp "$package" "$DIST_DIR/${BIN}_$arch.deb"
done

for arch in x86_64 aarch64; do
    target="$arch-unknown-linux-musl"
    [[ -f "target/$target/release/$BIN" ]] || continue
    package="$DIST_DIR/$BIN-$VERSION-1.$arch.rpm"
    cargo generate-rpm --target "$target" --output "$package"
    cp "$package" "$DIST_DIR/$BIN-$arch.rpm"
done

cp install.sh install.ps1 "$DIST_DIR/"
write_checksums
