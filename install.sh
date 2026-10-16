#!/bin/sh
# Install or update Atlas on macOS.
#
#   curl -fsSL https://atlas.insanearts.io/install.sh | sh
#
# Downloads the latest release, verifies its checksum and its Developer ID
# signature, installs Atlas.app into /Applications (where the helper
# account can run it too) and links the `atlas` command into ~/.local/bin.
#
# Releases are served from https://atlas.insanearts.io/releases/<version>/,
# each a directory holding SHA256SUMS and the Atlas-<version>-macos.zip it
# names; latest/ is the newest.
#
# Environment:
#   ATLAS_VERSION              release to install (default: latest)
#   ATLAS_DOWNLOAD_BASE        another directory URL holding the zip and SHA256SUMS
#   ATLAS_ALLOW_UNNOTARIZED=1  accept a signed but not notarized build
#   ATLAS_NO_OPEN=1            do not open Atlas when installed
set -eu

SITE="https://atlas.insanearts.io"
IDENTIFIER="com.insanearts.atlas"
TEAM="A474HM679A"
REQUIREMENT="anchor apple generic and identifier \"$IDENTIFIER\" and certificate 1[field.1.2.840.113635.100.6.2.6] and certificate leaf[field.1.2.840.113635.100.6.1.13] and certificate leaf[subject.OU] = \"$TEAM\""

say() { printf '%s\n' "$*"; }
fail() {
	printf 'atlas install: %s\n' "$*" >&2
	exit 1
}

[ "$(uname -s)" = Darwin ] || fail "Atlas runs on macOS only."

work=$(mktemp -d "${TMPDIR:-/tmp}/atlas-install.XXXXXX")
trap 'rm -rf "$work"' EXIT INT TERM

fetch() { # url output
	curl -fsSL -o "$2" "$1"
}

base=${ATLAS_DOWNLOAD_BASE:-$SITE/releases/${ATLAS_VERSION:-latest}}
base=${base%/}
status=$(curl -sSL -o "$work/SHA256SUMS" -w '%{http_code}' "$base/SHA256SUMS" 2>/dev/null) || status=000
case $status in
200) ;;
404)
	[ -n "${ATLAS_DOWNLOAD_BASE:-}${ATLAS_VERSION:-}" ] || fail "Atlas has no public release yet. Check $SITE for news."
	fail "there is no release at $base"
	;;
*) fail "could not reach $base (HTTP $status)" ;;
esac
name=$(awk 'NR == 1 { print $2 }' "$work/SHA256SUMS")
case $name in
Atlas-*-macos.zip) ;;
*) fail "SHA256SUMS does not name an Atlas archive" ;;
esac
case $name in */*) fail "SHA256SUMS does not name an Atlas archive" ;; esac
fetch "$base/$name" "$work/$name" || fail "could not download $base/$name"

say "Verifying $name"
(cd "$work" && grep "  $name\$" SHA256SUMS | shasum -a 256 -c -s) || fail "checksum mismatch for $name"
ditto -x -k "$work/$name" "$work/unpacked"
app="$work/unpacked/Atlas.app"
[ -d "$app" ] || fail "the archive holds no Atlas.app"
codesign --verify --strict --deep "$app" || fail "the app's signature is invalid"
codesign --verify -R "=$REQUIREMENT" "$app/Contents/MacOS/atlas" ||
	fail "the app is not Atlas signed by its Developer ID"
if ! spctl --assess --type execute "$app" 2>/dev/null; then
	[ "${ATLAS_ALLOW_UNNOTARIZED:-}" = 1 ] ||
		fail "the app is not notarized (ATLAS_ALLOW_UNNOTARIZED=1 accepts it)"
	say "Note: this build is signed but not notarized."
fi

# The helper account runs the app too, so it lives where every account can
# reach it.
target=/Applications
[ -w "$target" ] || fail "Atlas.app goes in /Applications; run this as an administrator"
dest="$target/Atlas.app"
# Stage beside the destination, then swap by rename: never copy over the
# executable a running helper daemon may be using.
staged="$target/.Atlas.app.$$"
rm -rf "$staged"
ditto "$app" "$staged"
if [ -d "$dest" ]; then
	previous="$target/.Atlas.app.previous.$$"
	mv "$dest" "$previous"
	mv "$staged" "$dest"
	rm -rf "$previous"
else
	mv "$staged" "$dest"
fi

bin="$HOME/.local/bin"
mkdir -p "$bin"
ln -sf "$dest/Contents/MacOS/atlas" "$bin/atlas"

version=$("$dest/Contents/MacOS/atlas" --version 2>/dev/null || true)
say "Installed ${version:-Atlas} at $dest"
case ":$PATH:" in
*":$bin:"*) ;;
*) say "Add $bin to your PATH, for example: echo 'export PATH=\"$bin:\$PATH\"' >> ~/.zprofile" ;;
esac
if ls "$HOME/Library/Application Support/Atlas"/setup-*.json >/dev/null 2>&1; then
	say "Next: atlas restart && atlas setup --apply"
elif [ -z "${SSH_CONNECTION:-}" ] && [ -z "${ATLAS_NO_OPEN:-}" ]; then
	# The setup window walks the person through the rest.
	say "Opening Atlas to finish setting up (or run atlas init in a terminal)."
	open "$dest" || say "Next: open Atlas from Applications, or run atlas init"
else
	say "Next: open Atlas from Applications, or run atlas init"
fi
