//! Local tmux discovery and lifecycle commands.
//!
//! A tmux session is a persistent external resource. Kea may attach a terminal
//! client to it, but the tmux server remains the source of truth and can outlive
//! any Kea tab.

use anyhow::{bail, Context as _, Result};
use std::{
    io,
    process::{Command, Output},
};

const LIST_FORMAT: &str = "#{session_id}\t#{session_name}\t#{session_windows}\t#{session_attached}";
const MAX_SESSIONS: usize = 256;
const MAX_ROW_BYTES: usize = 4096;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TmuxSession {
    id: String,
    name: String,
    windows: u32,
    attached_clients: u32,
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
}

pub fn list_sessions() -> Result<Vec<TmuxSession>> {
    let output = match Command::new("tmux")
        .args(["list-sessions", "-F", LIST_FORMAT])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
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

pub fn kill_session(id: &str) -> Result<()> {
    validate_session_id(id)?;
    let output = Command::new("tmux")
        .args(["kill-session", "-t", id])
        .output()
        .context("starting tmux")?;
    if output.status.success() {
        return Ok(());
    }
    bail!("tmux kill-session failed: {}", command_error(&output))
}

pub fn attach_command(id: &str) -> Result<Vec<std::ffi::OsString>> {
    validate_session_id(id)?;
    Ok(vec![
        "tmux".into(),
        "attach-session".into(),
        "-t".into(),
        id.into(),
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
        if fields.next().is_some() {
            bail!("tmux session metadata contains unexpected fields");
        }
        validate_session_id(id)?;
        if name.is_empty() {
            bail!("tmux returned an empty session name");
        }
        sessions.push(TmuxSession {
            id: id.into(),
            name: name.into(),
            windows,
            attached_clients,
        });
    }
    sessions.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
    Ok(sessions)
}

fn validate_session_id(id: &str) -> Result<()> {
    if id.len() < 2 || !id.starts_with('$') || !id.as_bytes()[1..].iter().all(u8::is_ascii_digit) {
        bail!("invalid tmux session id")
    }
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
) || !id.as_bytes()[1..].iter().all(u8::is_ascii_digit) {
        bail!("invalid tmux session id")
    }
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
