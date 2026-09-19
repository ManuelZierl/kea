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
    pub fn integration(self, command: &[OsString]) -> Vec<u8> {
        self.integration_for(command, "local")
    }

    /// The same protocol is available inside nested/remote shells. The caller
    /// chooses a distinct context identity; no SSH/application-name detection.
    pub fn integration_for(self, command: &[OsString], context: &str) -> Vec<u8> {
        assert!(
            !context.is_empty()
                && context.len() <= 128
                && context
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        );
        match self {
            Self::PowerShell => {
                let script = r#"if (-not $global:__kea_prompt_installed) { $global:__kea_prompt_installed=$true; $global:__kea_context='@CONTEXT@'; $global:__kea_saved_prompt=$function:prompt; function global:prompt { $keaStatus=[int](-not $global:?); $keaLast=$global:LASTEXITCODE; $keaCwd=[Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes((Get-Location).Path)); $keaPath=[Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes([string]$env:PATH)); [Console]::Write([char]27+']777;kea;native-done;'+$global:__kea_context+';'+$keaStatus+[char]7+[char]27+']777;kea;prompt;'+$keaCwd+[char]7+[char]27+']778;kea;path;'+$keaPath+[char]7); $global:LASTEXITCODE=$keaLast; if ($keaStatus -ne 0) { Write-Error 'previous command failed' -ErrorAction Ignore }; $keaPrompt=if ($global:__kea_saved_prompt) { & $global:__kea_saved_prompt } else { 'PS '+(Get-Location).Path+'> ' }; [Console]::Write([char]27+']779;kea;input;1;'+$global:__kea_context+';powershell;ready'+[char]7); $keaPrompt }; if ($function:PSConsoleHostReadLine) { $global:__kea_saved_readline=$function:PSConsoleHostReadLine; function global:PSConsoleHostReadLine { $keaLine=& $global:__kea_saved_readline; if ($keaLine) { [Console]::Write([char]27+']779;kea;input;1;'+$global:__kea_context+';powershell;busy'+[char]7+[char]27+']777;kea;native-start;'+$global:__kea_context+[char]7) }; $keaLine } } }
"#;
                script.replace("@CONTEXT@", context).into_bytes()
            }
            Self::Posix => {
                let program = command
                    .first()
                    .map(|p| p.to_string_lossy().into_owned())
                    .or_else(|| std::env::var("SHELL").ok())
                    .unwrap_or_else(|| "sh".into());
                let name = program.rsplit('/').next().unwrap_or(&program);
                let report = r#"__kea_context='@CONTEXT@'; __kea_before_prompt() { __kea_status=$?; printf '\033]777;kea;native-done;%s;%d\007' "$__kea_context" "$__kea_status"; return "$__kea_status"; }; __kea_prompt() { __kea_dir=$(printf '%s' "$PWD" | command base64 2>/dev/null | tr -d '\r\n'); __kea_path=$(printf '%s' "$PATH" | command base64 2>/dev/null | tr -d '\r\n'); printf '\033]777;kea;prompt;%s\007\033]778;kea;path;%s\007\033]779;kea;input;1;%s;posix;ready\007' "$__kea_dir" "$__kea_path" "$__kea_context"; return "${__kea_status:-0}"; }; "#;
                let hook = match name {
                    "bash" => {
                        r#"if [[ $(declare -p PROMPT_COMMAND 2>/dev/null) == "declare -a"* ]]; then PROMPT_COMMAND=(__kea_before_prompt "${PROMPT_COMMAND[@]}" __kea_prompt); else PROMPT_COMMAND="__kea_before_prompt; ${PROMPT_COMMAND:+$PROMPT_COMMAND; }__kea_prompt"; fi; PS0="${PS0-}"$(printf '\033]779;kea;input;1;%s;posix;busy\007\033]777;kea;native-start;%s\007' "$__kea_context" "$__kea_context"); history -d $HISTCMD 2>/dev/null"#
                    }
                    "zsh" => {
                        r#"precmd_functions=(__kea_before_prompt "${precmd_functions[@]}" __kea_prompt); __kea_preexec() { printf '\033]779;kea;input;1;%s;posix;busy\007\033]777;kea;native-start;%s\007' "$__kea_context" "$__kea_context"; }; preexec_functions+=(__kea_preexec)"#
                    }
                    _ => r#"PS1='$(__kea_before_prompt; __kea_prompt)'"${PS1:-$ }""#,
                };
                format!("if [ -z \"${{__kea_installed-}}\" ]; then __kea_installed=1; {report}{hook}; fi\r")
                    .replace("@CONTEXT@", context).into_bytes()
            }
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/shell/mod.rs"]
mod tests;
