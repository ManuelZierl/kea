# Kea vendor note

This directory contains `gpui-component` 0.5.1 from crates.io under its
Apache-2.0 license. Kea pins this component because the editor's selection,
composition, undo, read-only selection and search behavior are application
contracts.

Kea carries two local changes:

- `src/input/search.rs`: changing a search query reveals the active match, and
  previous/next navigation may scroll in either direction after manual scrolling.
- `src/input/state.rs`: mouse-down focuses an input before placing its caret, so
  retained Settings fields receive typed text after a direct click.

These stay inside the component so Kea does not duplicate editor state or patch
focus behavior at each call site.
