use super::*;
use anyhow::{Context as _, Result};
use gpui_component::TitleBar;
use kea_app::{
    editor::history as draft_history, history::session_files, reverse_search::view, update,
};
use std::{ffi::OsString, fs::File, path::PathBuf};

pub(super) fn run() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let (mut demo, mut replay, mut record, mut terminal_focus) = (false, None, None, false);
    let mut command: Vec<OsString> = Vec::new();
    while let Some(arg) = args.next() {
        if arg == "--print-shell-integration" {
            let name = args
                .next()
                .context("--print-shell-integration needs a shell name")?;
            let context = args
                .next()
                .context("--print-shell-integration needs a unique context id")?;
            let context = context.to_str().context("context id must be UTF-8")?;
            anyhow::ensure!(
                context != "local"
                    && !context.is_empty()
                    && context.len() <= 128
                    && context
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)),
                "use a non-local alphanumeric context id (dash, dot and underscore are allowed)"
            );
            anyhow::ensure!(
                args.next().is_none(),
                "unexpected shell-integration arguments"
            );
            let flavor =
                ShellFlavor::from_program(&name.to_string_lossy()).context("unsupported shell")?;
            use std::io::Write as _;
            // Remote/nested hooks use the persistent remote token; it never
            // speaks for the local shell (see kea_app::shell::token).
            let token = kea_app::shell::token::remote_token()?;
            std::io::stdout().write_all(&flavor.integration_for(&[name], context, &token))?;
            return Ok(());
        }
        if arg == "--" {
            command.extend(args);
            break;
        }
        if arg == "--demo" {
            demo = true;
        } else if arg == "--direct" || arg == "--terminal-focus" {
            terminal_focus = true;
        } else if arg == "--replay" {
            replay = Some(PathBuf::from(args.next().context("--replay needs a path")?));
        } else if arg == "--record" {
            record = Some(PathBuf::from(
                args.next().context("--record needs a new file path")?,
            ));
        } else if arg == "--help" || arg == "-h" {
            let help = "Kea — independent terminal tabs with persistent text editors

kea [--terminal-focus] [--record NEW.kea] [-- PROGRAM ARG...]
kea --replay SESSION.kea
kea --demo

Each tab keeps a terminal and editor visible together; focus decides who receives keyboard input.
From composer: Ctrl+Shift+T opens a local shell, Ctrl+Shift+W closes a tab, Ctrl+Tab switches.
Ctrl+Shift+PageUp/PageDown reorders tabs. Live terminal input keeps its native shortcuts.
Editor defaults: Enter = new line, Ctrl+Enter = Submit to terminal, Tab = complete.
Unknown or nonempty terminal input requires a second Enter. Other keys cancel.
Ctrl+Shift+Enter remains a compatibility alias with the same safety policy.
kea --print-shell-integration bash remote-id prints optional nested/remote integration.
Ctrl+R in the compose editor opens history and saved memories; Enter inserts, never runs.
All semantic shortcuts are configurable. Terminal-like editor behavior is possible with:
  run_shell = enter
  newline = shift-enter

Blocks are an optional observational view. They never gate command execution.
The current integrated local-shell directory is reported by shell hooks; while a TUI/remote app owns the terminal, Kea labels it last reported rather than guessing.
Terminal Tab and other representable keys go to the child application.
Mouse buttons/wheel are forwarded when the child negotiates mouse reporting; hold Shift for local selection/scrollback.
Modified Enter is distinguished only after the child negotiates an extended keyboard protocol.

KEA_KEYBINDINGS and KEA_SETTINGS select explicit configuration files.
Input history stays session-only unless history_persistence = true. Explicit named saves persist.
KEA_MEMORY_DIR selects an absolute input-memory directory. See docs/reverse-search.md.
Sessions are temporary unless Save session or --record is used. Saved recordings are bounded and unencrypted; commands/output can contain secrets.";
            #[cfg(not(windows))]
            println!("{help}");
            #[cfg(windows)]
            show_message(help.to_string());
            return Ok(());
        } else if arg.to_string_lossy().starts_with('-') {
            anyhow::bail!("unknown option: {}", arg.to_string_lossy());
        } else {
            command.push(arg);
            command.extend(args);
            break;
        }
    }

    let replay_requested = replay.is_some();
    if (demo && replay_requested)
        || ((demo || replay_requested) && (record.is_some() || !command.is_empty()))
    {
        anyhow::bail!(
            "--demo and --replay cannot be combined with each other, --record, or a command"
        );
    }

    let mut bootstrap = None;
    let (session, shell, marker_auth) = if demo {
        (Session::demo()?, None, kea_core::MarkerAuth::Replay)
    } else if let Some(path) = replay {
        let loaded = kea_core::read_from(File::open(path)?)?;
        let mut session = Session::from_recording(loaded.recording)?;
        session.go_live();
        if loaded.truncated_tail {
            session.warning =
                Some("Recovered complete events; the final recording frame was truncated.".into());
        }
        (session, None, kea_core::MarkerAuth::Replay)
    } else {
        let spawned = spawn_terminal(command, record.as_deref())?;
        bootstrap = spawned.bootstrap;
        (
            spawned.session,
            spawned.shell,
            kea_app::shell::token::live_auth(spawned.token),
        )
    };

    let document = Document::from_recording(session.recording());
    let initial_focus = if demo || terminal_focus || shell.is_none() {
        InitialFocus::Terminal
    } else {
        InitialFocus::Editor
    };
    let (keymap, warning) = Keymap::load();
    let (settings, settings_warning) = Settings::load();
    let mut warnings: Vec<String> = warning.into_iter().chain(settings_warning).collect();
    // Load explicit draft-history persistence before entering the UI loop.
    let persisted_history = if settings.persist_history && !demo && !replay_requested {
        match session_files::session_directory() {
            Ok(directory) => {
                let path = directory.join("draft-history.txt");
                match draft_history::load_history_file(&path) {
                    Ok(entries) => Some((path, entries)),
                    Err(error) => {
                        warnings.push(format!(
                            "Draft history will not persist: {}: {error}.",
                            path.display()
                        ));
                        None
                    }
                }
            }
            Err(error) => {
                warnings.push(format!("Draft history will not persist: {error}."));
                None
            }
        }
    } else {
        None
    };
    if !demo && !replay_requested && shell.is_none() {
        warnings.push(
            "No integrated shell detected. Submit remains available with confirmation; native terminal input and Tab are unchanged."
                .into(),
        );
    }
    if let Some(update_notice) = update::startup_failure_notice() {
        warnings.push(update_notice);
    }
    if update::supported() {
        update::remove_stale_staged_updates();
    }
    let notice = (!warnings.is_empty()).then(|| format!("Warning: {}", warnings.join(" ")));

    Application::new()
        .with_assets(gpui_component_assets::Assets)
        .run(move |cx: &mut App| {
            gpui_component::init(cx);
            command_editor::register_languages();
            keymap.install(cx);
            if let Some((path, entries)) = persisted_history {
                command_editor::set_history_persistence(path, entries, cx);
            }
            view::install(cx);
            cx.on_window_closed(|cx| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1050.), px(780.)),
                    cx,
                ))),
                window_min_size: Some(size(px(760.), px(500.))),
                titlebar: Some(TitlebarOptions {
                    title: Some("Kea".into()),
                    ..TitleBar::title_bar_options()
                }),
                window_decorations: cfg!(target_os = "linux").then_some(WindowDecorations::Client),
                // Group the window under the installed desktop entry's app id.
                app_id: Some("kea".into()),
                ..Default::default()
            };
            if let Err(error) = cx.open_window(options, |window, cx| {
                rendering::apply_appearance(&settings, window, cx);
                let view = cx.new(|cx| {
                    let mut view = KeaView::new(
                        session,
                        document,
                        shell,
                        keymap,
                        settings,
                        initial_focus,
                        notice,
                        window,
                        cx,
                    );
                    view.set_marker_auth(marker_auth);
                    view.bootstrap_file = bootstrap;
                    view
                });
                let weak = view.downgrade();
                window.defer(cx, move |window, cx| {
                    let _ = weak.update(cx, |view, cx| {
                        if initial_focus == InitialFocus::Terminal {
                            window.focus(&view.focus);
                        } else {
                            view.focus_editor(window, cx);
                        }
                    });
                });
                let app = cx.new(|cx| KeaRoot::new(view, window, cx));
                let weak = app.downgrade();
                window.on_window_should_close(cx, move |window, cx| {
                    weak.update(cx, |root, cx| root.should_close(window, cx))
                        .unwrap_or(true)
                });
                cx.new(|cx| Root::new(app, window, cx))
            }) {
                eprintln!("kea: cannot open window: {error:#}");
                cx.quit();
            }
            cx.activate(true);
        });
    Ok(())
}

#[cfg(windows)]
pub(super) fn show_message(message: String) {
    struct Message(String);
    impl Render for Message {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .p_4()
                .bg(rgb(0x171b20))
                .text_color(rgb(0xd7dae0))
                .child(self.0.clone())
        }
    }
    Application::new().run(move |cx| {
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let _ = cx.open_window(WindowOptions::default(), |_, cx| {
            cx.new(|_| Message(message))
        });
        cx.activate(true);
    });
}

/// A freshly spawned terminal and what its view needs to observe it.
pub(super) struct SpawnedTerminal {
    pub session: Session,
    /// `None` when Kea did not integrate the shell; it then runs untracked.
    pub shell: Option<ShellFlavor>,
    /// Embedded in the integration when the shell is integrated, otherwise
    /// known to nobody, so printed markers are inert.
    pub token: kea_core::MarkerToken,
    /// Deleted when dropped; the view drops it once the shell has reported.
    pub bootstrap: Option<kea_app::shell::BootstrapFile>,
}

/// Shared startup path for a fresh local terminal. Never changes process-wide cwd.
/// The caller alone supplies explicit command/recording options for the first tab.
/// Integration is optional: if it cannot be prepared, the shell still starts,
/// untracked, with a visible warning. Only spawning the process itself can fail.
pub(super) fn spawn_terminal(
    mut command: Vec<OsString>,
    record: Option<&std::path::Path>,
) -> Result<SpawnedTerminal> {
    #[cfg(windows)]
    if command.is_empty() {
        command = vec!["powershell.exe".into(), "-NoLogo".into(), "-NoExit".into()];
    }
    let detected = ShellFlavor::detect(&command);
    let (token, mut integration_error) = match kea_app::shell::token::new_session_token() {
        Ok(token) => (token, None),
        Err(error) => (kea_app::shell::token::fallback_token(), Some(error)),
    };
    let mut shell = detected.filter(|_| integration_error.is_none());
    // Install after the user's profile; do not inject source through PSReadLine.
    if shell == Some(ShellFlavor::PowerShell) {
        if command.is_empty() {
            command.push(std::env::var_os("SHELL").context("PowerShell executable unavailable")?);
        }
        let script = String::from_utf8(ShellFlavor::PowerShell.integration(&command, &token))?;
        if !command
            .iter()
            .any(|arg| arg.to_string_lossy().eq_ignore_ascii_case("-noexit"))
        {
            command.push("-NoExit".into());
        }
        command.push("-Command".into());
        command.push(script.into());
    }
    // Prepare the file before spawning, so a failure leaves nothing typed.
    let bootstrap = if shell == Some(ShellFlavor::Posix) {
        let preferred = kea_app::shell::bootstrap_directory();
        let fallback = std::env::temp_dir();
        match ShellFlavor::posix_bootstrap(&command, &token, &preferred).or_else(|error| {
            if preferred == fallback {
                Err(error)
            } else {
                ShellFlavor::posix_bootstrap(&command, &token, &fallback)
            }
        }) {
            Ok(bootstrap) => Some(bootstrap),
            Err(error) => {
                integration_error = Some(error);
                shell = None;
                None
            }
        }
    } else {
        None
    };
    let mut session = Session::spawn(&command, kea_core::Size::new(100, 26)?, record)?;
    let bootstrap = match bootstrap {
        Some((line, file)) => match session.send_hidden(line) {
            Ok(()) => Some(file),
            Err(error) => {
                integration_error = Some(error);
                shell = None;
                None
            }
        },
        None => None,
    };
    if let Some(error) = integration_error.filter(|_| detected.is_some()) {
        session.warning = Some(format!(
            "Shell integration is off for this terminal ({error:#}); \
             it runs untracked, so Submit asks for confirmation."
        ));
    }
    Ok(SpawnedTerminal {
        session,
        shell,
        token,
        bootstrap,
    })
}
