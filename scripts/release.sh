#!/bin/sh
# Release Atlas as a GitHub release, where install.sh and atlas update
# download it.
#
#   scripts/release.sh 0.2.0
#
# Sets the version in Cargo.toml and Cargo.lock (committing the bump when it
# changes), runs the checks, builds, signs, notarizes and staples Atlas.app
# with cargo xtask dist, then tags v<version>, pushes main and the tag, and
# publishes the zip and SHA256SUMS. Releases are cut from a clean main that
# matches origin/main.
#
# Environment:
#   SIGN_IDENTITY   Developer ID Application identity
#                   (default: Developer ID Application: Techzy LLC (539293JFA3))
#   NOTARY_PROFILE  notarytool keychain profile (default: camus-notary)
set -eu

identity=${SIGN_IDENTITY:-"Developer ID Application: Techzy LLC (539293JFA3)"}
profile=${NOTARY_PROFILE:-camus-notary}
releases="https://github.com/InsaneArts/atlas/releases"

step() { printf '\n> %s\n' "$*"; }
fail() {
	printf 'release: %s\n' "$*" >&2
	exit 1
}

version=${1:-}
[ $# -eq 1 ] || fail "usage: scripts/release.sh X.Y.Z"
expr "$version" : '[0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*$' >/dev/null ||
	fail "the version must be plain numbers such as 0.2.0, not $version"
tag="v$version"
cd "$(dirname "$0")/.."

# The toolchain rust-toolchain.toml pins, with both Mac targets, ahead of any
# other cargo on PATH (Homebrew's ignores the pin and has no x86_64 library).
command -v rustup >/dev/null || fail "rustup is required for the pinned toolchain"
PATH="$(dirname "$(rustup which cargo)"):$PATH"

step "Preflight"
[ "$(git rev-parse --abbrev-ref HEAD)" = main ] || fail "release from main"
[ -z "$(git status --porcelain)" ] || fail "commit or stash your changes first"
git fetch -q origin main
[ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] ||
	fail "main differs from origin/main; pull or push first"
! git rev-parse -q --verify "refs/tags/$tag" >/dev/null || fail "$tag already exists"
gh auth status >/dev/null 2>&1 || fail "gh is not signed in; run gh auth login"
security find-identity -v -p codesigning | grep -qF "\"$identity\"" ||
	fail "the keychain has no signing identity named $identity"
xcrun notarytool history --keychain-profile "$profile" >/dev/null 2>&1 ||
	fail "no notarytool profile named $profile; store it with xcrun notarytool store-credentials"

current=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml)
if [ "$current" != "$version" ]; then
	step "Setting the version to $version"
	sed -i '' "s/^version = \"$current\"$/version = \"$version\"/" Cargo.toml
	cargo update --workspace --offline --quiet
	git commit -q -m "chore: release $version" Cargo.toml Cargo.lock
fi

step "Checks"
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
npm ci --silent
npm run format:check
npm test
node scripts/check-swift.mjs

step "Building, signing and notarizing"
cargo xtask dist --sign "$identity" --notarize "$profile"
dist=target/dist
zip="$dist/Atlas-$version-macos.zip"
[ -f "$zip" ] || fail "cargo xtask dist wrote no $zip"

# The checks install.sh makes, on the archive people download.
step "Checking the archive"
work=$(mktemp -d "${TMPDIR:-/tmp}/atlas-release.XXXXXX")
trap 'rm -rf "$work"' EXIT
(cd "$dist" && shasum -a 256 -c -s SHA256SUMS) || fail "SHA256SUMS does not match $zip"
ditto -x -k "$zip" "$work"
codesign --verify --strict --deep "$work/Atlas.app" || fail "the app's signature is invalid"
spctl --assess --type execute "$work/Atlas.app" || fail "the app is not notarized"
built=$("$work/Atlas.app/Contents/MacOS/atlas" --version | awk '{ print $NF }')
[ "$built" = "$version" ] || fail "the app says $built, not $version"

step "Publishing $tag"
previous=$(git describe --tags --abbrev=0 2>/dev/null || true)
{
	printf '## Install\n\n```sh\ncurl -fsSL https://atlas.insanearts.io/install.sh | sh\n```\n\n'
	printf 'Requires macOS 14 or later. An installed Atlas updates with `atlas update`.\n\n## Changes\n\n'
	if [ -n "$previous" ]; then
		git log --no-merges --pretty='- %s' "$previous..HEAD"
	else
		echo "The first release."
	fi
} >"$work/notes.md"
git tag -a "$tag" -m "Atlas $version"
git push -q origin main "$tag"
gh release create "$tag" "$zip" "$dist/SHA256SUMS" --verify-tag \
	--title "Atlas $version" --notes-file "$work/notes.md"

step "Checking the download"
if curl -fsSL "$releases/latest/download/SHA256SUMS" | grep -q "Atlas-$version-macos.zip"; then
	echo "$releases/latest/download serves $version"
else
	echo "warning: $releases/latest/download does not serve $version yet; a private repository serves nothing"
fi
curl -fsSL https://atlas.insanearts.io/install.sh | cmp -s - install.sh ||
	echo "warning: the site's install.sh differs from this one; deploy atlas-landing"
echo "Released: $releases/tag/$tag"
