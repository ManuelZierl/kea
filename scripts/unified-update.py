"""Temporary, assertion-checked source migration; removed before the final commit."""
from pathlib import Path

def change(path, transform):
    p = Path(path)
    p.write_text(transform(p.read_text(encoding='utf-8')), encoding='utf-8')

def input_bridge(s):
    s=s.replace('"f5" => Some(15),','"f5" => Some(15),\n        "f6" => Some(17),\n        "f7" => Some(18),\n        "f8" => Some(19),\n        "f9" => Some(20),')
    s=s.replace('"f12" => Some(24),','"f12" => Some(24),\n        "f13" => Some(25),\n        "f14" => Some(26),\n        "f15" => Some(28),\n        "f16" => Some(29),\n        "f17" => Some(31),\n        "f18" => Some(32),\n        "f19" => Some(33),\n        "f20" => Some(34),\n        "f21" => Some(42),\n        "f22" => Some(43),\n        "f23" => Some(44),\n        "f24" => Some(45),')
    s=s.replace('"escape" => vec![27],', '''"escape" => vec![27],
        // Windows can report a named Space without a completed key_char.
        "space" if !m.control => if m.alt { vec![27, b' '] } else { vec![b' '] },''')
    s += '''
#[cfg(test)]
mod passthrough_tests {
    use super::*;
    #[test]
    fn space_control_and_function_keys_are_encoded() {
        for spec in ["space", "shift-space"] {
            assert_eq!(encode(&Keystroke::parse(spec).unwrap(),false).unwrap(),b" ");
        }
        for number in 1..=24 {
            assert!(encode(&Keystroke::parse(&format!("f{number}")).unwrap(),false).is_some());
        }
        for (spec,expected) in [("ctrl-c",3),("ctrl-v",22),("ctrl-z",26),("ctrl-l",12),("ctrl-space",0)] {
            assert_eq!(encode(&Keystroke::parse(spec).unwrap(),false).unwrap(),vec![expected]);
        }
    }
}
'''
    return s
change('crates/kea-app/src/input.rs',input_bridge)

def keys(s):
    s=s.replace('    Execute,\n','    Execute,\n    SendText,\n    Complete,\n')
    s=s.replace('const ALL: [Self; 19]','const ALL: [Self; 21]').replace('        Self::Execute,','        Self::Execute,\n        Self::SendText,\n        Self::Complete,')
    s=s.replace('Self::Execute => "execute",','Self::Execute => "execute",\n            Self::SendText => "send_text",\n            Self::Complete => "complete",')
    s=s.replace('(Action::Execute, "ctrl-enter"),','(Action::Execute, "ctrl-enter"),\n            (Action::SendText, "ctrl-shift-enter"),\n            (Action::Complete, "tab"),')
    a=s.index('            let contexts: &[&str] = match action {')
    b=s.index('            for shortcut in &self.bindings[&action]',a)
    s=s[:a]+'''            let contexts: &[&str] = match action {
                Action::Execute | Action::SendText | Action::Complete => &["KeaCommand > Input"],
                _ => &["Kea > Input", "KeaChrome"],
            };
'''+s[b:]
    s=s.replace('for spec in ["ctrl-z", "ctrl-enter", "ctrl-l", "tab", "shift-tab"] {','for spec in ["ctrl-c", "ctrl-v", "ctrl-z", "ctrl-enter", "ctrl-shift-enter", "ctrl-l", "ctrl-shift-space", "ctrl-shift-q", "f6", "f7", "f8", "f9", "f10", "tab", "shift-tab"] {')
    return s
change('crates/kea-app/src/keybindings.rs',keys)
change('crates/kea-app/src/settings.rs',lambda s:s.replace('    pub syntax_highlighting: bool,','    pub syntax_highlighting: bool,\n    pub show_blocks: bool,').replace('            syntax_highlighting: true,','            syntax_highlighting: true,\n            show_blocks: false,').replace('"syntax_highlighting" => settings.syntax_highlighting = boolean(value)?,','"syntax_highlighting" => settings.syntax_highlighting = boolean(value)?,\n                "show_blocks" => settings.show_blocks = boolean(value)?,') )
change('crates/kea-app/src/lib.rs',lambda s:s+'\npub mod completion;\n')

def document(s):
    s=s.replace('    pub truncated: bool,','    pub truncated: bool,\n    pub directory: Option<String>,')
    s=s.replace('    scanner: MarkerScanner,','    scanner: MarkerScanner,\n    directory: Option<String>,\n    prompt_ready: bool,')
    s=s.replace('    pub fn active(&self)', '''    /// Last explicit report; never inferred from prompt text.
    pub fn directory(&self) -> Option<&str> { self.directory.as_deref() }
    pub fn prompt_ready(&self) -> bool { self.prompt_ready && !self.has_in_flight() }
    pub fn note_terminal_input(&mut self) { self.prompt_ready = false; }
    pub fn active(&self)''')
    s=s.replace('            truncated: false,','            truncated: false,\n            directory: self.directory.clone(),')
    s=s.replace('        self.next_id = self.next_id.max(id.saturating_add(1));\n        self.retained_bytes','        self.prompt_ready = false;\n        self.next_id = self.next_id.max(id.saturating_add(1));\n        self.retained_bytes',1)
    s=s.replace('                Piece::Marker(Marker::Start(id, input)) => {','''                Piece::Marker(Marker::Prompt(directory)) => {
                    self.abort_in_flight(at);
                    self.directory = (!directory.is_empty()).then_some(directory);
                    self.prompt_ready = true;
                    changed = true;
                }
                Piece::Marker(Marker::Directory(directory)) => {
                    self.directory = Some(directory);
                    changed = true;
                }
                Piece::Marker(Marker::Start(id, input)) => {
                    self.prompt_ready = false;''')
    s=s.replace('self.retained_bytes += input.len();','self.retained_bytes += input.len() + self.directory.as_ref().map_or(0, String::len);')
    s=s.replace('&& input.len() <= MAX_DOCUMENT_BYTES.saturating_sub(self.retained_bytes)','&& input.len() + self.directory.as_ref().map_or(0, String::len) <= MAX_DOCUMENT_BYTES.saturating_sub(self.retained_bytes)')
    s=s.replace('    Done(u64, i32),','    Done(u64, i32),\n    Directory(String),\n    Prompt(String),')
    s=s.replace('        "start" => {','''        kind @ ("cwd" | "prompt") => {
            let encoded = parts.next()?;
            if encoded.len() > 8192 || parts.next().is_some() { return None; }
            let directory = String::from_utf8(base64_decode(encoded)?).ok()?;
            if (directory.is_empty() && kind != "prompt") || directory.chars().any(char::is_control) { return None; }
            Some(if kind == "prompt" { Marker::Prompt(directory) } else { Marker::Directory(directory) })
        }
        "start" => {''',1)
    s += r'''
#[cfg(test)]
mod directory_tests {
    use super::*;
    #[test]
    fn explicit_directory_reports_are_chunk_independent() {
        let mut d=Document::new();
        let marker=format!("\x1b]777;kea;prompt;{}\x07",base64_encode("/tmp/space ä;dir".as_bytes()));
        for byte in marker.as_bytes() { d.ingest_output(1,&[*byte]); }
        assert_eq!(d.directory(),Some("/tmp/space ä;dir")); assert!(d.prompt_ready());
        d.note_terminal_input(); assert!(!d.prompt_ready());
        d.ingest_output(2,marker.as_bytes());
        d.queue_local(1,"pwd".into(),3).unwrap(); assert!(!d.prompt_ready());
        assert_eq!(d.blocks()[0].directory.as_deref(),d.directory());
        d.ingest_output(4,b"\x1b]777;kea;prompt;AA==\x07");
        assert_eq!(d.directory(),Some("/tmp/space ä;dir"));
    }
}
'''
    return s
change('crates/kea-document/src/lib.rs',document)
change('crates/kea-session/src/lib.rs',lambda s:s.replace('if find_bytes(&self.pending, b"\\x1b]777;kea;start;").is_some() {','if find_bytes(&self.pending, b"\\x1b]777;kea;start;").is_some()\n            || find_bytes(&self.pending, b"\\x1b]777;kea;prompt;").is_some() {'))

def shell(s):
    s=s.replace('    pub fn detect(command: &[OsString]) -> Option<Self> {','''    pub fn detect(command: &[OsString]) -> Option<Self> {
        // Never inject an interactive hook into a -c/-Command/script invocation.
        if command.iter().skip(1).any(|arg| !matches!(arg.to_string_lossy().to_ascii_lowercase().as_str(), "-i" | "-l" | "--login" | "--noprofile" | "--norc" | "-nologo" | "-noprofile" | "-noexit")) { return None; }''',1)
    a=s.index('    pub fn wrap(')
    s=s[:a]+r'''    /// Preserve the existing prompt and prompt callbacks while adding metadata.
    pub fn integration(self, command: &[OsString]) -> Vec<u8> {
        match self {
            Self::PowerShell => br#"$global:__kea_saved_prompt=$function:prompt; function global:prompt { $status=$?; $code=$global:LASTEXITCODE; $p=[Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes((Get-Location).Path)); [Console]::Write([char]27+']777;kea;prompt;'+$p+[char]7); $global:LASTEXITCODE=$code; if ($global:__kea_saved_prompt) { & $global:__kea_saved_prompt } else { 'PS '+(Get-Location).Path+'> ' } }
"#.iter().copied().map(|b| if b == b'\n' { b'\r' } else { b }).collect(),
            Self::Posix => {
                let program = command.first().map(|p| p.to_string_lossy().into_owned()).or_else(|| std::env::var("SHELL").ok()).unwrap_or_else(|| "sh".into());
                let name=program.rsplit('/').next().unwrap_or(&program);
                let report="__kea_prompt() { __kea_rc=$?; __kea_dir=$(printf '%s' \"$PWD\" | command base64 2>/dev/null | tr -d '\\r\\n'); printf '\\033]777;kea;prompt;%s\\007' \"$__kea_dir\"; return \"$__kea_rc\"; }; ";
                let hook=match name {
                    "bash" => r#"if [[ $(declare -p PROMPT_COMMAND 2>/dev/null) == "declare -a"* ]]; then PROMPT_COMMAND+=(__kea_prompt); else PROMPT_COMMAND="${PROMPT_COMMAND:+$PROMPT_COMMAND; }__kea_prompt"; fi"#,
                    "zsh" => "precmd_functions+=(__kea_prompt)",
                    _ => "PS1='$(__kea_prompt)'\"${PS1:-$ }\"",
                };
                format!("{report}{hook}\r").into_bytes()
            }
        }
    }

'''+s[a:]
    s += r'''
#[cfg(test)]
mod integration_tests {
    use super::*;
    use kea_document::Document;
    use kea_session::{Session,Observed};
    use std::time::{Duration,Instant};
    fn wait_ready(session: &mut Session,document: &mut Document) {
        let start=Instant::now();
        loop {
            for event in session.pump_observed().observed {
                if let Observed::Output {at,bytes}=event { document.ingest_output(at,&bytes); }
            }
            if document.prompt_ready() { break; }
            assert!(start.elapsed()<Duration::from_secs(10),"no ready marker: {}",session.screen().text());
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    #[test]
    fn directory_follows_editor_and_native_terminal_cd() {
        #[cfg(unix)]
        let (command,flavor,text,native)=(vec!["bash".into(),"--noprofile".into(),"--norc".into()],ShellFlavor::Posix,"cd /tmp",b"cd /\r".to_vec());
        #[cfg(windows)]
        let (command,flavor,text,native)=(vec!["powershell.exe".into(),"-NoLogo".into(),"-NoProfile".into(),"-NoExit".into()],ShellFlavor::PowerShell,"Set-Location $env:TEMP",b"Set-Location $env:SystemRoot\r".to_vec());
        let mut session=Session::spawn(&command,kea_core::Size::new(100,24).unwrap(),None).unwrap();
        let mut d=Document::new();
        session.send_hidden(flavor.integration(&command)).unwrap();
        wait_ready(&mut session,&mut d); assert!(d.directory().is_some());
        d.queue_local(1,text.into(),session.elapsed_micros()).unwrap();
        session.send_hidden(flavor.wrap(1,text).unwrap()).unwrap();
        wait_ready(&mut session,&mut d);
        #[cfg(unix)] assert_eq!(d.directory(),Some("/tmp"));
        #[cfg(windows)] assert_eq!(d.directory().map(str::to_lowercase),std::env::var("TEMP").ok().map(|p|p.to_lowercase()));
        session.send(native).unwrap(); d.note_terminal_input();
        wait_ready(&mut session,&mut d);
        #[cfg(unix)] assert_eq!(d.directory(),Some("/"));
        #[cfg(windows)] assert_eq!(d.directory().map(str::to_lowercase),std::env::var("SystemRoot").ok().map(|p|p.to_lowercase()));
    }
}
'''
    return s
change('crates/kea-app/src/shell.rs',shell)
change('crates/kea-app/src/command_editor.rs',lambda s:s+r'''
/// Use the component's scrolling after layout; do not steal focus or a selection.
pub fn follow_output_tail(editor: &Entity<InputState>,window: &mut Window,_cx: &mut App) {
    let editor=editor.downgrade();
    window.on_next_frame(move |window,cx| {
        let previous=window.focused(cx);
        let _=editor.update(cx,|state,cx| {
            use gpui_component::input::RopeExt as _;
            if state.selected_text_range(true,window,cx).is_some_and(|s| !s.range.is_empty()) { return; }
            let position=state.text().offset_to_position(state.text().len());
            state.set_cursor_position(position,window,cx);
        });
        if let Some(previous)=previous { window.focus(&previous); } else { window.blur(); }
    });
}
''')
change('crates/kea-app/src/document_view.rs',lambda s:s.replace('                    self.document_ui.visible.insert(','                    command_editor::follow_output_tail(&editor,window,cx);\n                    self.document_scroll.scroll_to_bottom();\n                    self.document_ui.visible.insert(',1).replace('                        view.output_len = block.output().len();','                        command_editor::follow_output_tail(&view.editor,window,cx);\n                        view.output_len = block.output().len();'))

# Test PE subsystem in the existing untagged Windows artifact build.
ci=Path('.github/workflows/ci.yml');s=ci.read_text()
s=s.replace('      - run: cargo build --release --locked -p kea-app','''      - run: cargo test --locked -p kea-app --lib
      - run: cargo build --release --locked -p kea-app
      - name: Verify Windows GUI subsystem
        shell: python
        run: |
          import struct
          from pathlib import Path
          data = Path('target/release/kea.exe').read_bytes()
          pe = struct.unpack_from('<I', data, 0x3c)[0]
          assert struct.unpack_from('<H', data, pe + 24 + 68)[0] == 2
''')
ci.write_text(s)
