# Kea

A Rust terminal with a rewindable screen history, designed as an embeddable engine plus a small GPUI desktop application.

The working name is **Kea**. This repository is being bootstrapped with the first end-to-end implementation: PTY output capture, terminal emulation, read-only history navigation, and return to the still-running live terminal.

The core must remain independent of GPUI, Zed, operating systems and shells. Replay is observation, never command re-execution. Raw keyboard input is not recorded.
