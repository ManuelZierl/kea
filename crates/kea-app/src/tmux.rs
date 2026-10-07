//! Local tmux discovery and lifecycle commands.
//!
//! A tmux session is a persistent external resource. Kea may attach a terminal
//! client to it, but the tmux server remains the source of truth and can outlive
//! any Kea tab.

use anyhow::{bail, Context as _, Result};
#[cfg(unix)]
use nix::{
    fcntl::{fcntl, FcntlArg, OFlag},
    sys::signal::{killpg, Signal},
    unistd::Pid,
};
use std::{io, process::Output, time::Duration};
#[cfg(unix)]
use std::{
    io::Read,
    os::fd::AsRawFd,
    os::unix::process::CommandExt,
    process::{Child, ChildStderr, ChildStdout, Command, Stdio},
    thread,
    time::Instant,
};

const LIST_FORMAT: &str = "#{session_id}\t#{session_name}\t#{session_windows}\t#{session_attached}\t#{pid}\t#{start_time}\t#{session_created}";
const MAX_SESSIONS: usize = 256;
const MAX_ROW_BYTES: usize = 4096;
const MAX_COMMAND_OUTPUT: usize = 64 * 1024;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(2);
const KILL_SUCCESS_MARKER: &str = "KEA_TMUX_KILL_CONFIRMED";
const STALE_SESSION_MESSAGE: &str = "Kea: tmux session changed; refresh and retry";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TmuxSession {
    id: String,
    name: String,
    windows: u32,
    attached_clients: u32,
    server_pid: u32,
    server_start_time: u64,
    session_created: u64,
}

impl TmuxSession {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn windows(&self) -> u32 {
        self.windows
    }

    pub fn attached_clients(&self) -> u32 {
        self.attached_clients
    }

    pub fn server_pid(&self) -> u32 {
        self.server_pid
    }

    pub fn server_start_time(&self) -> u64 {
        self.server_start_time
    }

    pub fn session_created(&self) -> u64 {
        self.session_created
    }
}

pub fn list_sessions() -> Result<Vec<TmuxSession>> {
    let output = match run_command(
        "tmux",
        ["list-sessions", "-F", LIST_FORMAT],
        COMMAND_TIMEOUT,
    ) {
        Ok(output) => output,
        Err(error)
            if error
                .downcast_ref::<io::Error>()
                .is_some_and(|error| error.kind() == io::ErrorKind::NotFound) =>
        {
            bail!("tmux is not installed or is not available in PATH")
        }
        Err(error) => return Err(error).context("starting tmux"),
    };

    if output.status.success() {
        return parse_sessions(&output.stdout);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.stdout.is_empty() && no_server_running(&stderr) {
        return Ok(Vec::new());
    }

    bail!("tmux list-sessions failed: {}", command_error(&output))
}

pub fn kill_session(session: &TmuxSession) -> Result<()> {
    #[cfg(not(unix))]
    {
        let _ = session;
        bail!("native tmux management is supported only on Unix")
    }

    #[cfg(unix)]
    {
        let output = run_guarded_command(session, "kill-session", COMMAND_TIMEOUT)
            .context("starting tmux")?;
        confirm_kill(&output)
    }
}

fn confirm_kill(output: &Output) -> Result<()> {
    if output.status.success()
        && output
            .stdout
            .as_slice()
            .split(|byte| *byte == b'\n')
            .map(|line| String::from_utf8_lossy(line))
            .any(|line| line.trim_end() == KILL_SUCCESS_MARKER)
    {
        return Ok(());
    }
    if !output.status.success() {
        bail!("tmux kill-session failed: {}", command_error(output))
    }
    bail!(
        "tmux kill-session did not confirm completion: {}",
        command_error(output)
    )
}

pub fn attach_command(session: &TmuxSession) -> Result<Vec<std::ffi::OsString>> {
    #[cfg(not(unix))]
    {
        let _ = session;
        bail!("native tmux management is supported only on Unix")
    }

    #[cfg(unix)]
    {
        let target = validated_target(&session.id)?;
        let mut command = vec!["tmux".to_owned()];
        command.extend(guarded_command_args(
            session,
            &format!("attach-session -t {target}"),
            None,
        ));
        Ok(command.into_iter().map(Into::into).collect())
    }
}

fn parse_sessions(bytes: &[u8]) -> Result<Vec<TmuxSession>> {
    let text = std::str::from_utf8(bytes).context("tmux returned non-UTF-8 session metadata")?;
    let mut sessions = Vec::new();
    for raw_line in text.lines() {
        let line = raw_line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        if line.len() > MAX_ROW_BYTES {
            bail!("tmux session metadata row exceeds {MAX_ROW_BYTES} bytes");
        }
        if sessions.len() >= MAX_SESSIONS {
            bail!("tmux returned more than {MAX_SESSIONS} sessions");
        }

        let mut fields = line.split('\t');
        let id = fields.next().context("tmux session id is missing")?;
        let name = fields.next().context("tmux session name is missing")?;
        let windows = fields
            .next()
            .context("tmux session window count is missing")?
            .parse::<u32>()
            .context("tmux session window count is invalid")?;
        let attached_clients = fields
            .next()
            .context("tmux attached-client count is missing")?
            .parse::<u32>()
            .context("tmux attached-client count is invalid")?;
        let server_pid = fields
            .next()
            .context("tmux server pid is missing")?
            .parse::<u32>()
            .context("tmux server pid is invalid")?;
        let server_start_time = fields
            .next()
            .context("tmux server start time is missing")?
            .parse::<u64>()
            .context("tmux server start time is invalid")?;
        let session_created = fields
            .next()
            .context("tmux session creation time is missing")?
            .parse::<u64>()
            .context("tmux session creation time is invalid")?;
        if fields.next().is_some() {
            bail!("tmux session metadata contains unexpected fields");
        }
        validated_target(id)?;
        if name.is_empty() {
            bail!("tmux returned an empty session name");
        }
        sessions.push(TmuxSession {
            id: id.into(),
            name: name.into(),
            windows,
            attached_clients,
            server_pid,
            server_start_time,
            session_created,
        });
    }
    sessions.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
    Ok(sessions)
}

fn validated_target(id: &str) -> Result<String> {
    let digits = id.strip_prefix('$').context("invalid tmux session id")?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        bail!("invalid tmux session id")
    }
    Ok(id.to_owned())
}

fn guard_condition(session: &TmuxSession) -> String {
    let server = format!(
        "#{{&&:#{{==:#{{pid}},{pid}}},#{{==:#{{start_time}},{start}}}}}",
        pid = session.server_pid,
        start = session.server_start_time
    );
    format!(
        "#{{&&:{server},#{{==:#{{session_created}},{created}}}}}",
        created = session.session_created
    )
}

#[cfg(unix)]
fn guarded_command_args(
    session: &TmuxSession,
    action: &str,
    success_marker: Option<&str>,
) -> Vec<String> {
    let target = session.id.clone();
    let action = match success_marker {
        Some(marker) => format!("{action}; display-message -p '{marker}'"),
        None => action.to_owned(),
    };
    vec![
        "if-shell".to_owned(),
        "-F".to_owned(),
        "-t".to_owned(),
        target,
        guard_condition(session),
        action,
        format!("display-message -p '{STALE_SESSION_MESSAGE}'"),
    ]
}

#[cfg(unix)]
fn run_guarded_command(session: &TmuxSession, command: &str, timeout: Duration) -> Result<Output> {
    let target = validated_target(&session.id)?;
    let marker = (command == "kill-session").then_some(KILL_SUCCESS_MARKER);
    let args = guarded_command_args(session, &format!("{command} -t {target}"), marker);
    run_command("tmux", args.iter().map(String::as_str), timeout)
}

#[cfg(unix)]
struct NonblockingPipe<R> {
    reader: R,
    bytes: Vec<u8>,
    closed: bool,
}

#[cfg(unix)]
impl<R: AsRawFd> NonblockingPipe<R> {
    fn new(reader: R) -> Result<Self> {
        let fd = reader.as_raw_fd();
        let flags = OFlag::from_bits_truncate(fcntl(fd, FcntlArg::F_GETFL)?);
        fcntl(fd, FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK))?;
        Ok(Self {
            reader,
            bytes: Vec::new(),
            closed: false,
        })
    }
}

#[cfg(unix)]
fn drain_pipe<R: Read>(pipe: &mut NonblockingPipe<R>) -> Result<()> {
    let mut buffer = [0; 16 * 1024];
    loop {
        match pipe.reader.read(&mut buffer) {
            Ok(0) => {
                pipe.closed = true;
                return Ok(());
            }
            Ok(size) => {
                pipe.bytes.extend_from_slice(&buffer[..size]);
                if pipe.bytes.len() > MAX_COMMAND_OUTPUT {
                    bail!("command output exceeds {MAX_COMMAND_OUTPUT} bytes")
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error).context("reading command output"),
        }
    }
}

#[cfg(unix)]
fn run_command<'a, I>(program: &str, args: I, timeout: Duration) -> Result<Output>
where
    I: IntoIterator<Item = &'a str>,
{
    run_command_with_tmpdir(program, args, timeout, None)
}

#[cfg(not(unix))]
fn run_command<'a, I>(_program: &str, _args: I, _timeout: Duration) -> Result<Output>
where
    I: IntoIterator<Item = &'a str>,
{
    bail!("native tmux management is supported only on Unix")
}

#[cfg(unix)]
fn run_command_with_tmpdir<'a, I>(
    program: &str,
    args: I,
    timeout: Duration,
    tmux_tmpdir: Option<&std::path::Path>,
) -> Result<Output>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut command = Command::new(program);
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    if let Some(tmux_tmpdir) = tmux_tmpdir {
        command.env("TMUX_TMPDIR", tmux_tmpdir);
    }
    let mut child = command.spawn()?;
    let stdout = NonblockingPipe::new(child.stdout.take().context("capturing command stdout")?)?;
    let stderr = NonblockingPipe::new(child.stderr.take().context("capturing command stderr")?)?;
    run_command_loop(child, stdout, stderr, timeout)
}

#[cfg(unix)]
fn run_command_loop(
    mut child: Child,
    mut stdout: NonblockingPipe<ChildStdout>,
    mut stderr: NonblockingPipe<ChildStderr>,
    timeout: Duration,
) -> Result<Output> {
    let deadline = Instant::now() + timeout;
    loop {
        let output_error = drain_pipe(&mut stdout)
            .err()
            .or_else(|| drain_pipe(&mut stderr).err());
        if let Some(error) = output_error {
            terminate_child(&mut child)?;
            return Err(error);
        }
        let status = child.try_wait()?;
        if let Some(status) = status.filter(|_| stdout.closed && stderr.closed) {
            return Ok(Output {
                status,
                stdout: stdout.bytes,
                stderr: stderr.bytes,
            });
        }
        if Instant::now() >= deadline {
            terminate_child(&mut child)?;
            bail!("command timed out after {} ms", timeout.as_millis())
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(unix)]
fn terminate_child(child: &mut Child) -> Result<()> {
    if let Err(error) = killpg(Pid::from_raw(child.id() as i32), Signal::SIGKILL) {
        if error != nix::errno::Errno::ESRCH {
            child.kill().context("terminating timed-out command")?;
        }
    }
    child.wait().context("reaping timed-out command")?;
    Ok(())
}

#[cfg(not(unix))]
fn run_command_with_tmpdir<'a, I>(
    _program: &str,
    _args: I,
    _timeout: Duration,
    _tmux_tmpdir: Option<&std::path::Path>,
) -> Result<Output>
where
    I: IntoIterator<Item = &'a str>,
{
    bail!("native tmux management is supported only on Unix")
}

fn no_server_running(stderr: &str) -> bool {
    let stderr = stderr.to_ascii_lowercase();
    stderr.contains("no server running")
        || stderr.contains("no sessions")
        || stderr.contains("no such file or directory")
}

fn command_error(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let message = if stderr.trim().is_empty() {
        String::from_utf8_lossy(&output.stdout)
    } else {
        stderr
    };
    let message = message.trim();
    if message.is_empty() {
        format!("exit status {}", output.status)
    } else {
        message.into()
    }
}

#[cfg(test)]
#[path = "../tests/unit/tmux.rs"]
mod tests;
