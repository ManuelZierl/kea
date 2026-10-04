use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum UpdateAction {
    Checking { manual: bool },
    Installing,
}

impl KeaView {
    pub(super) fn begin_update_check(&mut self, manual: bool, cx: &mut Context<Self>) {
        if !update::supported() {
            if manual {
                self.notice =
                    Some("In-app updates are currently available on Windows x86_64.".into());
                cx.notify();
            }
            return;
        }
        if self.update_action.is_some() {
            if manual {
                self.notice = Some("An update operation is already running.".into());
                cx.notify();
            }
            return;
        }
        self.update_rx = Some(update::check_in_background());
        self.update_action = Some(UpdateAction::Checking { manual });
        if manual {
            self.notice = Some("Checking for Kea updates…".into());
        }
        cx.notify();
    }

    pub(super) fn begin_update_install(&mut self, cx: &mut Context<Self>) {
        if self.update_action.is_some() {
            return;
        }
        let Some(update) = self.update_available.clone() else {
            self.begin_update_check(true, cx);
            return;
        };
        self.update_rx = Some(update::install_in_background(update));
        self.update_action = Some(UpdateAction::Installing);
        self.notice = Some("Downloading and verifying the Kea update…".into());
        cx.notify();
    }

    pub(super) fn poll_update(&mut self, cx: &mut Context<Self>) {
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
                        | Some(UpdateAction::Installing)
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
                        self.notice = Some(format!(
                            "Kea v{version} is available. Use Update in the toolbar to install it."
                        ));
                    }
                    Ok(None) if manual => {
                        self.update_available = None;
                        self.notice = Some(format!(
                            "Kea v{} is up to date.",
                            env!("CARGO_PKG_VERSION")
                        ));
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
            update::WorkerResult::InstallReady(result) => match result {
                Ok(()) => {
                    self.notice =
                        Some("Update verified. Restarting Kea to finish installation…".into());
                    cx.notify();
                    cx.quit();
                    return;
                }
                Err(error) => {
                    self.notice = Some(format!("Update failed: {error}"));
                }
            },
        }
        cx.notify();
    }
}
