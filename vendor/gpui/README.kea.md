# Kea's GPUI patch

This is the **Apache-2.0** `gpui` **0.2.2** crate published on crates.io,
including its original `LICENSE-APACHE`. It is not Zed's GPL editor code.
The root `[patch.crates-io]` keeps every GPUI consumer on this same source.
`Cargo.toml` is the registry-normalized standalone manifest; `Cargo.toml.orig`
records the upstream workspace manifest.

## Local change

`src/platform/linux/x11/client.rs`: enable and verify XKB detectable autorepeat
on the application's connection, and preserve all key events in order. This
replaces upstream's single pending release and 20 ms repeat heuristic:

- Consecutive releases overwrote the pending slot. Releasing Enter then Ctrl
  could leave Kea's physical-key guard latched and swallow the next Submit.
- An autorepeat release/press pair split across polling batches exposed a fake
  physical release, allowing held Enter to confirm a guarded submission.
- Timing-based filtering could also discard a fast genuine release/press pair.

Detectable autorepeat supplies repeated presses without fake releases. Servers
that cannot enable it fail startup with an explicit error; there is no unsafe
timing fallback. Wayland, Windows and macOS paths are unchanged.

The graphical regression in `scripts/smoke-linux.sh` briefly stops only its
isolated fixture process while delivering the two releases, then resumes it.
This forces both into one event batch. The held-chord assertion forbids any PTY
input before physical release; the next guarded submission must deliver exactly
the expected PTY bytes. Repeated focused runs reproduced the split-batch failure
before detectable autorepeat was enabled.

Remove the patch when a compatible upstream release includes the fix, after
rerunning the held-key, guarded-composer, terminal-tab and shared-input suites.
