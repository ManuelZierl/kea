#!/usr/bin/env bash
# Run the std-only memory tests without downloading GUI dependencies.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
for module in reverse_search reverse_search_store reverse_search_worker; do
    cp "$root/crates/kea-app/src/$module.rs" "$work/$module.rs"
done
cat > "$work/memory_tests.rs" <<'RS'
#![allow(dead_code)]
mod reverse_search;
mod reverse_search_store;
mod reverse_search_worker;
RS
rustc --edition=2021 --test "$work/memory_tests.rs" -o "$work/memory-tests"
"$work/memory-tests"
