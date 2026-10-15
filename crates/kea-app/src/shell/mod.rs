use kea_core::MarkerToken;
use std::ffi::OsString;

pub mod token;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShellFlavor {
    Posix,
    PowerShell,
}

/// The private file that installs POSIX integration. Dropping the guard
/// deletes the file if the shell has not already done so.
#[derive(Debug)]
pub struct BootstrapFile(std::path::PathBuf);

impl BootstrapFile {
    pub fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for BootstrapFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Where the bootstrap file goes: the per-user runtime directory when the
/// platform provides an owner-only one, otherwise the system temp directory.
pub fn bootstrap_directory() -> std::path::PathBuf {
    #[cfg(unix)]
    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from) {
        use std::os::unix::fs::PermissionsExt as _;
        // Someone else's private directory is not writable; creating the file
        // then fails and the caller falls back to the temp directory.
        let private = dir.is_absolute()
            && std::fs::metadata(&dir)
                .is_ok_and(|meta| meta.is_dir() && meta.permissions().mode() & 0o077 == 0);
        if private {
            return dir;
        }
    }
    std::env::temp_dir()
}

impl ShellFlavor {
    pub fn detect(command: &[OsString]) -> Option<Self> {
        let program = command
            .first()
            .map(|program| program.to_string_lossy().into_owned())
            .or_else(|| std::env::var("SHELL").ok())?;
        let shell = Self::from_program(&program)?;
        let allowed: &[&str] = match shell {
            Self::Posix => &["-i", "-l", "--login", "--noprofile", "--norc"],
            Self::PowerShell => &["-nologo", "-noprofile", "-noexit"],
        };
        for arg in command.iter().skip(1) {
            if !allowed.contains(&arg.to_string_lossy().to_ascii_lowercase().as_str()) {
                return None;
            }
        }
        Some(shell)
    }

    pub fn from_program(program: &str) -> Option<Self> {
        let name = program
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(program)
            .to_ascii_lowercase();
        match name.as_str() {
            "sh" | "bash" | "dash" | "zsh" | "ksh" | "mksh" => Some(Self::Posix),
            "pwsh" | "pwsh.exe" | "powershell" | "powershell.exe" => Some(Self::PowerShell),
            _ => None,
        }
    }

    /// Install observational hooks once, preserving user profiles and prompts.
    /// Authored commands never pass through an eval driver.
    pub fn integration(self, command: &[OsString], token: &MarkerToken) -> Vec<u8> {
        let mut integration = self.integration_for(command, "local", token);
        if self == Self::Posix {
            debug_assert_eq!(integration.last(), Some(&b'\n'));
            *integration
                .last_mut()
                .expect("POSIX integration is not empty") = b'\r';
        }
        integration
    }

    /// Install POSIX integration by sourcing a private file instead of typing
    /// the whole script. Returns the short line to type and the file. The file is
    /// owner-only (it holds the session token) and removes itself once sourced;
    /// Kea also deletes it when the returned guard drops, because a shell that
    /// never finishes sourcing it (an rc `exec`, Ctrl-C, early exit) cannot.
    /// A short line is echoed and suppressed reliably instead of being redrawn
    /// across wrapped rows, and the token never appears on screen.
    pub fn posix_bootstrap(
        command: &[OsString],
        token: &MarkerToken,
        directory: &std::path::Path,
    ) -> anyhow::Result<(Vec<u8>, BootstrapFile)> {
        use anyhow::Context as _;
        use std::io::Write as _;
        let mut file = tempfile::Builder::new()
            .prefix(".kea-shell-")
            .suffix(".sh")
            .tempfile_in(directory)
            .context("cannot create the shell integration file")?;
        let path = file.path().to_owned();
        // A lossy path would make the shell source (and delete) another file.
        let quoted = posix_quote(
            path.to_str()
                .context("the shell integration directory is not valid UTF-8")?,
        );
        let mut script = Self::Posix.integration_for(command, "local", token);
        script.extend_from_slice(format!("command rm -f -- {quoted}\n").as_bytes());
        file.write_all(&script)
            .and_then(|()| file.as_file().sync_all())
            .context("cannot write the shell integration file")?;
        let path = BootstrapFile(
            file.into_temp_path()
                .keep()
                .context("cannot keep the shell integration file")?,
        );
        // Same history rules as the inline script: bash deletes the line itself
        // (the sourced hook runs `history -d $HISTCMD`); others skip a leading space.
        let prefix = if posix_name(command) == "bash" {
            ""
        } else {
            " "
        };
        Ok((format!("{prefix}. {quoted}\r").into_bytes(), path))
    }

    /// The same protocol is available inside nested/remote shells. The caller
    /// chooses a distinct context identity; no SSH/application-name detection.
    /// Every marker carries `token`, so output that merely prints a marker is
    /// not mistaken for this shell (see `kea_core::MarkerAuth`).
    pub fn integration_for(
        self,
        command: &[OsString],
        context: &str,
        token: &MarkerToken,
    ) -> Vec<u8> {
        assert!(
            !context.is_empty()
                && context.len() <= 128
                && context
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        );
        match self {
            Self::PowerShell => {
                let script = r#"if (-not $global:__kea_prompt_installed) { $global:__kea_prompt_installed=$true; $global:__kea_context='@CONTEXT@'; $global:__kea_token='@TOKEN@'; $global:__kea_saved_prompt=$function:prompt; function global:prompt { $keaStatus=[int](-not $global:?); $keaLast=$global:LASTEXITCODE; $keaCwd=[Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes((Get-Location).Path)); $keaPath=[Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes([string]$env:PATH)); [Console]::Write([char]27+']777;kea;'+$global:__kea_token+';native-done;'+$global:__kea_context+';'+$keaStatus+[char]7+[char]27+']777;kea;'+$global:__kea_token+';prompt;'+$keaCwd+[char]7+[char]27+']778;kea;'+$global:__kea_token+';path;'+$keaPath+[char]7); $global:LASTEXITCODE=$keaLast; if ($keaStatus -ne 0) { Write-Error 'previous command failed' -ErrorAction Ignore }; $keaPrompt=if ($global:__kea_saved_prompt) { & $global:__kea_saved_prompt } else { 'PS '+(Get-Location).Path+'> ' }; [Console]::Write([char]27+']779;kea;'+$global:__kea_token+';input;1;'+$global:__kea_context+';powershell;ready'+[char]7); $keaPrompt }; if ($function:PSConsoleHostReadLine) { $global:__kea_saved_readline=$function:PSConsoleHostReadLine; function global:PSConsoleHostReadLine { $keaLine=& $global:__kea_saved_readline; if ($keaLine) { [Console]::Write([char]27+']779;kea;'+$global:__kea_token+';input;1;'+$global:__kea_context+';powershell;busy'+[char]7+[char]27+']777;kea;'+$global:__kea_token+';native-start;'+$global:__kea_context+[char]7) }; $keaLine } } }
"#;
                script
                    .replace("@CONTEXT@", context)
                    .replace("@TOKEN@", token.as_str())
                    .into_bytes()
            }
            Self::Posix => {
                let name = posix_name(command);
                let name = name.as_str();
                let report = r#"__kea_context='@CONTEXT@'; __kea_token='@TOKEN@'; __kea_before_prompt() { __kea_status=$?; printf '\033]777;kea;%s;native-done;%s;%d\007' "$__kea_token" "$__kea_context" "$__kea_status"; return "$__kea_status"; }; __kea_prompt() { __kea_dir=$(printf '%s' "$PWD" | command base64 2>/dev/null | tr -d '\r\n'); __kea_path=$(printf '%s' "$PATH" | command base64 2>/dev/null | tr -d '\r\n'); printf '\033]777;kea;%s;prompt;%s\007\033]778;kea;%s;path;%s\007\033]779;kea;%s;input;1;%s;posix;ready\007' "$__kea_token" "$__kea_dir" "$__kea_token" "$__kea_path" "$__kea_token" "$__kea_context"; return "${__kea_status:-0}"; }; "#;
                let hook = match name {
                    "bash" => {
                        r#"if [[ $(declare -p PROMPT_COMMAND 2>/dev/null) == "declare -a"* ]]; then PROMPT_COMMAND=(__kea_before_prompt "${PROMPT_COMMAND[@]}" __kea_prompt); else __kea_nl=$'\n'; PROMPT_COMMAND="__kea_before_prompt${__kea_nl}${PROMPT_COMMAND:+$PROMPT_COMMAND${__kea_nl}}__kea_prompt"; fi; PS0="${PS0-}"$(printf '\033]779;kea;%s;input;1;%s;posix;busy\007\033]777;kea;%s;native-start;%s\007' "$__kea_token" "$__kea_context" "$__kea_token" "$__kea_context"); history -d $HISTCMD 2>/dev/null"#
                    }
                    "zsh" => {
                        r#"precmd_functions=(__kea_before_prompt "${precmd_functions[@]}" __kea_prompt); __kea_preexec() { printf '\033]779;kea;%s;input;1;%s;posix;busy\007\033]777;kea;%s;native-start;%s\007' "$__kea_token" "$__kea_context" "$__kea_token" "$__kea_context"; }; preexec_functions+=(__kea_preexec)"#
                    }
                    _ => r#"PS1='$(__kea_before_prompt; __kea_prompt)'"${PS1:-$ }""#,
                };
                // Newline-joined PROMPT_COMMAND keeps user values ending in `;`,
                // `&` or a newline valid. Bash deletes the bootstrap from history
                // explicitly; that must stay unprefixed, because with
                // `ignorespace` `history -d $HISTCMD` would delete the user's
                // previous command instead. Other shells cannot delete the line,
                // so a leading space lets HIST_IGNORE_SPACE/ignorespace skip it.
                let prefix = if name == "bash" { "" } else { " " };
                format!("{prefix}if [ -z \"${{__kea_installed-}}\" ]; then __kea_installed=1; {report}{hook}; fi\n")
                    .replace("@CONTEXT@", context)
                    .replace("@TOKEN@", token.as_str())
                    .into_bytes()
            }
        }
    }
}

/// The POSIX shell's program name, as `integration_for` selects hooks by it.
fn posix_name(command: &[OsString]) -> String {
    let program = command
        .first()
        .map(|p| p.to_string_lossy().into_owned())
        .or_else(|| std::env::var("SHELL").ok())
        .unwrap_or_else(|| "sh".into());
    program.rsplit('/').next().unwrap_or(&program).to_owned()
}

fn posix_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
#[path = "../../tests/unit/shell/mod.rs"]
mod tests;
