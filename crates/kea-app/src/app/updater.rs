use super::*;
use gpui_component::WindowExt as _;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum UpdateAction {
    Checking { manual: bool },
    Downloading,
    Verifying,
}

impl KeaRoot {
    pub(super) fn begin_update_check(&mut self, manual: bool, cx: &mut Context<Self>) {
        if !update::supported() {
            if manual {
                self.update_notice =
                    Some("In-app updates are currently available on Windows x86_64.".into());
                cx.notify();
            }
            return;
        }
        if self.update_action.is_some() {
            if manual {
                self.update_notice = Some("An update operation is already running.".into());
                cx.notify();
            }
            return;
        }
        self.update_staged = None;
        self.update_rx = Some(update::check_in_background());
        self.update_action = Some(UpdateAction::Checking { manual });
        if manual {
            self.update_notice = Some("Checking for Kea updates…".into());
        }
        cx.notify();
    }

    pub(super) fn begin_update_download(&mut self, cx: &mut Context<Self>) {
        if self.update_action.is_some() {
            return;
        }
        let Some(update) = self.update_available.clone() else {
            self.begin_update_check(true, cx);
            return;
        };
        self.update_rx = Some(update::stage_in_background(update));
        self.update_action = Some(UpdateAction::Downloading);
        self.update_notice = Some("Downloading and verifying the Kea update…".into());
        cx.notify();
    }

    pub(super) fn poll_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(receiver) = &self.update_rx else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                let manual = matches!(
                    self.update_action,
                    Some(UpdateAction::Checking { manual: true })
                        | Some(UpdateAction::Downloading)
                        | Some(UpdateAction::Verifying)
                );
                self.update_rx = None;
                self.update_action = None;
                if manual {
                    self.notice = Some("The update worker stopped unexpectedly.".into());
                } else {
                    eprintln!("kea: automatic update worker stopped unexpectedly");
                }
                cx.notify();
                return;
            }
        };

        let action = self.update_action.take();
        self.update_rx = None;
        match result {
            update::WorkerResult::Checked(result) => {
                let manual = matches!(action, Some(UpdateAction::Checking { manual: true }));
                match result {
                    Ok(Some(info)) => {
                        let version = info.version.clone();
                        self.update_available = Some(info);
                        self.update_staged = None;
                        self.update_notice = Some(format!(
                            "Kea v{version} is available. Use Update to download it."
                        ));
                    }
                    Ok(None) if manual => {
                        self.update_available = None;
                        self.update_staged = None;
                        self.update_notice =
                            Some(format!("Kea v{} is up to date.", env!("CARGO_PKG_VERSION")));
                    }
                    Ok(None) => {}
                    Err(error) if manual => {
                        self.notice = Some(format!("Update check failed: {error}"));
                    }
                    Err(error) => {
                        eprintln!("kea: automatic update check failed: {error}");
                    }
                }
            }
            update::WorkerResult::Staged(result) => match result {
                Ok(staged) => {
                    let version = staged.version.clone();
                    self.update_available = None;
                    self.update_staged = Some(staged);
                    self.update_notice = Some(format!(
                        "Kea v{version} is downloaded and verified. Restart when convenient."
                    ));
                }
                Err(error) => {
                    self.notice = Some(format!("Update failed: {error}"));
                }
            },
            update::WorkerResult::Verified(result) => match result {
                Ok(()) => {
                    let Some(staged) = self.update_staged.clone() else {
                        self.notice = Some("The staged update is no longer available.".into());
                        cx.notify();
                        return;
                    };
                    match update::launch_staged_update(&staged) {
                        Ok(()) => {
                            self.close_allowed = true;
                            window.defer(cx, |window, _| window.remove_window());
                        }
                        Err(error) => {
                            self.notice = Some(format!("Could not start update: {error:#}"));
                        }
                    }
                }
                Err(error) => {
                    self.notice = Some(format!(
                        "Update verification failed; Kea remains open and the staged installer was not run: {error}"
                    ));
                }
            },
        }
        cx.notify();
    }

    pub(super) fn request_update_restart(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }
        let Some(staged) = self.update_staged.clone() else {
            return;
        };
        let version = staged.version.clone();
        let weak = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, _| {
            let weak = weak.clone();
            let staged = staged.clone();
            dialog
                .title(format!("Restart to update Kea to v{version}?"))
                .child("Kea will close all terminal processes and discard unsaved transient state before running the verified current-user installer. Save anything you want to keep first.")
                .confirm()
                .on_ok(move |_, _, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        this.update_rx = Some(update::verify_staged_in_background(staged));
                        this.update_action = Some(UpdateAction::Verifying);
                        this.update_notice =
                            Some("Verifying the staged Kea update before restart…".into());
                        cx.notify();
                    });
                    true
                })
        });
    }
}
