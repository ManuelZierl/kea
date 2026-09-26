# PowerShell profile

AGENTS.md: preserve the user's default PowerShell profile. Catch a literal -NoProfile passed to `.arg(...)`, a same-line `.args([...])` array or a PowerShell/pwsh command in startup source, scripts and CI. `shell/mod.rs` is excluded: it permits an explicitly supplied user shell command with -NoProfile, not the default launch. The actual test in crates/kea-app/tests/unit/shell/mod.rs asserts its absence; test sources and assertions in shell smoke scripts are legitimate. A dynamically constructed argument or split-line command is outside this detector.
