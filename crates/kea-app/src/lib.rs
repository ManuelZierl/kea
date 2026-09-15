//! Testable interaction logic for the standalone Kea application.
//!
//! The GPUI window itself stays in `main.rs`; keeping these modules in a small
//! library lets `cargo test -p kea-app` exercise editor, keybinding, terminal
//! input, and shell-boundary behavior without compiling the large GPUI render
//! tree as a Rust test harness.

pub mod command_editor;
pub mod input;
pub mod keybindings;
pub mod shell;
