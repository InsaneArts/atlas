#!/bin/sh
# Runs the checks every change must pass, in the order CI runs them:
#
#   scripts/check.sh
#
# Rust formatting, lints and tests, then the Markdown and script formatting,
# the Node tests and Swift's source rules, then the Swift tests and lint. Each
# check prints its name first and the script stops at the first that fails.
# The Node checks need `npm ci` once.
set -eu
cd "$(dirname "$0")/.."

check() {
	printf '\n== %s\n' "$*"
	"$@"
}

check cargo fmt --all --check
check cargo clippy --workspace --all-targets --locked -- -D warnings
check cargo test --workspace --locked
check npm run format:check
check npm test
check node scripts/check-swift.mjs
