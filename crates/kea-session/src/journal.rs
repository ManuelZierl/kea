use anyhow::{Context, Result};
use kea_alacritty::MouseTracking;
use kea_core::{write_event, write_header, Event, Recording, Size};
use std::{
    fs::OpenOptions,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, SyncSender},
    thread::{self, JoinHandle},
};

/// A bounded, ordered disk writer. No filesystem writes on the render loop after
/// the initial snapshot has been committed.
pub struct Journal {
    path: PathBuf,
    sender: Option<SyncSender<Event>>,
    errors: Receiver<String>,
    worker: Option<JoinHandle<()>>,
}

impl Journal {
    pub fn create(path: &Path, size: Size) -> Result<Self> {
        Self::create_inner(path, size, None)
    }

    /// Start persistence in the middle of a live session. The retained canonical
    /// recording is written synchronously first, then new events continue through
    /// the bounded background writer. Existing files are never overwritten.
    pub fn create_from_recording(path: &Path, recording: &Recording) -> Result<Self> {
        Self::create_inner(path, recording.initial_size(), Some(recording))
    }

    fn create_inner(path: &Path, size: Size, seed: Option<&Recording>) -> Result<Self> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(path)
            .context("creating recording (existing files are never overwritten)")?;
        let mut writer = BufWriter::new(file);
        if let Some(recording) = seed {
            recording.write_to(&mut writer)?;
        } else {
            write_header(&mut writer, size)?;
            writer.flush()?;
        }
        writer.get_ref().sync_all()?;

        let (sender, receiver) = mpsc::sync_channel::<Event>(128);
        let (error_sender, errors) = mpsc::channel();
        let worker = thread::spawn(move || {
            let result = (|| -> std::io::Result<()> {
                while let Ok(event) = receiver.recv() {
                    write_event(&mut writer, &event)?;
                    writer.flush()?;
                }
                writer.flush()?;
                writer.get_ref().sync_all()
            })();
            if let Err(error) = result {
                let _ = error_sender.send(error.to_string());
            }
        });
        Ok(Self {
            path: path.to_path_buf(),
            sender: Some(sender),
            errors,
            worker: Some(worker),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn append(&mut self, event: &Event) -> Result<()> {
        self.sender
            .as_ref()
            .context("recording writer stopped")?
            .try_send(event.clone())
            .context("recording writer fell behind or closed")
    }

    pub fn error(&self) -> Option<String> {
        self.errors.try_recv().ok()
    }

    pub fn stop(&mut self) {
        self.sender.take();
    }
}

/// Small product-facing session extensions that do not mutate canonical history.
/// Terminal protocol state remains owned by the Alacritty projection; the host only
/// reads the negotiated modes when deciding how to encode user input.
impl crate::Session {
    pub fn persistence_path(&self) -> Option<&Path> {
        self.journal.as_ref().map(Journal::path)
    }

    pub fn persistence_active(&self) -> bool {
        self.journal.is_some() && !self.persistence_stopped
    }

    pub fn start_persistence(&mut self, path: &Path) -> Result<()> {
        if self.capture_stopped {
            anyhow::bail!(
                "history retention has already stopped; cannot begin a complete saved session"
            );
        }
        if self.persistence_active() {
            anyhow::bail!("session is already being saved");
        }
        self.journal = None;
        self.persistence_stopped = false;
        self.journal = Some(Journal::create_from_recording(path, &self.recording)?);
        Ok(())
    }

    pub fn stop_persistence(&mut self) {
        if let Some(journal) = &mut self.journal {
            journal.stop();
        }
        self.persistence_stopped = true;
    }

    pub fn terminal_mouse_tracking(&self) -> Option<MouseTracking> {
        self.input_allowed()
            .then(|| self.live.mouse_tracking())
            .flatten()
    }

    pub fn terminal_extended_keyboard(&self) -> bool {
        self.input_allowed() && self.live.extended_keyboard()
    }

    pub fn terminal_focus_reporting(&self) -> bool {
        self.input_allowed() && self.live.focus_reporting()
    }
}

impl Drop for Journal {
    fn drop(&mut self) {
        self.stop();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kea_core::Kind;
    use std::{
        fs::File,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("kea-{name}-{}-{unique}.kea", std::process::id()))
    }

    #[test]
    fn seeded_journal_contains_history_that_predates_saving() {
        let path = temp_path("seeded");
        let mut recording = Recording::new(Size::new(80, 24).unwrap()).unwrap();
        recording
            .append(5, Kind::Output(b"before save\r\n".to_vec()))
            .unwrap();
        {
            let mut journal = Journal::create_from_recording(&path, &recording).unwrap();
            journal
                .append(&Event {
                    at: 10,
                    kind: Kind::Output(b"after save\r\n".to_vec()),
                })
                .unwrap();
        }
        let loaded = kea_core::read_from(File::open(&path).unwrap()).unwrap();
        assert_eq!(loaded.recording.events().len(), 2);
        assert_eq!(loaded.recording.events()[0].at, 5);
        assert_eq!(loaded.recording.events()[1].at, 10);
        let _ = std::fs::remove_file(path);
    }
}
