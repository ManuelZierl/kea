//! Replaceable PTY transport. Neither the history core nor replay depends on it.
use std::{ffi::OsString, io::{Read, Write}, sync::mpsc::{self, Receiver, SyncSender}, thread};
use anyhow::{Context, Result};
use kea_core::Size;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

pub enum Message { Output(Vec<u8>), Eof, Error(String) }

pub struct Pty {
    master: Option<Box<dyn MasterPty + Send>>,
    child: Option<Box<dyn Child + Send + Sync>>,
    input: Option<SyncSender<Vec<u8>>>,
    output: Receiver<Message>,
    eof: bool,
    status: Option<u32>,
}

impl Pty {
    pub fn spawn(command: &[OsString], size: Size) -> Result<Self> {
        Size::new(size.columns, size.rows)?;
        let pair = native_pty_system().openpty(pty_size(size)).context("opening PTY")?;
        let mut builder = if let Some(program) = command.first() {
            let mut builder = CommandBuilder::new(program);
            builder.args(&command[1..]);
            builder
        } else { CommandBuilder::new_default_prog() };
        builder.env("TERM", "xterm-256color");
        builder.env("COLORTERM", "truecolor");
        builder.env("TERM_PROGRAM", "kea");
        // Acquire I/O before spawning so setup failures cannot leak a child.
        let mut reader = pair.master.try_clone_reader().context("opening PTY reader")?;
        let mut writer = pair.master.take_writer().context("opening PTY writer")?;
        let child = pair.slave.spawn_command(builder).context("spawning terminal program")?;
        drop(pair.slave);
        let (output_tx, output) = mpsc::sync_channel(128);
        let errors = output_tx.clone();
        thread::spawn(move || {
            let mut buffer = [0; 16 * 1024];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => { if output_tx.send(Message::Output(buffer[..n].to_vec())).is_err() { return; } }
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(e) if cfg!(target_os = "linux") && e.raw_os_error() == Some(5) => break,
                    Err(e) => { let _ = output_tx.send(Message::Error(e.to_string())); break; }
                }
            }
            let _ = output_tx.send(Message::Eof);
        });
        let (input, input_rx) = mpsc::sync_channel::<Vec<u8>>(64);
        thread::spawn(move || {
            while let Ok(bytes) = input_rx.recv() {
                if let Err(e) = writer.write_all(&bytes).and_then(|_| writer.flush()) {
                    let _ = errors.send(Message::Error(format!("PTY write: {e}")));
                    break;
                }
            }
        });
        Ok(Self { master: Some(pair.master), child: Some(child), input: Some(input), output, eof: false, status: None })
    }
    pub fn try_recv(&mut self) -> Option<Message> {
        let message = self.output.try_recv().ok()?;
        if matches!(message, Message::Eof) { self.eof = true; }
        Some(message)
    }
    pub fn send(&self, bytes: Vec<u8>) -> Result<()> {
        if bytes.len() > kea_core::MAX_OUTPUT { anyhow::bail!("input exceeds 1 MiB"); }
        self.input.as_ref().context("PTY input is closed")?.try_send(bytes).context("PTY input queue is full or closed")
    }
    pub fn resize(&self, size: Size) -> Result<()> {
        Size::new(size.columns, size.rows)?;
        self.master.as_ref().context("PTY is closing")?.resize(pty_size(size)).context("resizing PTY")
    }
    /// Poll child termination independently of EOF, but expose exit only after
    /// the ordered output channel has drained through its EOF event.
    pub fn exit_code(&mut self) -> Result<Option<u32>> {
        if self.status.is_none() {
            if let Some(child) = &mut self.child {
                self.status = child.try_wait()?.map(|status| status.exit_code());
            }
        }
        // An owned Windows HPCON can keep output open after child exit. Close it
        // off the UI/reader thread while the reader drains the final frame.
        #[cfg(windows)]
        if self.status.is_some() {
            self.input.take();
            if let Some(master) = self.master.take() { thread::spawn(move || drop(master)); }
        }
        Ok(if self.eof { self.status } else { None })
    }
}
impl Drop for Pty {
    fn drop(&mut self) {
        self.input.take();
        let master = self.master.take();
        let child = self.child.take();
        // Process waits and ConPTY shutdown must not block the closing GUI.
        thread::spawn(move || {
            if let Some(mut child) = child {
                let _ = child.kill();
                let _ = child.wait();
            }
            drop(master);
        });
    }
}
fn pty_size(size: Size) -> PtySize {
    PtySize { rows: size.rows, cols: size.columns, pixel_width: 0, pixel_height: 0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn captures_output_and_drains_before_exit() {
        #[cfg(unix)]
        let command = vec!["sh".into(), "-c".into(), "printf 'kea-pty-ok'; exit 7".into()];
        #[cfg(windows)]
        let command = vec!["cmd.exe".into(), "/C".into(), "echo kea-pty-ok & exit /b 7".into()];
        let mut pty = Pty::spawn(&command, Size::new(80, 24).unwrap()).unwrap();
        let mut output = Vec::new();
        let start = std::time::Instant::now();
        loop {
            while let Some(message) = pty.try_recv() {
                if let Message::Output(bytes) = message { output.extend_from_slice(&bytes); }
            }
            if let Some(code) = pty.exit_code().unwrap() { assert_eq!(code, 7); break; }
            assert!(start.elapsed().as_secs() < 15, "PTY did not terminate");
            thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(String::from_utf8_lossy(&output).contains("kea-pty-ok"));
    }
}
