# PTY teardown input ownership

`portable-pty`'s Unix master writer sends a newline and VEOF when dropped.
`Pty::drop` must therefore retain the input sender in its asynchronous teardown
worker until the child has been killed and waited for. A discarded sender can
execute a pending terminal line or log out the last tmux pane.
