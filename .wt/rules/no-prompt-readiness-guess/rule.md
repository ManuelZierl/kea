# Explicit readiness

AGENTS.md: do not infer readiness from `$ ` or `> ` prompt text. Review these exact prompt-character comparisons in live input context and submit/workspace code. The actual context parser's `pending.ends_with(b"\x1b\\")` checks an OSC terminator, not a prompt. Idle-timer heuristics, regexes and other prompt shapes are not expressible here without a broad noisy search; inspect those separately.
