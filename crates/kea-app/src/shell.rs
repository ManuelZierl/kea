use kea_document::{encode_input, Error};
use std::{ffi::OsString, fmt::Write as _};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShellFlavor {
    Posix,
    PowerShell,
}

impl ShellFlavor {
    pub fn detect(command: &[OsString]) -> Option<Self> {
        let program = command
            .first()
            .map(|program| program.to_string_lossy().into_owned())
            .or_else(|| std::env::var("SHELL").ok())?;
        let shell = Self::from_program(&program)?;

        // Only instrument launches that are known to remain interactive. Never
        // inject prompt hooks into scripts, -c/-Command invocations or unknown
        // argument combinations.
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

    /// Install a prompt hook without replacing the user's normal prompt/profile.
    /// Every idle prompt reports the real local shell cwd and PATH. That makes cwd
    /// tracking follow commands typed directly in the terminal as well as commands
    /// submitted through the editor; no prompt text is scraped.
    pub fn integration(self, command: &[OsString]) -> Vec<u8> {
        match self {
            Self::PowerShell => {
                let script = r#"if (-not $global:__kea_prompt_installed) { $global:__kea_prompt_installed=$true; $global:__kea_saved_prompt=$function:prompt; function global:prompt { $keaLast=$global:LASTEXITCODE; $keaCwd=[Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes((Get-Location).Path)); $keaPath=[Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes([string]$env:PATH)); [Console]::Write([char]27+']777;kea;prompt;'+$keaCwd+';'+$keaPath+[char]7); $global:LASTEXITCODE=$keaLast; if ($global:__kea_saved_prompt) { & $global:__kea_saved_prompt } else { 'PS '+(Get-Location).Path+'> ' } } }
"#;
                script
                    .bytes()
                    .map(|byte| if byte == b'\n' { b'\r' } else { byte })
                    .collect()
            }
            Self::Posix => {
                let program = command
                    .first()
                    .map(|p| p.to_string_lossy().into_owned())
                    .or_else(|| std::env::var("SHELL").ok())
                    .unwrap_or_else(|| "sh".into());
                let name = program.rsplit('/').next().unwrap_or(&program);
                let report = r#"__kea_prompt() { __kea_rc=$?; __kea_dir=$(printf '%s' "$PWD" | command base64 2>/dev/null | tr -d '\r\n'); __kea_path=$(printf '%s' "$PATH" | command base64 2>/dev/null | tr -d '\r\n'); printf '\033]777;kea;prompt;%s;%s\007' "$__kea_dir" "$__kea_path"; return "$__kea_rc"; }; "#;
                let hook = match name {
                    "bash" => r#"if [[ $(declare -p PROMPT_COMMAND 2>/dev/null) == "declare -a"* ]]; then PROMPT_COMMAND+=(__kea_prompt); else PROMPT_COMMAND="${PROMPT_COMMAND:+$PROMPT_COMMAND; }__kea_prompt"; fi"#,
                    "zsh" => "precmd_functions+=(__kea_prompt)",
                    _ => "PS1='$(__kea_prompt)'\"${PS1:-$ }\"",
                };
                format!("{report}{hook}\r").into_bytes()
            }
        }
    }

    /// Build one *physical* line for the interactive shell. User newlines are
    /// encoded as data and decoded inside the shell, so a multiline editor draft
    /// cannot be split by Readline/PSReadLine before Kea's start marker executes.
    pub fn wrap(self, id: u64, input: &str) -> Result<Vec<u8>, Error> {
        let encoded = encode_input(input)?;
        let command = match self {
            Self::Posix => {
                let source = encode_posix_source(input);
                let source_var = format!("__kea_source_{id}");
                let status_var = format!("__kea_status_{id}");
                format!(
                    "{source_var}=$(printf '%b_' '{source}'); {source_var}=${{{source_var}%_}}; printf '\\033]777;kea;start;{id};{encoded}\\007'; eval \"${source_var}\"; {status_var}=$?; printf '\\033]777;kea;done;{id};%d\\007' \"${status_var}\"; unset {source_var} {status_var}\r"
                )
            }
            Self::PowerShell => {
                let source_var = format!("$__kea_source_{id}");
                format!(
                    "{source_var}=[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{encoded}')); [Console]::Write([char]27 + ']777;kea;start;{id};{encoded}' + [char]7); $__kea_previous=$global:LASTEXITCODE; $global:LASTEXITCODE=$null; Invoke-Expression {source_var}; $__kea_ok=$?; $__kea_native=$global:LASTEXITCODE; $__kea_status=if ($__kea_ok) {{ if ($null -ne $__kea_native) {{ [int]$__kea_native }} else {{ 0 }} }} else {{ if ($null -ne $__kea_native -and [int]$__kea_native -ne 0) {{ [int]$__kea_native }} else {{ 1 }} }}; if ($null -eq $__kea_native) {{ $global:LASTEXITCODE=$__kea_previous }}; [Console]::Write([char]27 + ']777;kea;done;{id};' + $__kea_status + [char]7); Remove-Variable __kea_source_{id} -ErrorAction SilentlyContinue\r"
                )
            }
        };
        Ok(command.into_bytes())
    }
}

fn encode_posix_source(input: &str) -> String {
    let mut encoded = String::with_capacity(input.len().saturating_mul(5));
    for byte in input.as_bytes() {
        // POSIX printf %b supports \0ddd octal escapes. No user byte is copied
        // literally into the interactive shell line.
        write!(&mut encoded, "\\0{byte:03o}").expect("writing to String cannot fail");
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_only_supported_interactive_shell_launches() {
        assert_eq!(
            ShellFlavor::from_program("/bin/bash"),
            Some(ShellFlavor::Posix)
        );
        assert_eq!(
            ShellFlavor::from_program(r"C:\\Program Files\\PowerShell\\7\\pwsh.exe"),
            Some(ShellFlavor::PowerShell)
        );
        assert_eq!(ShellFlavor::from_program("opencode"), None);
        for args in [
            vec!["bash", "script.sh"],
            vec!["bash", "-lc", "opencode"],
            vec!["pwsh", "-Command", "opencode"],
            vec!["pwsh", "-e", "encoded-command"],
        ] {
            let command = args.into_iter().map(OsString::from).collect::<Vec<_>>();
            assert_eq!(ShellFlavor::detect(&command), None);
        }
        assert_eq!(
            ShellFlavor::detect(&["powershell.exe".into(), "-NoLogo".into(), "-NoExit".into()]),
            Some(ShellFlavor::PowerShell)
        );
    }

    #[test]
    fn prompt_integration_reports_cwd_and_path_without_replacing_profiles() {
        let posix = String::from_utf8(ShellFlavor::Posix.integration(&["bash".into()])).unwrap();
        assert!(posix.contains("PROMPT_COMMAND"));
        assert!(posix.contains("kea;prompt;%s;%s"));
        assert!(posix.contains("$PWD"));
        assert!(posix.contains("$PATH"));

        let ps = String::from_utf8(
            ShellFlavor::PowerShell.integration(&["powershell.exe".into()]),
        )
        .unwrap();
        assert!(ps.contains("__kea_saved_prompt"));
        assert!(ps.contains("$env:PATH"));
        assert!(ps.contains("kea;prompt;"));
    }

    #[test]
    fn wrappers_transport_multiline_input_as_one_physical_line() {
        let input = "ls\nls\nprintf 'ä\\n'\n";
        for shell in [ShellFlavor::Posix, ShellFlavor::PowerShell] {
            let wrapper = shell.wrap(42, input).unwrap();
            assert_eq!(wrapper.last(), Some(&b'\r'));
            assert!(
                !wrapper[..wrapper.len() - 1].contains(&b'\n'),
                "wrapper contains a physical LF: {}",
                String::from_utf8_lossy(&wrapper)
            );
            assert!(String::from_utf8_lossy(&wrapper).contains("start;42"));
            assert!(String::from_utf8_lossy(&wrapper).contains("done;42"));
        }
    }

    #[test]
    fn posix_source_encoding_keeps_user_text_out_of_driver_line() {
        let encoded = encode_posix_source("ls\nls\n'ä'");
        assert!(!encoded.contains('\n'));
        assert!(!encoded.contains('\''));
        assert!(encoded.starts_with("\\0154\\0163\\0012\\0154\\0163"));
    }

    #[cfg(unix)]
    #[test]
    fn prompt_hook_tracks_native_cd_and_multiline_run() {
        use kea_core::Size;
        use kea_document::{CommandStatus, Document};
        use kea_session::{Observed, Session};
        use std::time::{Duration, Instant};

        fn pump_until(
            session: &mut Session,
            document: &mut Document,
            predicate: impl Fn(&Document) -> bool,
        ) {
            let started = Instant::now();
            loop {
                for event in session.pump_observed().observed {
                    match event {
                        Observed::Output { at, bytes } => {
                            document.ingest_output(at, &bytes);
                        }
                        Observed::Exit { at, .. } => {
                            document.abort_in_flight(at);
                        }
                    }
                }
                if predicate(document) {
                    break;
                }
                assert!(started.elapsed() < Duration::from_secs(10));
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        let command = ["bash".into(), "--noprofile".into(), "--norc".into()];
        let mut session = Session::spawn(&command, Size::new(100, 24).unwrap(), None).unwrap();
        let mut document = Document::new();
        session
            .send_hidden(ShellFlavor::Posix.integration(&command))
            .unwrap();
        pump_until(&mut session, &mut document, Document::prompt_ready);
        assert!(document.directory().is_some());
        assert!(document.shell_path().is_some());

        session.send(b"cd /tmp\r".to_vec()).unwrap();
        document.note_terminal_input();
        pump_until(&mut session, &mut document, |d| {
            d.prompt_ready() && d.directory() == Some("/tmp")
        });

        let input = "printf 'KEA_ONE\\n'\nprintf 'KEA_TWO\\n'";
        session
            .send_hidden(ShellFlavor::Posix.wrap(9, input).unwrap())
            .unwrap();
        pump_until(&mut session, &mut document, |d| {
            d.blocks()
                .last()
                .is_some_and(|block| matches!(block.status, CommandStatus::Finished(0)))
                && d.prompt_ready()
        });
        let block = document.blocks().last().unwrap();
        assert_eq!(block.input, input);
        assert_eq!(block.plain_output(), "KEA_ONE\nKEA_TWO");
    }

    #[test]
    fn powershell_wrapper_decodes_source_before_eval() {
        let wrapper = String::from_utf8(
            ShellFlavor::PowerShell
                .wrap(9, "Write-Output 'hello'\nWrite-Output 'again'")
                .unwrap(),
        )
        .unwrap();
        assert!(wrapper.contains("FromBase64String"));
        assert!(wrapper.contains("Invoke-Expression $__kea_source_9"));
        assert!(wrapper.contains("start;9"));
        assert!(wrapper.contains("done;9"));
        assert!(!wrapper.trim_end_matches('\r').contains('\n'));
        assert!(!wrapper.contains("Write-Output 'hello'"));
    }
}
