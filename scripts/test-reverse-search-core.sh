#!/usr/bin/env bash
# Run the std-only memory tests without downloading GUI dependencies.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
rustc --edition=2021 --test "$root/crates/kea-app/tests/standalone/reverse_search.rs" -o "$work/memory-tests"
"$work/memory-tests"
