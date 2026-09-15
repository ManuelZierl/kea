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
        // Only instrument launches known to remain interactive. Scripts, -c,
        // abbreviated PowerShell switches and unknown arguments use literal input.
        let flags: &[&str] = match shell {
            Self::Posix => &["-i", "-l", "--login", "--noprofile", "--norc"],
            Self::PowerShell => &["-nologo", "-noprofile", "-noexit"],
        };
        for arg in command.iter().skip(1) {
            if !flags.contains(&arg.to_string_lossy().to_ascii_lowercase().as_str()) {
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

    /// Metadata-only request for a known shell. Never use it inside an arbitrary
    /// TUI/REPL. The terminal output is canonical, so recordings retain the report.
    pub fn directory_query(self) -> Vec<u8> {
        format!("{}\r", self.directory_statement()).into_bytes()
    }

    fn directory_statement(self) -> &'static str {
        match self {
            Self::Posix => r#"printf '\033]777;kea;cwd;%s\007' "$(printf '%s' "$PWD" | base64 | tr -d '\r\n')""#,
            Self::PowerShell => "[Console]::Write([char]27 + ']777;kea;cwd;' + [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($pwd.Path)) + [char]7)",
        }
    }

    pub fn wrap(self, id: u64, input: &str) -> Result<Vec<u8>, Error> {
        let encoded = encode_input(input)?;
        let command = match self {
            Self::Posix => {
                // Interactive line editors treat literal LF/CR bytes as input
                // submission. Encode the complete draft as octal data so even a
                // multiline command is transported as exactly one physical line.
                // The trailing '_' prevents command substitution from stripping a
                // user-supplied trailing newline; `${var%_}` removes only sentinel.
                let source = encode_posix_source(input);
                let source_var = format!("__kea_source_{id}");
                let status_var = format!("__kea_status_{id}");
                format!(
                    "{source_var}=$(printf '%b_' '{source}'); {source_var}=${{{source_var}%_}}; printf '\\033]777;kea;start;{id};{encoded}\\007'; eval \"${source_var}\"; {status_var}=$?; printf '\\033]777;kea;done;{id};%d\\007' \"${status_var}\"; unset {source_var} {status_var}\r"
                )
            }
            Self::PowerShell => {
                // PowerShell has a built-in base64 decoder. Decode into a String
                // before Invoke-Expression so PTY input itself contains no embedded
                // newline from the user's draft.
                let source_var = format!("$__kea_source_{id}");
                format!(
                    "{source_var}=[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{encoded}')); [Console]::Write([char]27 + ']777;kea;start;{id};{encoded}' + [char]7); $__kea_previous=$global:LASTEXITCODE; $global:LASTEXITCODE=$null; Invoke-Expression {source_var}; $__kea_ok=$?; $__kea_native=$global:LASTEXITCODE; $__kea_status=if ($__kea_ok) {{ if ($null -ne $__kea_native) {{ [int]$__kea_native }} else {{ 0 }} }} else {{ if ($null -ne $__kea_native -and [int]$__kea_native -ne 0) {{ [int]$__kea_native }} else {{ 1 }} }}; if ($null -eq $__kea_native) {{ $global:LASTEXITCODE=$__kea_previous }}; [Console]::Write([char]27 + ']777;kea;done;{id};' + $__kea_status + [char]7); Remove-Variable __kea_source_{id} -ErrorAction SilentlyContinue\r"
                )
            }
        };
        // Emit after the done marker, so metadata collection cannot replace the
        // real command exit status. Never parse a formatted prompt for the cwd.
        let command = command.trim_end_matches('\r');
        Ok(format!(
            "{}; {command}; {}\r",
            self.directory_statement(),
            self.directory_statement()
        )
        .into_bytes())
    }
}

/// Encode arbitrary UTF-8 command bytes for POSIX `printf %b` without placing
/// any byte from the user's command directly into the interactive input line.
fn encode_posix_source(input: &str) -> String {
    let mut encoded = String::with_capacity(input.len().saturating_mul(5));
    for byte in input.as_bytes() {
        // POSIX printf %b specifies \0ddd octal escapes (up to three digits).
        write!(&mut encoded, "\\0{byte:03o}").expect("writing to String cannot fail");
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_supported_shells_without_path_assumptions() {
        assert_eq!(
            ShellFlavor::from_program("/bin/bash"),
            Some(ShellFlavor::Posix)
        );
        assert_eq!(
            ShellFlavor::from_program(r"C:\\Program Files\\PowerShell\\7\\pwsh.exe"),
            Some(ShellFlavor::PowerShell)
        );
        assert_eq!(ShellFlavor::from_program("opencode"), None);
        assert_eq!(
            ShellFlavor::detect(&["bash".into(), "-c".into(), "opencode".into()]),
            None
        );
    }

    #[test]
    fn scripts_and_unknown_shell_arguments_do_not_receive_startup_metadata() {
        for args in [
            vec!["bash", "script.sh"],
            vec!["bash", "-lc", "opencode"],
            vec!["pwsh", "-Command", "opencode"],
            vec!["pwsh", "-e", "encoded-command"],
        ] {
            let command = args.into_iter().map(OsString::from).collect::<Vec<_>>();
            assert_eq!(ShellFlavor::detect(&command), None);
        }
        let command = ["bash", "--noprofile", "--norc"].map(OsString::from);
        assert_eq!(ShellFlavor::detect(&command), Some(ShellFlavor::Posix));
        let command = ["powershell.exe", "-NoLogo", "-NoProfile", "-NoExit"].map(OsString::from);
        assert_eq!(ShellFlavor::detect(&command), Some(ShellFlavor::PowerShell));
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
    fn posix_source_encoding_preserves_newlines_quotes_and_unicode_as_data() {
        let encoded = encode_posix_source("ls\nls\n'ä'");
        assert!(!encoded.contains('\n'));
        assert!(!encoded.contains('\''));
        assert!(encoded.starts_with("\\0154\\0163\\0012\\0154\\0163"));
    }

    #[test]
    fn posix_wrapper_quotes_user_text_as_data_and_carries_document_metadata() {
        let bytes = ShellFlavor::Posix
            .wrap(42, "printf '%s\\n' \"it's safe\"")
            .unwrap();
        let wrapper = String::from_utf8(bytes).unwrap();
        assert!(wrapper.contains("start;42"));
        assert!(wrapper.contains("done;42"));
        assert!(!wrapper.contains("it's safe"));
        assert!(!wrapper.contains("start;42;printf"));
    }

    #[cfg(unix)]
    #[test]
    fn posix_wrapper_creates_a_real_multiline_structured_block_through_the_pty() {
        use kea_core::Size;
        use kea_document::{CommandStatus, Document};
        use kea_session::{Observed, Session};
        use std::time::{Duration, Instant};

        let mut session = Session::spawn(&["sh".into()], Size::new(80, 24).unwrap(), None).unwrap();
        let mut document = Document::new();
        let input = "printf 'KEA_DOC_ONE\\n'\nprintf 'KEA_DOC_TWO\\n'";
        let id = document.allocate_id();
        let wrapper = ShellFlavor::Posix.wrap(id, input).unwrap();
        assert!(!wrapper[..wrapper.len() - 1].contains(&b'\n'));
        let queued_at = session.elapsed_micros();
        document.queue_local(id, input.into(), queued_at).unwrap();
        session.send_hidden(wrapper).unwrap();

        let started = Instant::now();
        loop {
            let pump = session.pump_observed();
            for event in pump.observed {
                match event {
                    Observed::Output { at, bytes } => {
                        document.ingest_output(at, &bytes);
                    }
                    Observed::Exit { at, .. } => {
                        document.abort_in_flight(at);
                    }
                }
            }
            if matches!(document.blocks()[0].status, CommandStatus::Finished(0))
                && document.current_directory().is_some()
            {
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(10));
        }

        let block = &document.blocks()[0];
        assert_eq!(block.input, input);
        assert_eq!(block.plain_output(), "KEA_DOC_ONE\nKEA_DOC_TWO");
    }

    #[test]
    fn powershell_wrapper_decodes_source_before_eval_and_keeps_it_one_line() {
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
        assert!(wrapper.contains("$__kea_previous=$global:LASTEXITCODE"));
        assert!(wrapper.contains("$global:LASTEXITCODE=$__kea_previous"));
        assert!(!wrapper.trim_end_matches('\r').contains('\n'));
        assert!(!wrapper.contains("Write-Output 'hello'"));
    }
}
