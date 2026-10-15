# Locale-independent tmux metadata

Discovery must request tmux's UTF-8 mode (`-u`). Otherwise a noninteractive SSH
session with a C locale replaces tab separators with underscores and Kea cannot
parse the session list. The unit test exercises a real isolated tmux server with
`LC_ALL=C`; the SSH graphical fixture also sets that locale explicitly. Do not
relax the metadata parser or change the user's remote locale to hide this failure.
