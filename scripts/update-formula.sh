#!/usr/bin/env bash
# Generate the complete Homebrew formula from the release checksum manifest.
set -euo pipefail

if [[ "$#" -ne 3 ]]; then
    echo "Usage: $0 VERSION SHA256SUMS FORMULA_PATH" >&2
    exit 1
fi

VERSION="${1#v}"
SUMS="$2"
FORMULA="$3"
if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$ ]]; then
    echo "Invalid release version: $VERSION" >&2
    exit 1
fi

sha_for() {
    local asset="sshtui-$VERSION-$1.tar.gz" digest
    digest="$(LC_ALL=C awk -v asset="$asset" '
        { sub(/\r$/, ""); sub(/^\*/, "", $2) }
        $2 == asset && length($1) == 64 && $1 ~ /^[[:xdigit:]]+$/ {
            print tolower($1)
        }
    ' "$SUMS")"
    if [[ "${#digest}" -ne 64 ]]; then
        echo "Expected one SHA256 checksum for $asset in $SUMS." >&2
        return 1
    fi
    printf '%s\n' "$digest"
}

mac_arm="$(sha_for aarch64-apple-darwin)"
mac_intel="$(sha_for x86_64-apple-darwin)"
linux_arm="$(sha_for aarch64-unknown-linux-musl)"
linux_intel="$(sha_for x86_64-unknown-linux-musl)"
base="https://github.com/hungryZoo/sshtui/releases/download/v$VERSION/sshtui-$VERSION"

mkdir -p "$(dirname "$FORMULA")"
cat > "$FORMULA" <<FORMULA_EOF
class Sshtui < Formula
  desc "Terminal interface for the system SSH client"
  homepage "https://github.com/hungryZoo/sshtui"
  version "$VERSION"
  license "MIT"

  on_macos do
    on_arm do
      url "$base-aarch64-apple-darwin.tar.gz"
      sha256 "$mac_arm"
    end
    on_intel do
      url "$base-x86_64-apple-darwin.tar.gz"
      sha256 "$mac_intel"
    end
  end

  on_linux do
    on_arm do
      url "$base-aarch64-unknown-linux-musl.tar.gz"
      sha256 "$linux_arm"
    end
    on_intel do
      url "$base-x86_64-unknown-linux-musl.tar.gz"
      sha256 "$linux_intel"
    end
  end

  def install
    bin.install "sshtui"
  end

  test do
    assert_equal "sshtui #{version}", shell_output("#{bin}/sshtui --version").strip
  end
end
FORMULA_EOF
echo "$FORMULA now points at v$VERSION"
