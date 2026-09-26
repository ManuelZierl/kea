# Changelog

## 0.0.1-alpha.2 — 2026-09-26

- Add independent terminal tabs with per-tab processes, drafts, history navigation,
  completion, recordings and input ownership; support switching, reordering and
  confirmed closing without interrupting other tabs.
- Add a native Windows application icon and verify the embedded icon and GUI subsystem.
- Preserve the block inspector's reading position when new blocks arrive, including
  wheel-only reading without a selection; explicit Latest resumes following output.
- Label the shell directory as current only at the explicitly reported local prompt.
- Add repository invariant rules with reviewed evidence and a checksum-pinned CI tool.

- Add opt-in confirmation before forwarding Ctrl-C, with per-terminal overrides
  and protection against stale targets and repeated confirmation keys.
- Add independently submitted composer sections, split/merge/navigation actions,
  bounded structural undo, and explicit selection submission that preserves its draft.
- Suggest nonexecuting editor actions for separators, code fences, placeholders,
  and configurable shorthand. Text remains literal until a transformation is accepted.
  See [Composer sections and actions](docs/composer-workflow.md) for controls and
  the desktop acceptance checklist.
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

### Validation and known limits

- Linux automated coverage includes portable engines, application/component tests,
  warnings-denied Clippy, and isolated graphical acceptance for terminal tabs,
  guarded composer actions, terminal/editor routing, completion, history and replay.
- Windows application tests/build and macOS portable tests are covered by hosted CI.
  Real Windows/macOS graphical interaction, platform IMEs, accessibility, SSH/TUI
  combinations and long-session behavior remain separate acceptance work.
- Release archives are unsigned; native installers, advanced image protocols and
  higher extended-keyboard protocol levels remain unavailable. Recordings, saved
  history and memories are bounded and unencrypted.

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
