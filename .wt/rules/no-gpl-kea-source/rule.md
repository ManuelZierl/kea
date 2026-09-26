# Kea source license headers

AGENTS.md: do not copy GPL Zed code into MIT Kea crates. Detect explicit GPL/AGPL SPDX and Zed Industries/Zed team copyright header lines in Kea-owned Rust source. Vendor is excluded: vendor/gpui-component declares Apache-2.0 in its Cargo.toml and ships LICENSE-APACHE; its license is documented in CONTRIBUTING.md. This is header detection, not a provenance audit of copied code without headers.
