#!/usr/bin/env bash
# Prints the Homebrew formula for a published release: packaging/homebrew/flyline.rb.in with the
# version and the sha256 of each archive filled in.
#
#   packaging/homebrew/render.sh v2.1.0 > Formula/flyline.rb
#
# Release binaries are hashed from the release's own `.sha256` assets; the source archive, which
# the release does not hash, is downloaded and hashed here.
set -euo pipefail

tag="${1:?usage: render.sh vX.Y.Z}"
version="${tag#v}"
repo="${FLYLINE_REPO:-aislopware/flyline}"
template="$(dirname "${BASH_SOURCE[0]}")/flyline.rb.in"

fetch() { curl -fsSL --retry 3 "$1"; }
sha256() { if command -v sha256sum >/dev/null; then sha256sum; else shasum -a 256; fi | cut -d' ' -f1; }
check() {
    [[ "$2" =~ ^[0-9a-f]{64}$ ]] || { echo "render.sh: no sha256 for $1" >&2; exit 1; }
}

args=(-e "s|@VERSION@|${version}|g")
for target in aarch64-apple-darwin aarch64-unknown-linux-gnu x86_64-unknown-linux-gnu; do
    asset="libflyline-${tag}-${target}.tar.gz"
    sum="$(fetch "https://github.com/${repo}/releases/download/${tag}/${asset}.sha256" | cut -d' ' -f1)"
    check "${asset}" "${sum}"
    key="$(tr 'a-z-' 'A-Z_' <<<"${target}")"
    args+=(-e "s|@SHA256_${key}@|${sum}|g")
done

sum="$(fetch "https://github.com/${repo}/archive/refs/tags/${tag}.tar.gz" | sha256)"
check "the ${tag} source archive" "${sum}"
args+=(-e "s|@SHA256_SOURCE@|${sum}|g")

formula="$(sed "${args[@]}" "${template}")"
if grep -q '@[A-Z0-9_]*@' <<<"${formula}"; then
    echo "render.sh: placeholders left unfilled:" >&2
    grep -o '@[A-Z0-9_]*@' <<<"${formula}" | sort -u >&2
    exit 1
fi
printf '%s\n' "${formula}"
