# Fail-open shell integration

Kea's shell integration is an optional observer. Creating the private POSIX
bootstrap file (a full, read-only or missing `$TMPDIR`) or the session token (no
system randomness) can fail; the terminal must still open, untracked, with a
visible warning. Propagating either error with `?` from the spawn path left the
first window or a new tab without any terminal at all.
