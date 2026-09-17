# Kea documentation

Start with the [project README](../README.md) for setup, shortcuts and configuration.
Use the [human desktop checklist](manual-testing.md) to validate real keyboard,
mouse, clipboard, IME and TUI interactions.
These documents describe the current product; superseded mode/selection proposals
are not part of the active documentation.

## Product and interaction

- [Interaction contract](unified-session.md) — terminal/composer ownership,
  explicit Run and Send actions, focus, completion and cwd.
- [Terminal text selection](terminal-text-selection.md) — local selection,
  keyboard and mouse routing, caret lifecycle and compatibility boundaries.
- [Terminal compatibility](terminal-compatibility-alpha.md) — alpha contracts,
  representative applications and platform validation limits.
- [Session persistence](session-persistence.md) — temporary/saved sessions,
  retention, failure states and acceptance criteria.
- [Roadmap](roadmap.md) — implemented capabilities and remaining work.

## Implementation references

- [Architecture](architecture.md) — crate responsibilities, canonical data and
  live/history isolation.
- [Editor integration](editor-integration.md) — component/platform text services,
  focus, read-only output and OS acceptance boundaries.
- [Command metadata protocol](document-protocol.md) — shell-provided OSC command
  boundaries and trust model.
- [Recording format](recording-format.md) — binary v1 layout and validation.

Automated checks are listed in the [development section](../README.md#development-validation).
A green build or unit-test run does not establish real keyboard, IME, clipboard,
TUI or assistive-technology compatibility.
