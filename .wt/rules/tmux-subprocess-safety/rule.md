# Tmux subprocess safety

The tmux adapter owns external subprocesses. Direct `Command::output()` can
buffer unbounded stdout/stderr, and unsafe process control bypasses the
workspace's `unsafe_code = "forbid"` contract. Keep lifecycle handling behind
the bounded runner and safe platform wrappers.
