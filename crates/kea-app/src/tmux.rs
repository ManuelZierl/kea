//! Local tmux discovery and lifecycle commands.
//!
//! A tmux session is a persistent external resource. Kea may attach a terminal
//! client to it, but the tmux server remains the source of truth and can outlive
//! any Kea tab.

use anyhow::{bail, Context as _, Result};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::{
    io::{self, Read},
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

const LIST_FORMAT: &str = "#{session_id}\t#{session_name}\t#{session_windows}\t#{session_attached}\t#{pid}\t#{start_time}\t#{session_created}";
const MAX_SESSIONS: usize = 256;
const MAX_ROW_BYTES: usize = 4096;
const MAX_COMMAND_OUTPUT: usize = 64 * 1024;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(2);

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
    let output =
        run_guarded_command(session, "kill-session", COMMAND_TIMEOUT).context("starting tmux")?;
    if output.status.success() {
        return Ok(());
    }
    bail!("tmux kill-session failed: {}", command_error(&output))
}

pub fn attach_command(session: &TmuxSession) -> Result<Vec<std::ffi::OsString>> {
    let target = validated_target(&session.id)?;
    let condition = guard_condition(session);
    Ok(vec![
        "tmux".into(),
        "if-shell".into(),
        "-F".into(),
        "-t".into(),
        target.clone().into(),
        condition.into(),
        format!("attach-session -t {target}").into(),
        "run-shell 'exit 1'".into(),
    ])
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
    let pid = format!("#{{==:#{{pid}},{pid}}}", pid = session.server_pid);
    let start = format!(
        "#{{==:#{{start_time}},{start}}}",
        start = session.server_start_time
    );
    let created = format!(
        "#{{==:#{{session_created}},{created}}}",
        created = session.session_created
    );
    format!("#{{&&:{pid},{start},{created}}}")
}

fn run_guarded_command(session: &TmuxSession, command: &str, timeout: Duration) -> Result<Output> {
    let target = validated_target(&session.id)?;
    let args = [
        "if-shell".to_owned(),
        "-F".to_owned(),
        "-t".to_owned(),
        target.clone(),
        guard_condition(session),
        format!("{command} -t {target}"),
        "run-shell 'exit 1'".to_owned(),
    ];
    run_command("tmux", args.iter().map(String::as_str), timeout)
}

fn run_command<'a, I>(program: &str, args: I, timeout: Duration) -> Result<Output>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut command = Command::new(program);
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) == -1 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
    let mut child = command.spawn()?;
    let stdout = child.stdout.take().context("capturing command stdout")?;
    let stderr = child.stderr.take().context("capturing command stderr")?;
    let stdout_reader = thread::spawn(move || read_capped(stdout));
    let stderr_reader = thread::spawn(move || read_capped(stderr));
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            terminate_child(&mut child)?;
            join_reader(stdout_reader)?;
            join_reader(stderr_reader)?;
            bail!("command timed out after {} ms", timeout.as_millis())
        }
        thread::sleep(Duration::from_millis(10));
    };
    let stdout = join_reader(stdout_reader)?;
    let stderr = join_reader(stderr_reader)?;
    if stdout.len() > MAX_COMMAND_OUTPUT || stderr.len() > MAX_COMMAND_OUTPUT {
        bail!("command output exceeds {MAX_COMMAND_OUTPUT} bytes")
    }
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn read_capped(reader: impl io::Read) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take((MAX_COMMAND_OUTPUT + 1) as u64)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn join_reader(reader: thread::JoinHandle<io::Result<Vec<u8>>>) -> Result<Vec<u8>> {
    reader
        .join()
        .map_err(|_| anyhow::anyhow!("command output reader panicked"))?
        .context("reading command output")
}

fn terminate_child(child: &mut Child) -> Result<()> {
    #[cfg(unix)]
    {
        let result = unsafe { libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL) };
        if result == -1 {
            child.kill().context("terminating timed-out command")?;
        }
    }
    #[cfg(not(unix))]
    child.kill().context("terminating timed-out command")?;
    child.wait().context("reaping timed-out command")?;
    Ok(())
}

fn no_server_running(stderr: &str) -> bool {
    let stderr = stderr.to_ascii_lowercase();
    stderr.contains("no server running")
        || stderr.contains("failed to connect to server")
        || stderr.contains("no sessions")
}

fn command_error(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let message = stderr.trim();
    if message.is_empty() {
        format!("exit status {}", output.status)
    } else {
        message.into()
    }
}

#[cfg(test)]
#[path = "../tests/unit/tmux.rs"]
mod tests;
