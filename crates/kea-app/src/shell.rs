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

    /// Install a prompt hook without replacing the user's profile or normal prompt.
    /// OSC 777 remains recording/document metadata for cwd/readiness. OSC 778 is
    /// live host metadata carrying the shell's effective PATH for completion.
    pub fn integration(self, command: &[OsString]) -> Vec<u8> {
        match self {
            Self::PowerShell => {
                let script = r#"if (-not $global:__kea_prompt_installed) { $global:__kea_prompt_installed=$true; $global:__kea_saved_prompt=$function:prompt; function global:prompt { $keaLast=$global:LASTEXITCODE; $keaCwd=[Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes((Get-Location).Path)); $keaPath=[Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes([string]$env:PATH)); [Console]::Write([char]27+']777;kea;prompt;'+$keaCwd+[char]7+[char]27+']778;kea;path;'+$keaPath+[char]7); $global:LASTEXITCODE=$keaLast; if ($global:__kea_saved_prompt) { & $global:__kea_saved_prompt } else { 'PS '+(Get-Location).Path+'> ' } } }
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
                let report = r#"__kea_prompt() { __kea_rc=$?; __kea_dir=$(printf '%s' "$PWD" | command base64 2>/dev/null | tr -d '\r\n'); __kea_path=$(printf '%s' "$PATH" | command base64 2>/dev/null | tr -d '\r\n'); printf '\033]777;kea;prompt;%s\007\033]778;kea;path;%s\007' "$__kea_dir" "$__kea_path"; return "$__kea_rc"; }; "#;
                let hook = match name {
                    "bash" => {
                        r#"if [[ $(declare -p PROMPT_COMMAND 2>/dev/null) == "declare -a"* ]]; then PROMPT_COMMAND+=(__kea_prompt); else PROMPT_COMMAND="${PROMPT_COMMAND:+$PROMPT_COMMAND; }__kea_prompt"; fi"#
                    }
                    "zsh" => "precmd_functions+=(__kea_prompt)",
                    _ => "PS1='$(__kea_prompt)'\"${PS1:-$ }\"",
                };
                format!("{report}{hook}\r").into_bytes()
            }
        }
    }

    /// Build one physical PTY line. User newlines are decoded only *inside* the
    /// shell so an interactive line editor cannot split a multiline draft early.
    pub fn wrap(self, id: u64, input: &str, prompt_column: usize) -> Result<Vec<u8>, Error> {
        let encoded = encode_input(input)?;
        let display = display_source(input);
        let display_column = prompt_column.saturating_add(1);
        let command = match self {
            Self::Posix => {
                let source = encode_posix_source(input);
                let display = encode_posix_source(&display);
                let source_var = format!("__kea_source_{id}");
                let status_var = format!("__kea_status_{id}");
                let command = format!(
                    "{source_var}=$(printf '%b_' '{source}'); {source_var}=${{{source_var}%_}}; printf '\\033]777;kea;start;{id};{encoded}\\007'; eval \"${source_var}\"; {status_var}=$?; printf '\\033]777;kea;done;{id};%d\\007' \"${status_var}\"; unset {source_var} {status_var}\r"
                );
                format!("printf '\\033[{display_column}G%b' '{display}'; {command}")
            }
            Self::PowerShell => {
                let source_var = format!("$__kea_source_{id}");
                let display = encode_input(&display)?;
                let command = format!(
                    "{source_var}=[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{encoded}')); [Console]::Write([char]27 + ']777;kea;start;{id};{encoded}' + [char]7); $__kea_previous=$global:LASTEXITCODE; $global:LASTEXITCODE=$null; Invoke-Expression {source_var}; $__kea_ok=$?; $__kea_native=$global:LASTEXITCODE; $__kea_status=if ($__kea_ok) {{ if ($null -ne $__kea_native) {{ [int]$__kea_native }} else {{ 0 }} }} else {{ if ($null -ne $__kea_native -and [int]$__kea_native -ne 0) {{ [int]$__kea_native }} else {{ 1 }} }}; if ($null -eq $__kea_native) {{ $global:LASTEXITCODE=$__kea_previous }}; [Console]::Write([char]27 + ']777;kea;done;{id};' + $__kea_status + [char]7); Remove-Variable __kea_source_{id} -ErrorAction SilentlyContinue\r"
                );
                format!("[Console]::Write([char]27 + '[{display_column}G' + [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{display}'))); {command}")
            }
        };
        Ok(command.into_bytes())
    }
}

fn encode_posix_source(input: &str) -> String {
    let mut encoded = String::with_capacity(input.len().saturating_mul(5));
    for byte in input.as_bytes() {
        write!(&mut encoded, "\\0{byte:03o}").expect("writing to String cannot fail");
    }
    encoded
}

fn display_source(input: &str) -> String {
    let mut display = String::with_capacity(input.len().saturating_add(1));
    for character in input.chars() {
        match character {
            '\n' | '\t' => display.push(character),
            character if character.is_control() => {
                write!(&mut display, "\\u{{{:x}}}", u32::from(character))
                    .expect("writing to String cannot fail");
            }
            character => display.push(character),
        }
    }
    if !display.ends_with('\n') {
        display.push('\n');
    }
    display
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
    fn prompt_integration_reports_cwd_and_path_and_preserves_prompt() {
        let posix = String::from_utf8(ShellFlavor::Posix.integration(&["bash".into()])).unwrap();
        assert!(posix.contains("PROMPT_COMMAND"));
        assert!(posix.contains("777;kea;prompt;%s"));
        assert!(posix.contains("778;kea;path;%s"));
        assert!(posix.contains("$PWD"));
        assert!(posix.contains("$PATH"));

        let ps = String::from_utf8(ShellFlavor::PowerShell.integration(&["powershell.exe".into()]))
            .unwrap();
        assert!(ps.contains("__kea_saved_prompt"));
        assert!(ps.contains("$env:PATH"));
        assert!(ps.contains("777;kea;prompt;"));
        assert!(ps.contains("778;kea;path;"));
    }

    #[test]
    fn wrappers_transport_multiline_input_as_one_physical_line() {
        let input = "ls\nls\nprintf 'ä\\n'\n";
        for shell in [ShellFlavor::Posix, ShellFlavor::PowerShell] {
            let wrapper = shell.wrap(42, input, 7).unwrap();
            assert_eq!(wrapper.last(), Some(&b'\r'));
            assert!(!wrapper[..wrapper.len() - 1].contains(&b'\n'));
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

    #[test]
    fn command_presentation_preserves_lines_but_escapes_terminal_controls() {
        assert_eq!(
            display_source("printf ok\nprintf '\u{1b}'"),
            "printf ok\nprintf '\\u{1b}'\n"
        );
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
        assert!(
            !session.screen().text().contains("__kea_prompt"),
            "private shell integration leaked into the terminal: {:?}",
            session.screen().text()
        );
        let canonical_output = session
            .recording()
            .events()
            .iter()
            .filter_map(|event| match &event.kind {
                kea_core::Kind::Output(bytes) => Some(bytes.as_slice()),
                _ => None,
            })
            .flatten()
            .copied()
            .collect::<Vec<_>>();
        assert!(
            String::from_utf8_lossy(&canonical_output).contains("__kea_prompt"),
            "canonical recording must retain unmodified PTY output"
        );
        session.seek(session.recording().events().len()).unwrap();
        assert!(
            !session.screen().text().contains("__kea_prompt"),
            "historical presentation exposed private shell integration: {:?}",
            session.screen().text()
        );
        session.go_live();

        session.send(b"abc\x7f\x7f\x7f".to_vec()).unwrap();
        document.note_terminal_input();
        assert!(!document.prompt_ready());
        session.send(vec![3]).unwrap();
        pump_until(&mut session, &mut document, Document::prompt_ready);

        session.send(b"cd /tmp\r".to_vec()).unwrap();
        document.note_terminal_input();
        pump_until(&mut session, &mut document, |d| {
            d.prompt_ready() && d.directory() == Some("/tmp")
        });

        let input = "printf 'KEA_ONE\\n'\nprintf 'KEA_TWO\\n'";
        session
            .send_hidden(ShellFlavor::Posix.wrap(9, input, 10).unwrap())
            .unwrap();
        document.note_terminal_input();
        pump_until(&mut session, &mut document, |d| {
            d.blocks()
                .last()
                .is_some_and(|block| matches!(block.status, CommandStatus::Finished(0)))
                && d.prompt_ready()
        });
        let block = document.blocks().last().unwrap();
        assert_eq!(block.input, input);
        assert_eq!(block.plain_output(), "KEA_ONE\nKEA_TWO");
        let screen = session.screen().text();
        assert!(screen.contains("bash-5.3$ printf 'KEA_ONE\\n'"));
        assert!(screen.contains("printf 'KEA_TWO\\n'"));
        assert!(!screen.contains("__kea_source_9"));

        session.seek(session.recording().events().len()).unwrap();
        let replayed = session.screen().text();
        assert!(replayed.contains("bash-5.3$ printf 'KEA_ONE\\n'"));
        assert!(replayed.contains("printf 'KEA_TWO\\n'"));
        assert!(replayed.contains("KEA_ONE"));
        assert!(replayed.contains("KEA_TWO"));
        assert!(!replayed.contains("__kea_source_9"));
    }

    #[test]
    fn powershell_wrapper_decodes_source_before_eval() {
        let wrapper = String::from_utf8(
            ShellFlavor::PowerShell
                .wrap(9, "Write-Output 'hello'\nWrite-Output 'again'", 4)
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
