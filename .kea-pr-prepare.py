"""One-shot, branch-local source preparation; removed before the PR is opened."""
from pathlib import Path
import textwrap


def replace(path, old, new, count=1):
    p = Path(path)
    text = p.read_text()
    assert text.count(old) == count, (path, old[:100], text.count(old))
    p.write_text(text.replace(old, new))


def write(path, text):
    p = Path(path)
    assert not p.exists(), path
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(textwrap.dedent(text).lstrip('\n'))


p = Path('crates/kea-app/src/lib.rs')
p.write_text(p.read_text() + '\npub mod tabs;\n')
app = 'crates/kea-app/src/app/'
replace(app+'mod.rs', 'mod terminal_input;', 'mod tabs;\nmod terminal_input;\nuse tabs::{KeaRoot, WorkspaceEvent};')
p = Path(app+'mod.rs')
s = p.read_text()
start = s.index('struct KeaRoot {')
end = s.index('fn button(', start)
s = s[:start] + s[end:]
s = s.replace('struct KeaView {\n', 'struct KeaView {\n    visible: bool,\n')
p.write_text(s)
replace(app+'workspace.rs', '        Self {\n            session,', '        Self {\n            visible: true,\n            session,')
replace(app+'workspace.rs', 'if this.settings.animate_logo && this.poll_composer_logo()', 'if this.visible && this.settings.animate_logo && this.poll_composer_logo()')
replace(app+'workspace.rs', 'if this.settings.animate_logo && this.logo_warmed_frames < LOGO_FRAME_COUNT', 'if this.visible && this.settings.animate_logo && this.logo_warmed_frames < LOGO_FRAME_COUNT')
replace(app+'rendering.rs', '    TitleBar,\n', '')
replace(app+'rendering.rs', '            .child(TitleBar::new().child(div().font_weight(FontWeight::BOLD).child("Kea")))\n', '')

p = Path(app+'startup.rs')
s = p.read_text()
start = s.index('    #[cfg(windows)]\n    if !demo && !replay_requested')
end = s.index('    let document = Document::from_recording', start)
s = s[:start] + '''    let (session, shell) = if demo {
        (Session::demo()?, None)
    } else if let Some(path) = replay {
        let loaded = kea_core::read_from(File::open(path)?)?;
        let mut session = Session::from_recording(loaded.recording)?;
        session.go_live();
        if loaded.truncated_tail {
            session.warning = Some("Recovered complete events; the final recording frame was truncated.".into());
        }
        (session, None)
    } else {
        spawn_terminal(command, record.as_deref())?
    };

''' + s[end:]
s = s.replace('Kea — one terminal session with a persistent text editor', 'Kea — independent terminal tabs with persistent text editors')
s = s.replace('Terminal and editor are visible together; focus decides who receives keyboard input.', 'Each tab keeps a terminal and editor visible together; focus decides who receives keyboard input.\nFrom composer: Ctrl+Shift+T opens a local shell, Ctrl+Shift+W closes a tab, Ctrl+Tab switches.\nCtrl+Shift+PageUp/PageDown reorders tabs. Live terminal input keeps its native shortcuts.')
s = s.replace('let app = cx.new(|_| KeaRoot { view });', '''let app = cx.new(|cx| KeaRoot::new(view, window, cx));
                let weak = app.downgrade();
                window.on_window_should_close(cx, move |window, cx| {
                    weak.update(cx, |root, cx| root.should_close(window, cx)).unwrap_or(true)
                });''')
s += '''
/// Shared startup path for a fresh local terminal. Never changes process-wide cwd.
/// The caller alone supplies explicit command/recording options for the first tab.
pub(super) fn spawn_terminal(
    mut command: Vec<OsString>, record: Option<&std::path::Path>,
) -> Result<(Session, Option<ShellFlavor>)> {
    #[cfg(windows)]
    if command.is_empty() {
        command = vec!["powershell.exe".into(), "-NoLogo".into(), "-NoExit".into()];
    }
    let shell = ShellFlavor::detect(&command);
    // Install after the user's profile; do not inject source through PSReadLine.
    if shell == Some(ShellFlavor::PowerShell) {
        if command.is_empty() {
            command.push(std::env::var_os("SHELL").context("PowerShell executable unavailable")?);
        }
        let script = String::from_utf8(ShellFlavor::PowerShell.integration(&command))?;
        if !command.iter().any(|arg| arg.to_string_lossy().eq_ignore_ascii_case("-noexit")) {
            command.push("-NoExit".into());
        }
        command.push("-Command".into());
        command.push(script.into());
    }
    let mut session = Session::spawn(&command, kea_core::Size::new(100, 26)?, record)?;
    if shell == Some(ShellFlavor::Posix) {
        session.send_hidden(ShellFlavor::Posix.integration(&command))?;
    }
    Ok((session, shell))
}
'''
p.write_text(s)

p = Path(app+'actions.rs')
s = p.read_text()
start = s.index('        match change {', s.index('let path = match self.settings.save()'))
end = s.index('        self.notice = Some(', start)
presentation = s[start:end]
s = s[:start] + '''        self.apply_shared_settings(self.settings.clone(), change, window, cx);
        cx.emit(WorkspaceEvent::SettingsChanged(self.settings.clone(), change));

''' + s[end:]
anchor = '    fn copy_document('
idx = s.index(anchor)
s = s[:idx] + '''    /// Apply a successfully saved global change without writing the file again.
    pub(super) fn apply_shared_settings(
        &mut self, settings: Settings, change: settings_window::SettingsChange,
        window: &mut Window, cx: &mut Context<Self>,
    ) {
        use settings_window::SettingsChange;
        self.settings = settings;
''' + presentation + '''        cx.notify();
    }

''' + s[idx:]
assert s.count('Action::Quit => cx.quit(),') == 1
s = s.replace('Action::Quit => cx.quit(),', '''Action::Quit | Action::NewTerminal | Action::CloseTerminal
            | Action::NextTerminal | Action::PreviousTerminal
            | Action::MoveTerminalLeft | Action::MoveTerminalRight => {
                cx.emit(WorkspaceEvent::Action(event.action));
            }''')
p.write_text(s)

path = 'crates/kea-app/src/config/keybindings.rs'
actions = [
    ('NewTerminal', 'new_terminal', 'New local terminal', 'ctrl-shift-t'),
    ('CloseTerminal', 'close_terminal', 'Close current terminal', 'ctrl-shift-w'),
    ('NextTerminal', 'next_terminal', 'Next terminal', 'ctrl-tab'),
    ('PreviousTerminal', 'previous_terminal', 'Previous terminal', 'ctrl-shift-tab'),
    ('MoveTerminalLeft', 'move_terminal_left', 'Move terminal left', 'ctrl-shift-pageup'),
    ('MoveTerminalRight', 'move_terminal_right', 'Move terminal right', 'ctrl-shift-pagedown'),
]
replace(path, '    SelectTerminalText,\n', '    SelectTerminalText,\n' + ''.join(f'    {a},\n' for a, _, _, _ in actions))
replace(path, 'const ALL: [Self; 24]', 'const ALL: [Self; 30]')
replace(path, '        Self::SelectTerminalText,\n', '        Self::SelectTerminalText,\n' + ''.join(f'        Self::{a},\n' for a, _, _, _ in actions))
replace(path, '            Self::SelectTerminalText => "select_terminal_text",', '            Self::SelectTerminalText => "select_terminal_text",\n' + ''.join(f'            Self::{a} => "{name}",\n' for a, name, _, _ in actions))
replace(path, '            Self::SelectTerminalText => "Select terminal text",', '            Self::SelectTerminalText => "Select terminal text",\n' + ''.join(f'            Self::{a} => "{label}",\n' for a, _, label, _ in actions))
replace(path, '            (Action::SelectTerminalText, "f4"),', '            (Action::SelectTerminalText, "f4"),\n' + ''.join(f'            (Action::{a}, "{key}"),\n' for a, _, _, key in actions))
replace(path, '                Action::SelectTerminalText => &["Kea > Input", "KeaChrome"],', '''                Action::SelectTerminalText => &["Kea > Input", "KeaChrome"],
                Action::NewTerminal | Action::CloseTerminal | Action::NextTerminal
                | Action::PreviousTerminal | Action::MoveTerminalLeft | Action::MoveTerminalRight => {
                    &["KeaCommand > Input", "KeaChrome"]
                }''')
replace(path, '# select_terminal_text = f4', '# select_terminal_text = f4\n' + '\n'.join(f'# {name} = {key}' for _, name, _, key in actions))

# Keep the existing editor component and successful-submission boundary. Only
# scope its recall adapter by terminal id; shared persistence has ONE writer.
path = 'crates/kea-app/src/editor/command.rs'
replace(path, 'use std::path::PathBuf;', 'use std::{collections::HashMap, path::PathBuf};')
replace(path, 'struct DraftRecallGlobal {\n', '''struct DraftRecallGlobal {
    active_tab: u64,
    inactive: HashMap<u64, TabRecall>,
    persisted: DraftHistory,
    persisted_seed: Vec<String>,
''')
replace(path, 'impl Global for DraftRecallGlobal {}', '''impl Global for DraftRecallGlobal {}

#[derive(Default)]
struct TabRecall {
    history: DraftHistory,
    editor: Option<Entity<InputState>>,
}

/// Switch the owned recall context without resetting navigation or scratch text.
/// A failed/armed submission candidate must never cross a tab switch.
pub fn activate_tab_history(id: u64, cx: &mut App) {
    let recall = cx.default_global::<DraftRecallGlobal>();
    if recall.active_tab == id { return; }
    let old = TabRecall {
        history: std::mem::take(&mut recall.history),
        editor: recall.current_editor.take(),
    };
    recall.inactive.insert(recall.active_tab, old);
    let state = recall.inactive.remove(&id).unwrap_or_else(|| {
        let mut history = DraftHistory::default();
        for text in &recall.persisted_seed { history.record(text.clone()); }
        TabRecall { history, editor: None }
    });
    recall.history = state.history;
    recall.current_editor = state.editor;
    recall.submission_candidate = None;
    recall.active_tab = id;
}

/// Release retained editor entities when a terminal closes.
pub fn forget_tab_history(id: u64, cx: &mut App) {
    let recall = cx.default_global::<DraftRecallGlobal>();
    recall.inactive.remove(&id);
    if recall.active_tab == id {
        recall.history = DraftHistory::default();
        recall.current_editor = None;
        recall.submission_candidate = None;
    }
}
''')
replace(path, '''        cx.default_global::<DraftRecallGlobal>()
            .history
            .record(submitted);''', '''        let recall = cx.default_global::<DraftRecallGlobal>();
        recall.history.record(submitted.clone());
        recall.persisted.record(submitted);''')
replace(path, '''        let editor = editor.downgrade();
        window.defer(cx, move |window, cx| {
            let _ = editor.update(cx, |state, cx| state.focus(window, cx));
        });''', '''        let weak = editor.downgrade();
        window.defer(cx, move |window, cx| {
            let Some(editor) = weak.upgrade() else { return; };
            let is_current = cx.try_global::<DraftRecallGlobal>()
                .and_then(|recall| recall.current_editor.as_ref()) == Some(&editor);
            if is_current { editor.update(cx, |state, cx| state.focus(window, cx)); }
        });''')
replace(path, '''    cx.default_global::<DraftRecallGlobal>()
        .history
        .set_persisted_entries(path, entries);''', '''    let recall = cx.default_global::<DraftRecallGlobal>();
    recall.persisted_seed = entries.clone();
    recall.history = DraftHistory::default();
    for entry in &entries { recall.history.record(entry.clone()); }
    recall.persisted.set_persisted_entries(path, entries);''')
replace(path, '''    cx.default_global::<DraftRecallGlobal>()
        .history
        .take_persistence_warning()''', '''    cx.default_global::<DraftRecallGlobal>()
        .persisted
        .take_persistence_warning()''')

write('crates/kea-app/build.rs', '''
    use std::{env, path::PathBuf};

    fn main() {
        println!("cargo:rerun-if-changed=../../assets/windows/kea.rc");
        println!("cargo:rerun-if-changed=../../assets/windows/kea.ico");
        if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") { return; }
        let assets = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo manifest directory"))
            .join("../../assets/windows");
        embed_resource::compile_for(
            assets.join("kea.rc"), ["kea"],
            embed_resource::ParamsIncludeDirs([assets.as_os_str()]),
        ).manifest_required().expect("Kea Windows application icon must be embedded");
    }
''')
p = Path('crates/kea-app/Cargo.toml')
s = p.read_text()
assert '[build-dependencies]' not in s
p.write_text(s + '\n[build-dependencies]\nembed-resource = "=3.0.6"\n')
write('assets/windows/kea.rc', '''
    // The lowest numbered icon group is Explorer's default executable icon.
    // GPUI's native window class loads the application icon from the executable.
    1 ICON "kea.ico"
''')
write('scripts/generate-windows-icon.py', r'''
    #!/usr/bin/env python3
    """Pack the committed Kea PNG artwork into a deterministic multi-size ICO.

    No image library or network is needed. --check rejects stale/missing assets.
    """
    import argparse
    from pathlib import Path
    import struct

    ROOT = Path(__file__).resolve().parents[1]
    SIZES = (16, 24, 32, 48, 64, 128, 256)

    def encode_icon(images: list[tuple[int, bytes]]) -> bytes:
        if not images or len(images) > 65535:
            raise ValueError("ICO requires 1..65535 images")
        offset = 6 + 16 * len(images)
        entries = []
        payloads = []
        for size, data in images:
            if not 1 <= size <= 256 or len(data) < 33 or data[:8] != b"\x89PNG\r\n\x1a\n":
                raise ValueError("invalid PNG icon")
            if data[12:16] != b"IHDR" or struct.unpack_from(">II", data, 16) != (size, size):
                raise ValueError("PNG dimensions do not match icon size")
            entries.append(struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(data), offset))
            payloads.append(data)
            offset += len(data)
        return struct.pack("<HHH", 0, 1, len(images)) + b"".join(entries + payloads)

    def generated_icon() -> bytes:
        return encode_icon([(size, (ROOT / f"assets/linux/icons/hicolor/{size}x{size}/apps/kea.png").read_bytes()) for size in SIZES])

    def main() -> None:
        parser = argparse.ArgumentParser(description=__doc__)
        parser.add_argument("--check", action="store_true")
        args = parser.parse_args()
        path = ROOT / "assets/windows/kea.ico"
        data = generated_icon()
        if args.check:
            if not path.exists() or path.read_bytes() != data:
                raise SystemExit("Windows icon is missing or stale; run scripts/generate-windows-icon.py")
            print("Windows icon matches all seven committed PNG sizes")
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
            print(f"Wrote {path} ({len(data)} bytes)")

    if __name__ == "__main__":
        main()
''')
write('scripts/verify-windows-icon.py', r'''
    #!/usr/bin/env python3
    """Verify actual PE icon resources, not Windows' generic fallback icon.

    Uses Win32's resource loader as a data file, never executes the application.
    Every embedded image must match the committed source ICO byte for byte.
    """
    import ctypes
    from ctypes import wintypes
    from pathlib import Path
    import struct
    import sys

    def main() -> None:
        if sys.platform != "win32":
            raise SystemExit("Run this verifier on Windows")
        if len(sys.argv) != 2:
            raise SystemExit("usage: verify-windows-icon.py path/to/kea.exe")
        path = Path(sys.argv[1]).resolve(strict=True)
        ico = (Path(__file__).resolve().parents[1] / "assets/windows/kea.ico").read_bytes()
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel.LoadLibraryExW.argtypes = [wintypes.LPCWSTR, wintypes.HANDLE, wintypes.DWORD]
        kernel.LoadLibraryExW.restype = wintypes.HMODULE
        kernel.FindResourceW.argtypes = [wintypes.HMODULE, ctypes.c_void_p, ctypes.c_void_p]
        kernel.FindResourceW.restype = wintypes.HRSRC
        kernel.LoadResource.argtypes = [wintypes.HMODULE, wintypes.HRSRC]
        kernel.LoadResource.restype = wintypes.HGLOBAL
        kernel.LockResource.argtypes = [wintypes.HGLOBAL]
        kernel.LockResource.restype = ctypes.c_void_p
        kernel.SizeofResource.argtypes = [wintypes.HMODULE, wintypes.HRSRC]
        kernel.SizeofResource.restype = wintypes.DWORD
        kernel.FreeLibrary.argtypes = [wintypes.HMODULE]
        kernel.FreeLibrary.restype = wintypes.BOOL
        module = kernel.LoadLibraryExW(str(path), None, 0x00000002 | 0x00000020)
        if not module:
            raise ctypes.WinError(ctypes.get_last_error())
        def resource(kind: int, identity: int) -> bytes:
            handle = kernel.FindResourceW(module, identity, kind)
            if not handle:
                raise RuntimeError(f"Missing PE resource type={kind}, id={identity}")
            size = kernel.SizeofResource(module, handle)
            loaded = kernel.LoadResource(module, handle)
            pointer = kernel.LockResource(loaded) if loaded else None
            if not size or not pointer:
                raise ctypes.WinError(ctypes.get_last_error())
            return ctypes.string_at(pointer, size)
        try:
            group = resource(14, 1) # RT_GROUP_ICON
            reserved, kind, count = struct.unpack_from("<HHH", group)
            if (reserved, kind, count) != struct.unpack_from("<HHH", ico) or count != 7:
                raise RuntimeError("Wrong application icon group")
            for n in range(count):
                actual = struct.unpack_from("<BBBBHHIH", group, 6 + n * 14)
                expected = struct.unpack_from("<BBBBHHII", ico, 6 + n * 16)
                if actual[:7] != expected[:7]:
                    raise RuntimeError(f"Incorrect icon entry {n}")
                payload = resource(3, actual[7]) # RT_ICON
                if payload != ico[expected[7]:expected[7] + expected[6]]:
                    raise RuntimeError(f"Embedded icon payload {n} differs from source")
            print("Verified Kea executable icon: seven exact embedded image resources")
        finally:
            kernel.FreeLibrary(module)

    if __name__ == "__main__":
        main()
''')

# Windows host tests must compile the binary-level workspace as well as the lib.
replace('.github/workflows/ci.yml', 'cargo test --locked -p kea-app --lib', 'cargo test --locked -p kea-app')
replace('.github/workflows/ci.yml', '      - name: Verify Windows GUI subsystem', '''      - name: Verify icon source and embedded application resources
        run: |
          python scripts/generate-windows-icon.py --check
          python scripts/verify-windows-icon.py target/release/kea.exe
      - name: Verify Windows GUI subsystem''')
replace('.github/workflows/ci.yml', '      - run: cargo fmt --all -- --check', '''      - run: python scripts/generate-windows-icon.py --check
      - run: cargo fmt --all -- --check''')

# Reuse the existing component test fixture rather than introducing fake input.
p = Path('crates/kea-app/tests/unit/editor/command.rs')
p.write_text(p.read_text() + r'''

#[gpui::test]
fn terminal_tabs_preserve_independent_recall_and_scratch(cx: &mut TestAppContext) {
    cx.update(|cx| { gpui_component::init(cx); register_languages(); });
    let window = cx.add_window(|window, cx| {
        let view = cx.new(|_| EmptyView);
        Root::new(view, window, cx)
    });
    window.update(cx, |_, window, cx| {
        let a = new_draft(None, &Settings::default(), "submitted A", window, cx);
        a.update(cx, |state, cx| state.focus(window, cx));
        assert_eq!(submission_text(&a, window, cx).as_deref(), Some("submitted A"));
        let a = new_draft(None, &Settings::default(), "", window, cx);
        a.update(cx, |state, cx| {
            state.focus(window, cx);
            state.replace_text_in_range(None, "scratch A", window, cx);
        });
        assert!(navigate_submitted_drafts(HistoryDirection::Previous, window, cx));
        activate_tab_history(1, cx);
        let b = new_draft(None, &Settings::default(), "draft B", window, cx);
        b.update(cx, |state, cx| state.focus(window, cx));
        assert_eq!(submitted_history_len(cx), 0);
        assert!(!navigate_submitted_drafts(HistoryDirection::Previous, window, cx));
        // Reading a candidate is not a successful send; switching discards it.
        assert_eq!(submission_text(&b, window, cx).as_deref(), Some("draft B"));
        activate_tab_history(2, cx);
        let c = new_draft(None, &Settings::default(), "", window, cx);
        assert_eq!(submitted_history_len(cx), 0);
        assert!(c.read(cx).value().is_empty());
        activate_tab_history(0, cx);
        a.update(cx, |state, cx| state.focus(window, cx));
        assert_eq!(submitted_history_len(cx), 1);
        assert_eq!(a.read(cx).value().as_ref(), "submitted A");
        assert!(navigate_submitted_drafts(HistoryDirection::Next, window, cx));
        assert_eq!(a.read(cx).value().as_ref(), "scratch A");
        activate_tab_history(1, cx);
        assert_eq!(submitted_history_len(cx), 0);
        assert_eq!(b.read(cx).value().as_ref(), "draft B");
        forget_tab_history(0, cx);
        assert!(!cx.default_global::<DraftRecallGlobal>().inactive.contains_key(&0));
    }).unwrap();
}

#[gpui::test]
fn interleaved_tabs_share_one_persistence_writer(cx: &mut TestAppContext) {
    cx.update(|cx| { gpui_component::init(cx); register_languages(); });
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("drafts.txt");
    let window = cx.add_window(|window, cx| {
        let view = cx.new(|_| EmptyView);
        Root::new(view, window, cx)
    });
    window.update(cx, |_, window, cx| {
        set_history_persistence(path.clone(), vec!["old".into()], cx);
        for (id, text) in [(0, "A"), (1, "B"), (0, "C")] {
            activate_tab_history(id, cx);
            let draft = new_draft(None, &Settings::default(), text, window, cx);
            draft.update(cx, |state, cx| state.focus(window, cx));
            assert_eq!(submission_text(&draft, window, cx).as_deref(), Some(text));
            new_draft(None, &Settings::default(), "", window, cx);
        }
        assert_eq!(crate::editor::history::load_history_file(&path).unwrap(), vec!["old", "A", "B", "C"]);
        assert_eq!(submitted_history_len(cx), 3); // old, A, C; B belongs to tab 1
        activate_tab_history(1, cx);
        assert_eq!(submitted_history_len(cx), 2); // old, B
    }).unwrap();
}
''')
p = Path('crates/kea-app/tests/unit/config/keybindings.rs')
p.write_text(p.read_text() + r'''

#[test]
fn terminal_actions_are_configurable_and_round_trip() {
    let map = Keymap::parse("new_terminal = alt-t\nclose_terminal = none\n").unwrap();
    assert_eq!(map.action_for(&Keystroke::parse("alt-t").unwrap()), Some(Action::NewTerminal));
    assert_eq!(map.action_for(&Keystroke::parse("ctrl-tab").unwrap()), Some(Action::NextTerminal));
    assert_eq!(map.label(Action::CloseTerminal), "unbound");
    assert_eq!(Keymap::parse(&map.to_config()).unwrap(), map);
    assert!(Keymap::parse("new_terminal = ctrl-enter").is_err());
}
''')

write('docs/terminal-tabs.md', '''
    ---
    title: Terminal tabs
    nav_order: 15
    ---

    # Independent terminal tabs

    Each tab owns a PTY/process, live and historical emulator, recording, blocks,
    shell/input metadata, composer editor/undo, recall cursor and scratch draft,
    completion request, reverse-search view and focus state. Switching never
    recreates the session or reruns a command. Hidden sessions continue consuming
    output, replying to terminal protocols and writing explicit recordings.

    Use **+** or **Ctrl+Shift+T** to open a fresh local shell. New tabs start in
    Kea's launch directory; Kea never changes its process-wide cwd or guesses a
    remote directory. The initial CLI command and --record path apply only to the
    initial tab. A new terminal does not repeat a command, SSH login or recording.

    From composer/chrome, **Ctrl+Tab / Ctrl+Shift+Tab** switch terminals,
    **Ctrl+Shift+W** closes one, and **Ctrl+Shift+PageUp / PageDown** reorder it.
    Tabs also support clicking, close buttons and drag reordering. Shortcuts are
    semantic and configurable in Settings (`new_terminal`, `close_terminal`,
    `next_terminal`, `previous_terminal`, `move_terminal_left`,
    `move_terminal_right`). While a live child owns focus, these keys remain
    child input. Use the existing Ctrl/Cmd+L escape first, or the visible buttons.

    Closing a running terminal, nonempty draft or unsaved recording requires
    confirmation. Closing the window confirms all affected terminals. A tab close
    drops only its owned session and stops its pump; other sessions keep running.
    Closing the final tab leaves an empty workspace with a New terminal button.
    Reordering and confirmation use stable IDs, never potentially stale indices.
    At most 32 tabs can be open to bound aggregate resource retention.

    Settings and explicitly saved input memories are workspace-wide. Composer
    recall/navigation stays tab-local; opted-in draft-history persistence uses
    one shared archive writer so interleaved submissions cannot overwrite each
    other's file snapshots. Prior persisted entries seed each tab. Persistence
    changes still take effect on restart. Nothing enables disk history implicitly.

    Changing tabs cancels pending submission confirmation and completion; a late
    completion or deferred focus cannot target a different terminal. Draft text,
    undo, history scratch and scrollback remain owned by their original tab.
    Only the visible terminal is resized from actual canvas geometry; hidden
    terminals retain their last nonzero dimensions.

    This change provides tabs, not split panes or process restoration after exit.
    Tab/context ownership is independent of layout, allowing future splits without
    moving session state into a shared mutable terminal singleton.

    ## Acceptance checks

    Open two tabs; run a long-lived command in one and edit a distinct draft in
    the other. Switch repeatedly and verify ongoing output, independent undo and
    recall, TUI state, terminal selection and per-tab Save/History controls.
    Reorder with mouse and keyboard; close a background tab and cancel/confirm
    close prompts. Check quitting, closing the final tab, opening after that,
    completion finishing after a switch, and launch failure preserving all tabs.
    Verify native TUI keys before using Ctrl/Cmd+L and the workspace shortcuts.
    Test Windows PowerShell/SSH/OpenCode and actual platform IMEs separately from
    Rust unit/component tests. No session persistence or remote cwd inference is
    implied by successful graphical smoke tests.
''')
write('docs/windows-icon.md', '''
    # Windows executable icon

    The portable kea.exe embeds a multi-resolution icon; no installer, signature,
    administrator access or external image file is needed for Explorer to display
    it. This does not bypass SmartScreen or application-control policy.

    `assets/windows/kea.ico` packages the existing Kea PNG artwork at 16, 24, 32,
    48, 64, 128 and 256 pixels. Regenerate with
    `python scripts/generate-windows-icon.py`; CI uses `--check` to reject drift.
    `kea-app/build.rs` compiles the Windows resource into the kea binary only and
    treats resource compilation failures as build failures. Windows CI inspects
    the actual PE resource group and compares every image with the source ICO.

    Explorer/Start shortcuts use the executable's icon. Pinned shortcuts to an old
    path or an explicitly overridden icon can still show an old image; recreate
    that shortcut after replacing the executable. Runtime taskbar/Alt-Tab and
    Start-menu appearance must also be checked on Windows, not inferred from a
    Linux build or resource-file existence alone.
''')

p = Path('README.md')
s = p.read_text().replace('Kea keeps a terminal and a multiline editor visible together over one session.', 'Kea keeps a terminal and a multiline editor visible together in each independent tab.')
s = s.replace('## Why Kea?', 'See [Terminal tabs](docs/terminal-tabs.md) for creation, switching, reordering and per-tab state.\n\n## Why Kea?')
s = s.replace('| Newline in the composer | Enter |', '| New terminal / close terminal | Ctrl+Shift+T / Ctrl+Shift+W |\n| Next / previous terminal | Ctrl+Tab / Ctrl+Shift+Tab |\n| Newline in the composer | Enter |')
p.write_text(s)
p = Path('AGENTS.md')
s = p.read_text()
s = s.replace('Kea has one terminal session and a persistent editor visible together.', 'Each Kea tab owns one terminal session and a persistent editor visible together.')
s += '\nMulti-terminal ownership: read docs/terminal-tabs.md. Never share a PTY, input context, pending submission, completion result or draft recall cursor across tabs. Global settings and explicit saved memories may be shared.\n'
p.write_text(s)
for file in ['docs/architecture.md', 'docs/unified-session.md']:
    p = Path(file)
    s = p.read_text().replace('# One session, independent surfaces', '# Per-tab sessions, independent surfaces')
    s = s.replace('Kea has one live terminal session and one persistent editor surface.', 'Each Kea tab has one live terminal session and one persistent editor surface.')
    s += '\nWindow-level ownership and isolation are specified in [Terminal tabs](terminal-tabs.md).\n'
    p.write_text(s)
