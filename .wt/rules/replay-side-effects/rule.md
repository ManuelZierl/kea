# Historical projection effects

AGENTS.md: replay is observation and historical engines cannot send input or mutate host state. Only replay.rs, presentation.rs and the engine projection are selected; session.rs owns both live and historical behavior and is deliberately excluded. A live PTY send there is valid. Checks named effect calls; aliases, indirect calls and protocol replies inside an engine require semantic review.
