# Completion blocking operations

AGENTS.md: completion work must not block the UI thread indefinitely. Catch named synchronous file, network, channel or sleep calls in composer and completion/provider code. In Kea, editor/completion.rs scans the local filesystem and provider.rs connects/opens files, but composer.rs dispatches suggest/provider completion to a worker; raw-positive does not automatically mean violation. Provider configuration loads at startup. `try_recv` is nonblocking. Aliases and indirect calls are not detected.
