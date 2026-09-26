# Contributing to Kea

Thanks for helping make Kea better! Bug reports, documentation improvements,
real-desktop testing and focused code changes are all welcome.

## Where to start

- Search [existing issues](https://github.com/ManuelZierl/kea/issues) before filing
  a bug or suggesting a feature.
- For bugs, include the commit/version, OS, shell or TUI version, exact steps,
  expected behavior and what happened. Use sample text instead of private output.
- For larger changes, open an issue first so we can agree on scope.
- The [roadmap](docs/roadmap.md) and [desktop checklist](docs/manual-testing.md)
  are useful places to find work. Documentation-only contributions are welcome.

## Development

Install stable Rust and the dependencies in the
[source setup](docs/usage.md#run-from-source). Fork the repository, create a
focused branch from **`develop`**, and open your pull request against **`develop`**.
`main` is the release-ready branch; maintainers promote tested changes there.

Read [AGENTS.md](AGENTS.md) for engineering invariants. Changes to execution,
input routing or persistence should also follow the [architecture](docs/architecture.md)
and [interaction contract](docs/unified-session.md).

Run the checks relevant to your change. For Rust changes, the baseline is:

```sh
cargo fmt --all -- --check
cargo test --locked -p kea-core -p kea-document -p kea-alacritty -p kea-pty -p kea-session
cargo clippy --locked -p kea-core -p kea-document -p kea-alacritty -p kea-pty -p kea-session --all-targets -- -D warnings
cargo test --locked -p kea-app --lib
cargo build --locked -p kea-app
wt validate --no-global && wt test --no-global && wt check --no-global
bash scripts/test-reverse-search-core.sh
```

Install [Watchtower wt v0.0.1](https://github.com/ManuelZierl/wt/releases/tag/v0.0.1) to run the repository's `.wt/` invariant checks; CI pins that release and runs them next to Clippy on Linux. Review findings individually with `wt inspect` and `wt review`; a reviewed occurrence reopens when its evidence changes.

`cargo test --locked -p kea-app` also runs the binary host's tests. On Linux,
run the graphical smoke test in an isolated X11 display with the dependencies
and invocation documented in [release validation](docs/releasing.md#local-validation).
CI performs these checks on pushes to `develop`/`main` and pull requests targeting
either branch. Real keyboard, clipboard, IME and accessibility checks remain
separate from automated tests; report what you actually tested.

Keep changes focused, preserve meaningful assertions, and include regression
coverage when fixing behavior. Commit `Cargo.lock` when dependencies or workspace
versions change. Do not include secrets, personal recordings or generated build
output. Vendored dependencies retain their original licenses; document any patch.
The vendored `vendor/gpui-component` is Apache-2.0 licensed (see its
`LICENSE-APACHE`); `.wt/` checks Kea-owned crate sources, not that vendored code.

## Documentation

The documentation site uses Just the Docs and Markdown under `docs/`. Each page
has a title and navigation order in YAML front matter. Use relative `.md` links;
Jekyll converts them for the website while they remain usable on GitHub.
The Pages workflow builds PRs and deploys `main`; see
[the release guide](docs/releasing.md#documentation-site) for setup and preview.

## Pull requests

Describe the problem, the change and your validation. Screenshots using sample
data help with UI changes. Small, reviewable pull requests are easier to merge;
you do not need to solve every related problem at once.

Please follow the [Code of Conduct](CODE_OF_CONDUCT.md). Report vulnerabilities
using [SECURITY.md](SECURITY.md), rather than posting exploit details publicly.
