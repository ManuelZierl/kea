# Generic selection Copy

AGENTS.md: Copy means focused selection; whole document/view/block export is explicit. Review a generic Action::Copy arm that directly routes to copy_document or screen().text() in actions.rs. The existing Action::CopyDocument arm is a legitimate explicit export; component Copy and terminal selection copy are separate. A structural match of an isolated Rust match arm failed the positive fixture in wt 0.0.1; this detector uses a narrow textual arm match. It cannot trace a helper's body or fallback reached indirectly.
