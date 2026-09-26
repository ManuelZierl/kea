# Locked Cargo commands

AGENTS.md: commit Cargo.lock and use --locked. Scan one-line Cargo build/test/run/clippy/install commands in CI, scripts and documentation, including README and CONTRIBUTING. Explicit examples are in scope because contributors copy them. Current README's `cargo run --locked --release` is valid. The detector does not parse multiline shell commands or distinguish a quoted example from an executable command.
