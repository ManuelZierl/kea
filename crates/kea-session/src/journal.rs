use anyhow::{Context, Result};
use kea_core::{write_event, write_header, Event, Size};
use std::{
    fs::OpenOptions,
    io::{BufWriter, Write},
    path::Path,
    sync::mpsc::{self, Receiver, SyncSender},
    thread::{self, JoinHandle},
};

/// A bounded, ordered disk writer. No filesystem writes on the render loop.
pub struct Journal {
    sender: Option<SyncSender<Event>>,
    errors: Receiver<String>,
    worker: Option<JoinHandle<()>>,
}
impl Journal {
    pub fn create(path: &Path, size: Size) -> Result<Self> {
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
        write_header(&mut writer, size)?;
        writer.flush()?;
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
            sender: Some(sender),
            errors,
            worker: Some(worker),
        })
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
impl Drop for Journal {
    fn drop(&mut self) {
        self.stop();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
