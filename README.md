<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/kea-logo-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="assets/kea-logo.svg">
    <img alt="Kea" src="assets/kea-logo.svg" width="220">
  </picture>
</p>

<h1 align="center">Kea</h1>
<p align="center"><strong>A terminal workspace with a real editor for input.</strong></p>

Kea keeps a terminal and a multiline editor visible together over one session.
Compose a command or prompt, run it in your shell or send it to a terminal app,
then recall and edit it without losing your next draft. Your shell, SSH, Vim,
REPLs and other terminal applications keep their native input behavior.

**Early alpha:** `v0.0.1-alpha.1` is the first planned release. Expect rough edges;
real-platform compatibility is still being validated. See the
[compatibility checklist](docs/terminal-compatibility-alpha.md) and
[known limits](docs/usage.md#limits-and-privacy).

[Documentation](https://manuelzierl.github.io/kea/) ·
[Releases](https://github.com/ManuelZierl/kea/releases) ·
[Contributing](CONTRIBUTING.md) · [Roadmap](docs/roadmap.md)

## Why Kea?

- **Edit before you send.** Multiline composition, selection, undo, search,
  syntax highlighting and local shell completion.
- **Keep the terminal native.** Switch focus to interact directly with your shell
  or TUI. One session, with explicit **Run in shell** and **Send to app** actions.
- **Reuse authored input.** Recall submitted drafts or search history and named
  memories. Recall never executes automatically.
- **Inspect output.** Select and copy terminal text, browse bounded scrollback,
  or save and replay a session. Command blocks are optional.
- **Choose what persists.** Sessions start temporary. Saving is explicit;
  recordings and persisted history are unencrypted and may contain secrets.

## Getting started

Release archives are unsigned standalone binaries for Linux, macOS and Windows;
native installers are not available yet. Download a matching archive from
[Releases](https://github.com/ManuelZierl/kea/releases) when published, extract it,
and run `kea` (`kea.exe` on Windows). Linux needs a Vulkan-capable graphics stack
and the system libraries listed in the [source setup](docs/usage.md#run-from-source).

To build from source, install stable Rust and the platform build dependencies
described in the [user guide](docs/usage.md#run-from-source), then:

```sh
git clone https://github.com/ManuelZierl/kea.git
cd kea
cargo run --locked --release
```

### Everyday shortcuts

| Action | Default |
| --- | --- |
| Newline in the composer | Enter |
| Run draft in the integrated shell | Ctrl+Enter |
| Send draft to the current terminal app | Ctrl+Shift+Enter |
| Switch terminal ⇄ composer | Ctrl+L / Cmd+L |
| Recall previous / next draft | Ctrl+Up / Ctrl+Down |
| Search submitted input and memories | Ctrl+R |
| Complete in the composer | Tab |

**Run in shell** requires an explicitly reported shell prompt. **Send to app**
sends literal text followed by Enter; multiline sends require bracketed paste.
Shortcuts and appearance are configurable in Settings. The
[user guide](docs/usage.md) covers selection, configuration, shell integration,
history and privacy in detail.

## Contributing

Contributions are welcome: bug reports, documentation, desktop testing and code.
Start with [CONTRIBUTING.md](CONTRIBUTING.md), especially the platform testing
notes. Open pull requests against **`develop`**; **`main`** holds release-ready
work. Please follow our [Code of Conduct](CODE_OF_CONDUCT.md).

For vulnerabilities, see [SECURITY.md](SECURITY.md). For design and implementation,
see the [architecture](docs/architecture.md) and [documentation index](docs/index.md).

## License

[MIT](LICENSE). Dependencies retain their licenses. Kea is independent, not an
official Zed feature or extension.
