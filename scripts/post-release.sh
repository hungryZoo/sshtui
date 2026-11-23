#!/usr/bin/env bash
# Update the Homebrew tap after publishing, using the developer's gh login.
set -euo pipefail

cd "$(dirname "$0")/.."
# shellcheck source=scripts/meta.sh
source scripts/meta.sh

if [[ "$#" -gt 1 ]]; then
    echo "Usage: $0 [VERSION]" >&2
    exit 1
fi

version="${1:-$VERSION}"
version="${version#v}"
tag="v$version"
tap_repo=hungryZoo/homebrew-tap
formula=Formula/sshtui.rb
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

gh release download "$tag" --repo hungryZoo/sshtui --pattern SHA256SUMS -O "$work/SHA256SUMS"
gh repo clone "https://github.com/$tap_repo.git" "$work/tap" -- --quiet --depth 1
scripts/update-formula.sh "$version" "$work/SHA256SUMS" "$work/tap/$formula"

git -C "$work/tap" add -- "$formula"
if git -C "$work/tap" diff --cached --quiet -- "$formula"; then
    echo "The formula already points at $tag."
else
    git -C "$work/tap" commit -m "sshtui $version" -- "$formula"
    git -C "$work/tap" -c 'credential.helper=' -c 'credential.helper=!gh auth git-credential' push origin HEAD
fi
