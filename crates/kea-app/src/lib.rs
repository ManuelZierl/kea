//! Reusable host adapters; platform/editor functionality stays in upstream components.
pub mod command_editor;
pub mod draft_history;
pub mod input;
pub mod keybindings;
pub mod logo;
mod logo_frames;
pub mod playback;
pub mod session_files;
pub mod settings;
pub mod shell;
pub mod terminal_mouse;
pub mod terminal_recovery;
pub mod terminal_selection;

pub mod completion;
pub mod reverse_search;
mod reverse_search_store;
pub mod reverse_search_view;
mod reverse_search_worker;
