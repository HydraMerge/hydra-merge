#!/bin/sh
# Fills the package templates in packaging/ for one release.
#
#   sh scripts/render-packages.sh <version> <SHA256SUMS> <out-dir> [release-date]
#
# <version> is without the leading v (0.1.19). <SHA256SUMS> is the file of that release. The
# release workflow runs this after publishing; run it by hand to look at the result.
#
# Writes under <out-dir>:
#   homebrew/Casks/hydra-merge.rb      macOS cask (both architectures)
#   homebrew/Formula/hydra-merge.rb    Linux formula (both architectures)
#   winget/manifests/h/HydraMerge/HydraMerge/<version>/*.yaml
#                                      only when the release has the Windows .zip archives
set -eu

if [ "$#" -lt 3 ]; then
    echo "usage: $0 <version> <SHA256SUMS> <out-dir> [release-date]" >&2
    exit 2
fi
version=$1
sums=$2
out=$3
date=${4:-$(date -u +%Y-%m-%d)}
here=$(cd "$(dirname "$0")/.." && pwd)
templates="$here/packaging"

case "$version" in
    [0-9]*.[0-9]*.[0-9]*) ;;
    *) echo "version must look like 1.2.3, got: $version" >&2; exit 2 ;;
esac

# The checksum of one archive of this release, or nothing when the release has no such file.
sum_of() {
    awk -v name="hydra-merge-v$version-$1" '$2 == name { print $1 }' "$sums"
}

# Like sum_of, but a missing archive is an error.
need() {
    value=$(sum_of "$1")
    if [ -z "$value" ]; then
        echo "no checksum for hydra-merge-v$version-$1 in $sums" >&2
        exit 1
    fi
    printf '%s' "$value"
}

fill() {
    # fill <template> <target>: replaces every @NAME@ placeholder.
    mkdir -p "$(dirname "$2")"
    sed \
        -e "s|@VERSION@|$version|g" \
        -e "s|@DATE@|$date|g" \
        -e "s|@SHA_AARCH64_APPLE@|$sha_aarch64_apple|g" \
        -e "s|@SHA_X86_64_APPLE@|$sha_x86_64_apple|g" \
        -e "s|@SHA_AARCH64_LINUX@|$sha_aarch64_linux|g" \
        -e "s|@SHA_X86_64_LINUX@|$sha_x86_64_linux|g" \
        -e "s|@SHA_AARCH64_WINDOWS@|$sha_aarch64_windows|g" \
        -e "s|@SHA_X86_64_WINDOWS@|$sha_x86_64_windows|g" \
        "$1" > "$2"
}

sha_aarch64_apple=$(need aarch64-apple-darwin.zip)
sha_x86_64_apple=$(need x86_64-apple-darwin.zip)
sha_aarch64_linux=$(need aarch64-unknown-linux-gnu.tar.gz)
sha_x86_64_linux=$(need x86_64-unknown-linux-gnu.tar.gz)
sha_aarch64_windows=
sha_x86_64_windows=

fill "$templates/homebrew/hydra-merge.cask.rb.in" "$out/homebrew/Casks/hydra-merge.rb"
fill "$templates/homebrew/hydra-merge.formula.rb.in" "$out/homebrew/Formula/hydra-merge.rb"
echo "homebrew: $version"

win_arm=$(sum_of aarch64-pc-windows-msvc.zip)
win_x64=$(sum_of x86_64-pc-windows-msvc.zip)
if [ -n "$win_arm" ] && [ -n "$win_x64" ]; then
    # winget lists checksums in capitals.
    sha_aarch64_windows=$(printf '%s' "$win_arm" | tr 'a-f' 'A-F')
    sha_x86_64_windows=$(printf '%s' "$win_x64" | tr 'a-f' 'A-F')
    dir="$out/winget/manifests/h/HydraMerge/HydraMerge/$version"
    fill "$templates/winget/version.yaml.in" "$dir/HydraMerge.HydraMerge.yaml"
    fill "$templates/winget/installer.yaml.in" "$dir/HydraMerge.HydraMerge.installer.yaml"
    fill "$templates/winget/locale.yaml.in" "$dir/HydraMerge.HydraMerge.locale.en-US.yaml"
    echo "winget: $version"
else
    echo "winget: skipped, $sums lists no Windows .zip archives"
fi
