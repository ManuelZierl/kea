use kea_document::{encode_input, Error};
use std::ffi::OsString;

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
        if command.iter().skip(1).any(|arg| {
            let arg = arg.to_string_lossy().to_ascii_lowercase();
            arg == "-c"
                || arg == "-command"
                || arg == "-encodedcommand"
                || arg == "-file"
                || arg == "-noninteractive"
                || arg == "--command"
        }) {
            return None;
        }
        Self::from_program(&program)
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
                let quoted = quote_posix(input);
                format!(
                    "printf '\\033]777;kea;start;{id};{encoded}\\007'; eval {quoted}; printf '\\033]777;kea;done;{id};%d\\007' \"$?\"\r"
                )
            }
            Self::PowerShell => {
                let quoted = quote_powershell(input);
                format!(
                    "[Console]::Write([char]27 + ']777;kea;start;{id};{encoded}' + [char]7); $__kea_previous=$global:LASTEXITCODE; $global:LASTEXITCODE=$null; Invoke-Expression {quoted}; $__kea_ok=$?; $__kea_native=$global:LASTEXITCODE; $__kea_status=if ($__kea_ok) {{ if ($null -ne $__kea_native) {{ [int]$__kea_native }} else {{ 0 }} }} else {{ if ($null -ne $__kea_native -and [int]$__kea_native -ne 0) {{ [int]$__kea_native }} else {{ 1 }} }}; if ($null -eq $__kea_native) {{ $global:LASTEXITCODE=$__kea_previous }}; [Console]::Write([char]27 + ']777;kea;done;{id};' + $__kea_status + [char]7)\r"
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

fn quote_posix(input: &str) -> String {
    let mut quoted = String::with_capacity(input.len() + 2);
    quoted.push('\'');
    for (index, part) in input.split('\'').enumerate() {
        if index != 0 {
            quoted.push_str("'\"'\"'");
        }
        quoted.push_str(part);
    }
    quoted.push('\'');
    quoted
}

fn quote_powershell(input: &str) -> String {
    format!("'{}'", input.replace('\'', "''"))
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
    fn posix_wrapper_quotes_user_text_as_data_and_carries_document_metadata() {
        let bytes = ShellFlavor::Posix
            .wrap(42, "printf '%s\\n' \"it's safe\"")
            .unwrap();
        let wrapper = String::from_utf8(bytes).unwrap();
        assert!(wrapper.contains("start;42"));
        assert!(wrapper.contains("done;42"));
        assert!(wrapper.contains("'\"'\"'"));
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
    fn powershell_wrapper_uses_current_scope_eval_and_markers() {
        let wrapper = String::from_utf8(
            ShellFlavor::PowerShell
                .wrap(9, "Write-Output 'hello'")
                .unwrap(),
        )
        .unwrap();
        assert!(wrapper.contains("Invoke-Expression"));
        assert!(wrapper.contains("start;9"));
        assert!(wrapper.contains("done;9"));
        assert!(wrapper.contains("$__kea_previous=$global:LASTEXITCODE"));
        assert!(wrapper.contains("$global:LASTEXITCODE=$__kea_previous")); // preserves the previous native exit code
        assert!(wrapper.contains("''hello''"));
    }
}
