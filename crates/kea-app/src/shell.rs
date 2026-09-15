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
            matches!(
                arg.as_str(),
                "-c" | "--command" | "-command" | "-encodedcommand" | "-file" | "-f"
            )
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

    pub fn report_directory(self) -> Vec<u8> {
        format!("{}\r", self.directory_script()).into_bytes()
    }

    fn directory_script(self) -> &'static str {
        match self {
            Self::Posix => {
                r#"printf '\033]777;kea;cwd;'; printf '%s' "$PWD" | command od -An -tx1 | command tr -d ' \n'; printf '\007'"#
            }
            Self::PowerShell => {
                r#"[Console]::Write([char]27 + ']777;kea;cwd;' + [BitConverter]::ToString([Text.Encoding]::UTF8.GetBytes((Get-Location).Path)).Replace('-', '') + [char]7)"#
            }
        }
    }

    pub fn wrap(self, id: u64, input: &str) -> Result<Vec<u8>, Error> {
        let encoded = encode_input(input)?;
        let directory = self.directory_script();
        let command = match self {
            Self::Posix => {
                let quoted = quote_posix(input);
                format!(
                    "{directory}; printf '\\033]777;kea;start;{id};{encoded}\\007'; eval {quoted}; printf '\\033]777;kea;done;{id};%d\\007' \"$?\"; {directory}\r"
                )
            }
            Self::PowerShell => {
                let quoted = quote_powershell(input);
                format!(
                    "{directory}; [Console]::Write([char]27 + ']777;kea;start;{id};{encoded}' + [char]7); $__kea_previous=$global:LASTEXITCODE; $global:LASTEXITCODE=$null; Invoke-Expression {quoted}; $__kea_ok=$?; $__kea_native=$global:LASTEXITCODE; $__kea_status=if ($__kea_ok) {{ if ($null -ne $__kea_native) {{ [int]$__kea_native }} else {{ 0 }} }} else {{ if ($null -ne $__kea_native -and [int]$__kea_native -ne 0) {{ [int]$__kea_native }} else {{ 1 }} }}; if ($null -eq $__kea_native) {{ $global:LASTEXITCODE=$__kea_previous }}; [Console]::Write([char]27 + ']777;kea;done;{id};' + $__kea_status + [char]7); {directory}\r"
                )
            }
        };
        Ok(command.into_bytes())
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
            if matches!(document.blocks()[0].status, CommandStatus::Finished(0)) {
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(10));
        }

        let block = &document.blocks()[0];
        assert_eq!(block.input, input);
        assert_eq!(block.plain_output(), "KEA_DOC_ONE\nKEA_DOC_TWO");
        assert!(document.current_directory().is_some());
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
    #[cfg(unix)]
    #[test]
    fn reports_changed_directory_without_losing_exit_status() {
        let script = String::from_utf8(ShellFlavor::Posix.wrap(1, "cd /; false").unwrap()).unwrap();
        let result = std::process::Command::new("sh")
            .arg("-c")
            .arg(script.trim_end_matches('\r'))
            .output()
            .unwrap();
        let mut doc = kea_document::Document::new();
        doc.ingest_output(0, &result.stdout);
        assert_eq!(doc.current_directory(), Some("/"));
        assert_eq!(
            doc.blocks()[0].status,
            kea_document::CommandStatus::Finished(1)
        );
        assert_eq!(doc.blocks()[0].plain_output(), "");
    }
    #[cfg(windows)]
    #[test]
    fn powershell_reports_directory_and_native_exit_code() {
        let directory = std::env::temp_dir();
        let input = format!(
            "Set-Location {}; cmd.exe /c exit 7",
            quote_powershell(&directory.to_string_lossy())
        );
        let script = String::from_utf8(ShellFlavor::PowerShell.wrap(1, &input).unwrap()).unwrap();
        let result = std::process::Command::new("powershell.exe")
            .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command"])
            .arg(script.trim_end_matches('\r'))
            .output()
            .unwrap();
        let mut doc = kea_document::Document::new();
        doc.ingest_output(0, &result.stdout);
        let reported = doc
            .current_directory()
            .expect("PowerShell did not report its location");
        assert_eq!(
            std::fs::canonicalize(reported).unwrap(),
            std::fs::canonicalize(directory).unwrap()
        );
        assert_eq!(
            doc.blocks()[0].status,
            kea_document::CommandStatus::Finished(7)
        );
    }
}
