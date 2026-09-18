# Kea vendor note

This directory contains `gpui-component` 0.5.1 from crates.io under its
Apache-2.0 license. Kea pins this component because the editor's selection,
composition, undo, read-only selection and search behavior are application
contracts.

Kea carries one local change in `src/input/search.rs`: changing a search query
reveals the active match, and previous/next navigation may scroll in either
direction after manual scrolling. This is implemented inside the component so
Kea does not duplicate search state or mutate the editor caret to control the
viewport.
