#!/bin/sh
# Install or update Atlas on macOS.
#
#   curl -fsSL https://raw.githubusercontent.com/InsaneArts/atlas/main/install.sh | sh
#
# Downloads the latest release, verifies its checksum and its Developer ID
# signature, installs Atlas.app into /Applications (where the helper
# account can run it too) and links the `atlas` command into ~/.local/bin.
#
# Environment:
#   GITHUB_TOKEN               token for a private repository
#   ATLAS_VERSION         release tag to install (default: latest)
#   ATLAS_DOWNLOAD_BASE   mirror directory URL holding the zip and SHA256SUMS
#   ATLAS_ALLOW_UNNOTARIZED=1  accept a signed but not notarized build
#   ATLAS_NO_OPEN=1        do not open Atlas when installed
set -eu

REPO="InsaneArts/atlas"
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

fetch() { # url output [accept]
	if [ -n "${GITHUB_TOKEN:-}" ]; then
		curl -fsSL -H "Authorization: Bearer $GITHUB_TOKEN" -H "Accept: ${3:-application/octet-stream}" -o "$2" "$1"
	else
		curl -fsSL -H "Accept: ${3:-application/octet-stream}" -o "$2" "$1"
	fi
}

if [ -n "${ATLAS_DOWNLOAD_BASE:-}" ]; then
	base=${ATLAS_DOWNLOAD_BASE%/}
	fetch "$base/SHA256SUMS" "$work/SHA256SUMS"
	name=$(awk 'NR == 1 { print $2 }' "$work/SHA256SUMS")
	case $name in
	Atlas-*-macos.zip) ;;
	*) fail "SHA256SUMS does not name an Atlas archive" ;;
	esac
	case $name in */*) fail "SHA256SUMS does not name an Atlas archive" ;; esac
	fetch "$base/$name" "$work/$name"
else
	tag=${ATLAS_VERSION:-latest}
	if [ "$tag" = latest ]; then
		api="https://api.github.com/repos/$REPO/releases/latest"
	else
		api="https://api.github.com/repos/$REPO/releases/tags/$tag"
	fi
	fetch "$api" "$work/release.json" "application/vnd.github+json" ||
		fail "could not read the release (set GITHUB_TOKEN while the repository is private)"
	# plutil reads JSON: walk the assets for the archive and its sums.
	i=0
	sums="" archive="" name=""
	while asset=$(plutil -extract "assets.$i.name" raw -o - "$work/release.json" 2>/dev/null); do
		url=$(plutil -extract "assets.$i.url" raw -o - "$work/release.json")
		case $asset in
		SHA256SUMS) sums=$url ;;
		Atlas-*-macos.zip) archive=$url name=$asset ;;
		esac
		i=$((i + 1))
	done
	[ -n "$sums" ] && [ -n "$archive" ] || fail "the release has no Atlas archive"
	fetch "$sums" "$work/SHA256SUMS"
	fetch "$archive" "$work/$name"
fi

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
