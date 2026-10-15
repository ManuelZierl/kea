//! Tokens that authenticate shell-integration markers (see `kea_core::MarkerAuth`).
//!
//! Each live terminal gets a fresh random session token, embedded only in the
//! integration Kea installs into its own shell. Remote/nested integration that
//! the user installs elsewhere uses one persistent remote token, stored
//! owner-only next to Kea's settings; it can never speak for the local shell.
use anyhow::{Context as _, Result};
use kea_core::{MarkerAuth, MarkerToken};
use std::{fs, io::Write as _, path::PathBuf};

const REMOTE_TOKEN_FILE: &str = "remote-integration-token";

pub fn new_session_token() -> Result<MarkerToken> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|error| anyhow::anyhow!("no system randomness: {error}"))?;
    Ok(MarkerToken::from_bytes(bytes))
}

/// The markers a live terminal accepts: its own token, plus the user's remote
/// token when one exists. An unreadable remote token only disables remote
/// integration; it never weakens the local check.
pub fn live_auth(session: MarkerToken) -> MarkerAuth {
    MarkerAuth::Live {
        session,
        remote: remote_token_path().and_then(|path| read_remote_token(&path).ok().flatten()),
    }
}

fn remote_token_path() -> Option<PathBuf> {
    crate::config::settings::Settings::path().map(|path| path.with_file_name(REMOTE_TOKEN_FILE))
}

fn read_remote_token(path: &std::path::Path) -> Result<Option<MarkerToken>> {
    match fs::read_to_string(path) {
        Ok(text) => MarkerToken::parse(text.trim())
            .map(Some)
            .with_context(|| format!("{} is not a valid integration token", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("cannot read {}", path.display())),
    }
}

/// The remote-integration token, created owner-only on first use.
pub fn remote_token() -> Result<MarkerToken> {
    let path = remote_token_path().context("the settings directory is unavailable")?;
    remote_token_at(&path)
}

pub(crate) fn remote_token_at(path: &std::path::Path) -> Result<MarkerToken> {
    if let Some(token) = read_remote_token(path)? {
        return Ok(token);
    }
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    let token = new_session_token()?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    match options.open(path) {
        Ok(mut file) => {
            writeln!(file, "{}", token.as_str())
                .and_then(|()| file.sync_all())
                .with_context(|| format!("cannot write {}", path.display()))?;
            Ok(token)
        }
        // Another Kea created it concurrently: use theirs.
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            read_remote_token(path)?.context("the remote integration token disappeared")
        }
        Err(error) => Err(error).with_context(|| format!("cannot create {}", path.display())),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/shell/token.rs"]
mod tests;
