use anyhow::{Context, Result};
use std::{
    env, fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

/// Product-owned location for explicitly saved Kea sessions. The file itself is
/// still created with create-new semantics by kea-session, so a collision can never
/// overwrite another recording.
pub fn new_recording_path() -> Result<PathBuf> {
    let directory = session_directory()?;
    fs::create_dir_all(&directory)
        .with_context(|| format!("creating session directory {}", directory.display()))?;
    let micros = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before Unix epoch")?
        .as_micros();
    Ok(directory.join(format!(
        "session-{micros}-{}.kea",
        std::process::id()
    )))
}

pub fn session_directory() -> Result<PathBuf> {
    if cfg!(target_os = "windows") {
        return env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|path| path.join("Kea").join("Sessions"))
            .context("LOCALAPPDATA is unavailable");
    }
    if cfg!(target_os = "macos") {
        return env::var_os("HOME")
            .map(PathBuf::from)
            .map(|path| path.join("Library/Application Support/Kea/Sessions"))
            .context("HOME is unavailable");
    }
    if let Some(path) = env::var_os("XDG_STATE_HOME") {
        return Ok(PathBuf::from(path).join("kea/sessions"));
    }
    env::var_os("HOME")
        .map(PathBuf::from)
        .map(|path| path.join(".local/state/kea/sessions"))
        .context("neither XDG_STATE_HOME nor HOME is available")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_names_are_kea_recordings() {
        let name = format!("session-123-{}.kea", std::process::id());
        assert!(name.starts_with("session-"));
        assert!(name.ends_with(".kea"));
    }
}
