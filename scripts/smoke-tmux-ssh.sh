#!/usr/bin/env bash
# Real loopback SSH + separate tmux sockets; never uses the user's hosts or server.
set -Eeuo pipefail
source scripts/smoke-input.sh
unset TMUX
mkdir -p smoke-artifacts
export XDG_RUNTIME_DIR="$(mktemp -d)"
chmod 700 "$XDG_RUNTIME_DIR"
export KEA_SETTINGS="$XDG_RUNTIME_DIR/settings.conf"
export KEA_KEYBINDINGS="$XDG_RUNTIME_DIR/keybindings.conf"
export KEA_MEMORY_DIR="$XDG_RUNTIME_DIR/memories"
export TMUX_TMPDIR="$XDG_RUNTIME_DIR/local"
mkdir -p "$TMUX_TMPDIR" "$XDG_RUNTIME_DIR/remote" "$XDG_RUNTIME_DIR/bin"
printf 'theme = dark\ncheck_for_updates = false\n' > "$KEA_SETTINGS"
: > "$KEA_KEYBINDINGS"
pid= sshd_pid= window=
cleanup() {
    local status=$?
    if [[ "$status" -ne 0 ]]; then
        [[ -z "$window" ]] || import -silent -window "$window" smoke-artifacts/tmux-ssh-failure.png || true
        cat "$XDG_RUNTIME_DIR/sshd.log" >&2 || true
    fi
    [[ -z "$pid" ]] || kill "$pid" 2>/dev/null || true
    [[ -z "$pid" ]] || wait "$pid" 2>/dev/null || true
    tmux kill-server 2>/dev/null || true
    TMUX_TMPDIR="$XDG_RUNTIME_DIR/remote" tmux kill-server 2>/dev/null || true
    [[ -z "$sshd_pid" ]] || kill "$sshd_pid" 2>/dev/null || true
    [[ -z "$sshd_pid" ]] || wait "$sshd_pid" 2>/dev/null || true
    rm -rf "$XDG_RUNTIME_DIR"
    exit "$status"
}
trap cleanup EXIT
trap 'echo "SSH tmux smoke failed at line $LINENO: $BASH_COMMAND" >&2' ERR
ssh_bin=$(command -v ssh)
sshd_bin=${SSHD_BIN:-/usr/sbin/sshd}
ssh-keygen -q -t ed25519 -N '' -f "$XDG_RUNTIME_DIR/host"
ssh-keygen -q -t ed25519 -N '' -f "$XDG_RUNTIME_DIR/client"
port=$(python3 - <<'PY'
import socket
with socket.socket() as sock:
    sock.bind(('127.0.0.1', 0))
    print(sock.getsockname()[1])
PY
)
cat > "$XDG_RUNTIME_DIR/sshd.conf" <<EOF
Port $port
ListenAddress 127.0.0.1
HostKey $XDG_RUNTIME_DIR/host
AuthorizedKeysFile $XDG_RUNTIME_DIR/client.pub
PidFile $XDG_RUNTIME_DIR/sshd.pid
PasswordAuthentication no
KbdInteractiveAuthentication no
UsePAM no
StrictModes no
AllowUsers $(id -un)
SetEnv TMUX_TMPDIR=$XDG_RUNTIME_DIR/remote LC_ALL=C
EOF
read -r key_type key_data _ < "$XDG_RUNTIME_DIR/host.pub"
printf '[127.0.0.1]:%s %s %s\n' "$port" "$key_type" "$key_data" > "$XDG_RUNTIME_DIR/known_hosts"
cat > "$XDG_RUNTIME_DIR/ssh.conf" <<EOF
Host kea-fixture
    HostName 127.0.0.1
    Port $port
    User $(id -un)
    IdentityFile $XDG_RUNTIME_DIR/client
    IdentitiesOnly yes
    UserKnownHostsFile $XDG_RUNTIME_DIR/known_hosts
    StrictHostKeyChecking yes
    BatchMode yes
    LogLevel ERROR
EOF
# The application still invokes system SSH normally; this fixture isolates config.
cat > "$XDG_RUNTIME_DIR/bin/ssh" <<EOF
#!/usr/bin/env bash
exec "$ssh_bin" -F "$XDG_RUNTIME_DIR/ssh.conf" "\$@"
EOF
chmod 700 "$XDG_RUNTIME_DIR/bin/ssh"
export PATH="$XDG_RUNTIME_DIR/bin:$PATH"
"$sshd_bin" -D -e -f "$XDG_RUNTIME_DIR/sshd.conf" > "$XDG_RUNTIME_DIR/sshd.log" 2>&1 & sshd_pid=$!
python3 - "$port" <<'PY'
import socket, sys, time
for _ in range(100):
    try:
        with socket.create_connection(('127.0.0.1', int(sys.argv[1])), timeout=.1):
            break
    except OSError:
        time.sleep(.05)
else:
    raise AssertionError('Isolated sshd did not start')
PY
ssh -o ConnectTimeout=5 kea-fixture true
tmux -f /dev/null new-session -d -s local-fixture 'exec bash --noprofile --norc'
TMUX_TMPDIR="$XDG_RUNTIME_DIR/remote" tmux -f /dev/null new-session -d -s remote-fixture 'exec bash --noprofile --norc'
remote_clients() { TMUX_TMPDIR="$XDG_RUNTIME_DIR/remote" tmux display-message -p -t remote-fixture '#{session_attached}'; }
assert_remote_clients() {
    for _ in $(seq 1 50); do
        [[ "$(remote_clients)" == "$1" ]] && return
        sleep .1
    done
    echo "Expected $1 remote tmux clients; got $(remote_clients)" >&2
    return 1
}
./target/debug/kea --direct -- python3 scripts/terminal-fixture.py smoke-artifacts/tmux-ssh-input.bin > smoke-artifacts/tmux-ssh.log 2>&1 & pid=$!
for _ in $(seq 1 100); do
    kill -0 "$pid"
    window=$(xdotool search --onlyvisible --name '^Kea$' 2>/dev/null | tail -1 || true)
    [[ -n "$window" ]] && break
    sleep .1
done
[[ -n "$window" ]]
xdotool windowfocus --sync "$window"
sleep 1
smoke_click 995 51
sleep .5
smoke_click 885 95 # SSH…
sleep .3
import -silent -window "$window" smoke-artifacts/tmux-ssh-host-input.png
xdotool type --clearmodifiers --delay 50 kea-fixture
xdotool key --clearmodifiers Return
sleep 1
import -silent -window "$window" smoke-artifacts/tmux-ssh-sessions.png
smoke_click 935 295 # Open remote session.
assert_remote_clients 1
[[ "$(tmux display-message -p -t local-fixture '#{session_attached}')" == 0 ]]
[[ ! -s smoke-artifacts/tmux-ssh-input.bin ]]
smoke_click 130 85
xdotool key --clearmodifiers ctrl+shift+w
sleep .3
xdotool key --clearmodifiers Return
assert_remote_clients 0
TMUX_TMPDIR="$XDG_RUNTIME_DIR/remote" tmux has-session -t remote-fixture
tmux has-session -t local-fixture
smoke_click 995 51
sleep .5
smoke_click 997 295 # Kill is scoped to the displayed SSH host.
sleep .3
import -silent -window "$window" smoke-artifacts/tmux-ssh-kill-confirm.png
xdotool key --clearmodifiers Escape
TMUX_TMPDIR="$XDG_RUNTIME_DIR/remote" tmux has-session -t remote-fixture
smoke_click 997 295
sleep .3
xdotool key --clearmodifiers Return
for _ in $(seq 1 50); do
    if ! TMUX_TMPDIR="$XDG_RUNTIME_DIR/remote" tmux has-session -t remote-fixture 2>/dev/null; then
        tmux has-session -t local-fixture
        [[ ! -s smoke-artifacts/tmux-ssh-input.bin ]]
        echo 'Real SSH discovery in C locale, attach, detach-only close, confirmed remote Kill, cancellation, local/remote isolation and no underlying PTY input passed.'
        exit 0
    fi
    sleep .1
done
echo 'Confirmed remote Kill did not terminate the disposable session' >&2
exit 1
