# Changelog

## Unreleased

- Unify composer submission under Ctrl+Enter with exact-draft/context confirmation
  for unknown input; retain Ctrl+Shift+Enter as a guarded compatibility alias.
- Replace authored-command eval wrappers with observational shell hooks. Install
  PowerShell hooks at startup rather than typing implementation code into its editor.
- Add scoped active-input reports, explicit nested-shell integration export and
  bounded opt-in structured completion providers. Native terminal Tab is unchanged.
- Add horizontal/visual-row completion navigation and stale-context rejection.
- Focus clicked input fields explicitly and add isolated shortcut recording.
- Write recording format v2 with separate submission metadata; keep v1 reading.
  Older Kea releases cannot read newly written v2 recordings.

## 0.0.1-alpha.1 — 2026-09-19

First early-alpha release of Kea, a terminal workspace with a multiline editor
for authored input.

### Highlights

- One terminal session with a persistent composer and explicit Run in shell /
  Send to app actions.
- Draft recall, searchable submitted input and named memories, local shell
  completion, configurable shortcuts and appearance.
- Terminal selection and scrollback, optional command blocks, explicit session
  saving and read-only replay.
- Linux desktop smoke coverage and cross-platform CI, unsigned release archives,
  and a Just the Docs documentation site.

### Known limits

- Real Windows/macOS/Linux desktop, IME, accessibility and shell/TUI acceptance
  remains distinct from build and automated-test coverage. See the compatibility
  matrix in the documentation before treating an environment as validated.
- No native installers or code signing. Advanced image protocols and higher
  extended-keyboard levels are not supported.
- History is bounded; backward replay seeks start from the beginning. Persisted
  sessions, history and memories are unencrypted and may contain sensitive data.
- Bash Run commands are retained by Kea's draft recall but excluded from Bash's
  own history with their internal driver wrappers. Other shells do not yet have
  equivalent wrapper-history suppression.
