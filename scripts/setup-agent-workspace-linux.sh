#!/usr/bin/env bash
set -Eeuo pipefail

version='v0.3.2'
repository='https://github.com/agent-sh/agent-workspace-linux'

case "$(uname -m)" in
  x86_64)
    target='x86_64-unknown-linux-gnu'
    checksum='130434f781370462c30fc793710528cbd72358d2e9b9976d56606a1d8d649f5d'
    ;;
  aarch64 | arm64)
    target='aarch64-unknown-linux-gnu'
    checksum='ce016b252513b967424dc1f54145b4d0f8306cd0073b24a993a63bebbb9b4458'
    ;;
  *)
    printf 'Unsupported architecture: %s\n' "$(uname -m)" >&2
    exit 1
    ;;
esac

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
install_dir="$repo_root/.opencode/bin"
asset="agent-workspace-linux-$target"
release_url="$repository/releases/download/$version"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

mkdir -p "$install_dir"
curl --fail --location --output "$tmp_dir/$asset" "$release_url/$asset"
printf '%s  %s\n' "$checksum" "$tmp_dir/$asset" | sha256sum --check --status
install -m 0755 "$tmp_dir/$asset" "$install_dir/agent-workspace-linux"

printf 'Installed agent-workspace-linux %s at %s\n' \
  "$version" "$install_dir/agent-workspace-linux"
doctor_output="$("$install_dir/agent-workspace-linux" doctor)"
printf '%s\n' "$doctor_output"
if [[ "$doctor_output" != *'"ready_for_x11_workspace": true'* ]]; then
  printf 'agent-workspace-linux is installed, but its X11 workspace is not ready.\n' >&2
  exit 1
fi
