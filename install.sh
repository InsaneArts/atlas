#!/bin/sh
# Install or update Atlas on macOS.
#
#   curl -fsSL https://atlas.insanearts.io/install.sh | sh
#
# Downloads the latest release, verifies its checksum and its Developer ID
# signature, installs Atlas.app into /Applications (where the helper
# account can run it too) and links the `atlas` command into ~/.local/bin.
# An Atlas that is already set up updates itself instead, with atlas update.
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
[ "$(id -u)" -ne 0 ] || fail "run this as your own administrator account, not with sudo"

# The helper account runs the app too, so it lives where every account can
# reach it.
target=/Applications
dest="$target/Atlas.app"
installed="$dest/Contents/MacOS/atlas"

# Whether Atlas was set up on this Mac: a live setup record, setup-<uid>.json.
# Archived records carry a timestamp in their name and do not count.
set_up() {
	for record in "$HOME/Library/Application Support/Atlas"/setup-*.json; do
		uid=${record##*/setup-}
		uid=${uid%.json}
		case $uid in
		'' | *[!0-9]*) ;;
		*) [ -f "$record" ] && return 0 ;;
		esac
	done
	return 1
}

# An Atlas that is set up updates itself: it swaps the app atomically, moves
# the helper, the menu bar and the agent plugins onto the new version, and
# keeps the previous app until that has succeeded.
if [ -x "$installed" ] && set_up; then
	set -- update
	[ -z "${ATLAS_VERSION:-}" ] || set -- "$@" --version "$ATLAS_VERSION"
	[ "${ATLAS_ALLOW_UNNOTARIZED:-}" != 1 ] || set -- "$@" --allow-unnotarized
	say "Atlas is installed and set up; updating it"
	exec "$installed" "$@"
fi

# The command goes in ~/.local/bin/atlas. Only a link into an Atlas.app, a
# link to nothing or nothing at all may be replaced there.
bin="$HOME/.local/bin"
link="$bin/atlas"
if [ -e "$link" ] || [ -L "$link" ]; then
	case $(readlink "$link" 2>/dev/null) in
	*/Atlas.app/*) ;;
	*)
		[ -L "$link" ] && [ ! -e "$link" ] ||
			fail "$link is another program, so it was left alone. Move it aside, then run this again."
		;;
	esac
fi

work=$(mktemp -d "${TMPDIR:-/tmp}/atlas-install.XXXXXX")
staged=""
previous=""
# On any exit, an app swap stopped half way puts the previous app back.
cleanup() {
	if [ -n "$previous" ] && [ -d "$previous" ] && [ ! -e "$dest" ]; then
		mv "$previous" "$dest" || say "Your previous Atlas.app is at $previous"
	fi
	[ -z "$staged" ] || rm -rf "$staged"
	rm -rf "$work"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

base=${ATLAS_DOWNLOAD_BASE:-$SITE/releases/${ATLAS_VERSION:-latest}}
base=${base%/}
# Over https (a local rehearsal may use http), following redirects only to
# https, and never waiting forever.
case $base in
http://*) protocols='=http,https' ;;
*) protocols='=https' ;;
esac
fetch() { # url output
	curl -fsSL --proto "$protocols" --proto-redir =https --connect-timeout 30 --max-time 900 -o "$2" "$1"
}

code=$(curl -sSL --proto "$protocols" --proto-redir =https --connect-timeout 30 --max-time 900 \
	-o "$work/SHA256SUMS" -w '%{http_code}' "$base/SHA256SUMS" 2>/dev/null) || code=000
case $code in
200) ;;
404)
	[ -n "${ATLAS_DOWNLOAD_BASE:-}${ATLAS_VERSION:-}" ] || fail "Atlas has no public release yet. Check $SITE for news."
	fail "there is no release at $base"
	;;
*) fail "could not reach $base (HTTP $code)" ;;
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

[ -w "$target" ] || fail "Atlas.app goes in /Applications; run this as an administrator"
# Stage beside the destination, then swap by rename: never copy over the
# executable a running helper daemon may be using.
staged="$target/.Atlas.app.$$"
rm -rf "$staged"
ditto "$app" "$staged"
if [ -d "$dest" ]; then
	previous="$target/.Atlas.app.previous.$$"
	mv "$dest" "$previous"
fi
mv "$staged" "$dest"
staged=""
if [ -n "$previous" ]; then
	rm -rf "$previous"
	previous=""
fi

mkdir -p "$bin"
ln -sfh "$installed" "$link"

version=$("$installed" --version 2>/dev/null || true)
say "Installed ${version:-Atlas} at $dest"
case ":$PATH:" in
*":$bin:"*) ;;
*) say "Add $bin to your PATH, for example: echo 'export PATH=\"$bin:\$PATH\"' >> ~/.zprofile" ;;
esac
if set_up; then
	# Set up, but the app was gone: the helper still runs the previous
	# executable. Restart the daemon, then the input service, then let setup
	# verify the new build.
	say "Next: atlas restart && atlas start-input && atlas setup --apply"
elif [ -z "${SSH_CONNECTION:-}" ] && [ -z "${ATLAS_NO_OPEN:-}" ]; then
	# The setup window walks the person through the rest.
	say "Opening Atlas to finish setting up (or run atlas init in a terminal)."
	open "$dest" || say "Next: open Atlas from Applications, or run atlas init"
else
	say "Next: open Atlas from Applications, or run atlas init"
fi
