# Tokened integration markers

Kea authenticates OSC 777/778/779 integration markers with a per-terminal
session token (remote integrations use the user's remote token). Program output
cannot know the token, so it cannot mark input ready, set cwd/PATH or forge
blocks. A hook that emits `kea;prompt;…` without the token is silently ignored
by live terminals; keep every emitter in the `kea;<token>;<kind>` form.
