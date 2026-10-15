# tmux attach policy

Attaching runs the tmux client inside Kea's UTF-8 terminal, so it must request
UTF-8 mode (`-u`) just like discovery (see `tmux-metadata-utf8`). Without it, a
remote server started under `LC_ALL=C` makes the attached client draw accented,
CJK and box-drawing characters as `_`.

SSH attach allocates a remote PTY (`-tt`) so password and passphrase prompts
still work, but it must keep `StrictHostKeyChecking=yes`: the manager promises
that only an already-trusted host key is used, and a user `StrictHostKeyChecking
no`/`accept-new` must not make attach weaker than discovery.
