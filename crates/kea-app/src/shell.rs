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

    /// Build one physical PTY line. User newlines are decoded only inside the shell.
    ///
    /// Instrumentation must be observational: after the wrapper finishes, the shell's
    /// visible status must be the user's command status, not the status of Kea's marker
    /// printf/unset bookkeeping. The final subshell `exit` restores `$?` without exiting
    /// the interactive parent shell. Symmetrically, the wrapper snapshots the entry
    /// `$?` before its own display line runs and restores it immediately before `eval`,
    /// so user code observing `$?` (for example `echo $?` after a failing Run) sees the
    /// previous command status instead of Kea's display `printf` status.
    pub fn wrap(self, id: u64, input: &str, prompt_column: usize) -> Result<Vec<u8>, Error> {
        let encoded = encode_input(input)?;
        let display = display_source(input);
        let display_column = prompt_column.saturating_add(1);
        let command = match self {
            Self::Posix => {
                let source = encode_posix_source(input);
                let display = encode_posix_source(&display);
                let entry_var = format!("__kea_entry_{id}");
                let source_var = format!("__kea_source_{id}");
                let status_var = format!("__kea_status_{id}");
                let command = format!(
                    "{entry_var}=$?; printf '\\033[{display_column}G%b' '{display}'; {source_var}=$(printf '%b_' '{source}'); {source_var}=${{{source_var}%_}}; printf '\\033]777;kea;start;{id};{encoded}\\007'; (exit \"${entry_var}\"); eval \"${source_var}\"; {status_var}=$?; printf '\\033]777;kea;done;{id};%d\\007' \"${status_var}\"; (exit \"${status_var}\"); {source_var}_kea_rc=$?; unset {entry_var} {source_var} {status_var}; (exit \"${{{source_var}_kea_rc}}\")\r"
                );
                command
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
    fn detects_supported_shells() {
        assert_eq!(
            ShellFlavor::from_program("/bin/bash"),
            Some(ShellFlavor::Posix)
        );
        assert_eq!(
            ShellFlavor::from_program(r"C:\\PowerShell\\pwsh.exe"),
            Some(ShellFlavor::PowerShell)
        );
        assert_eq!(ShellFlavor::from_program("opencode"), None);
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
    fn posix_wrapper_restores_user_exit_status_after_instrumentation() {
        let wrapper = String::from_utf8(ShellFlavor::Posix.wrap(7, "false", 0).unwrap()).unwrap();
        assert!(wrapper.contains("__kea_status_7=$?"));
        assert!(wrapper.contains("__kea_source_7_kea_rc=$?"));
        assert!(wrapper.ends_with("(exit \"${__kea_source_7_kea_rc}\")\r"));
    }

    /// Manual acceptance A5: `false` via Run, then `echo $?` via Run, must print
    /// `1`. The wrapper's own display line must not become the `$?` that user
    /// code observes, so this drives the real wrapper through a real bash.
    #[cfg(unix)]
    #[test]
    fn posix_wrapper_preserves_entry_status_in_real_bash() {
        use std::ffi::OsString;
        use std::io::Write as _;
        use std::process::{Command, Stdio};

        let installed = Command::new("bash")
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success());
        if !installed {
            eprintln!("skipping: bash is not installed");
            return;
        }
        // Kea feeds the PTY through the line discipline, where `\r` submits the
        // line. A pipe has no line discipline, so submit with `\n` instead.
        let feed = |bytes: Vec<u8>| {
            bytes
                .into_iter()
                .map(|b| if b == b'\r' { b'\n' } else { b })
                .collect::<Vec<_>>()
        };
        let integration = feed(ShellFlavor::Posix.integration(&[OsString::from("bash")]));
        let failing = feed(ShellFlavor::Posix.wrap(7, "false", 0).unwrap());
        let probe = feed(ShellFlavor::Posix.wrap(9, "echo MARKER:$?", 0).unwrap());

        let mut child = Command::new("bash")
            .args(["--noprofile", "--norc", "-i"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn bash for wrapper status check");
        {
            let stdin = child.stdin.as_mut().expect("piped bash stdin");
            stdin.write_all(&integration).expect("feed hook");
            stdin.write_all(&failing).expect("feed failing command");
            stdin.write_all(&probe).expect("feed status probe");
            stdin.write_all(b"exit\n").expect("feed exit");
        }
        let output = child.wait_with_output().expect("reap bash");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("MARKER:1"),
            "expected the probe to observe the previous Run status; got:\n{stdout}"
        );
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
}
