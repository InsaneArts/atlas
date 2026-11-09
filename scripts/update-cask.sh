#!/bin/sh
# Point the Homebrew cask in InsaneArts/homebrew-tap at a published release.
#
#   scripts/update-cask.sh 0.2.0
#
# Reads the checksum of Atlas-<version>-macos.zip from the release's
# SHA256SUMS on GitHub, sets the cask's version and sha256, and pushes the
# change to the tap. scripts/release.sh runs it after publishing; run it by
# hand to retry when that step failed. Nothing changes when the cask already
# names the release.
set -eu

tap="InsaneArts/homebrew-tap"
releases="https://github.com/InsaneArts/atlas/releases"

fail() {
	printf 'update-cask: %s\n' "$*" >&2
	exit 1
}

version=${1:-}
[ $# -eq 1 ] || fail "usage: scripts/update-cask.sh X.Y.Z"
expr "$version" : '[0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*$' >/dev/null ||
	fail "the version must be plain numbers such as 0.2.0, not $version"
name="Atlas-$version-macos.zip"

work=$(mktemp -d "${TMPDIR:-/tmp}/atlas-cask.XXXXXX")
trap 'rm -rf "$work"' EXIT

curl -fsSL --proto =https --proto-redir =https --connect-timeout 30 --max-time 120 \
	-o "$work/SHA256SUMS" "$releases/download/v$version/SHA256SUMS" ||
	fail "there is no published release v$version"
sum=$(awk -v name="$name" '$2 == name { print $1 }' "$work/SHA256SUMS")
expr "$sum" : '[0-9a-f]\{64\}$' >/dev/null || fail "the release's SHA256SUMS has no checksum for $name"

gh repo clone "$tap" "$work/tap" -- -q
cask="$work/tap/Casks/atlas.rb"
sed -i '' -e "s/^  version \".*\"$/  version \"$version\"/" -e "s/^  sha256 \".*\"$/  sha256 \"$sum\"/" "$cask"
if ! grep -q "^  version \"$version\"$" "$cask" || ! grep -q "^  sha256 \"$sum\"$" "$cask"; then
	fail "could not set the cask to $version"
fi
if git -C "$work/tap" diff --quiet; then
	echo "The cask already names $version"
	exit 0
fi
git -C "$work/tap" commit -q -m "feat(cask): update atlas to $version" Casks/atlas.rb
git -C "$work/tap" push -q origin HEAD
echo "brew install --cask insanearts/tap/atlas now installs $version"
