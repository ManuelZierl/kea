use crate::encoding::validate_input;
use crate::markers::{Marker, MarkerScanner, Piece};
use crate::text::terminalish_text;
use kea_core::{Kind, Recording};
use std::fmt;

pub const MAX_COMMAND_BYTES: usize = 64 * 1024;
pub const MAX_BLOCK_OUTPUT: usize = 4 * 1024 * 1024;
pub const MAX_DOCUMENT_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_BLOCKS: usize = 10_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandStatus {
    Queued,
    Running,
    Finished(i32),
    Aborted,
}

#[derive(Clone, Debug)]
pub struct CommandBlock {
    pub id: u64,
    pub input: String,
    pub status: CommandStatus,
    pub queued_at: u64,
    pub started_at: Option<u64>,
    pub finished_at: Option<u64>,
    pub truncated: bool,
    pub directory: Option<String>,
    output: Vec<u8>,
}

impl CommandBlock {
    pub fn output(&self) -> &[u8] {
        &self.output
    }

    pub fn plain_output(&self) -> String {
        terminalish_text(&self.output)
    }

    pub fn duration_micros(&self) -> Option<u64> {
        Some(self.finished_at?.saturating_sub(self.started_at?))
    }

    fn append_output(&mut self, bytes: &[u8], document_remaining: usize) -> usize {
        if self.output.len() >= MAX_BLOCK_OUTPUT || document_remaining == 0 {
            self.truncated = true;
            return 0;
        }
        let remaining = (MAX_BLOCK_OUTPUT - self.output.len()).min(document_remaining);
        let take = remaining.min(bytes.len());
        self.output.extend_from_slice(&bytes[..take]);
        if take != bytes.len() {
            self.truncated = true;
        }
        take
    }
}

#[derive(Debug, Default)]
pub struct Document {
    blocks: Vec<CommandBlock>,
    active: Option<u64>,
    next_id: u64,
    retained_bytes: usize,
    saturated: bool,
    scanner: MarkerScanner,
    directory: Option<String>,
    prompt_ready: bool,
    submitted: Option<(u64, String, String, u64)>,
    active_context: Option<String>,
}

impl Document {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            ..Self::default()
        }
    }

    pub fn from_recording(recording: &Recording) -> Self {
        let mut document = Self::new();
        for event in recording.events() {
            match &event.kind {
                Kind::Output(bytes) => {
                    document.ingest_output(event.at, bytes);
                }
                Kind::Exit(_) => {
                    document.finish(event.at);
                }
                Kind::Resize(_) => {}
                Kind::Submitted { id, context, input } => {
                    document.note_submission(*id, context, input, event.at);
                }
            }
        }
        document
    }

    /// Associate an accepted authored draft with the next explicit native start.
    /// No block exists before that marker, and missing metadata never gates input.
    pub fn note_submission(&mut self, id: u64, context: &str, input: &str, at: u64) {
        self.submitted = None;
        if self.active.is_none() && validate_input(input).is_ok() && self.can_retain_block(input) {
            self.next_id = self.next_id.max(id.saturating_add(1));
            self.submitted = Some((id, context.to_owned(), input.to_owned(), at));
        }
    }

    pub fn blocks(&self) -> &[CommandBlock] {
        &self.blocks
    }

    /// Last explicit report; never inferred from prompt text.
    pub fn directory(&self) -> Option<&str> {
        self.directory.as_deref()
    }
    pub fn prompt_ready(&self) -> bool {
        self.prompt_ready && !self.has_in_flight()
    }
    pub fn note_terminal_input(&mut self) {
        self.prompt_ready = false;
    }
    pub fn active(&self) -> Option<u64> {
        self.active
    }

    pub fn saturated(&self) -> bool {
        self.saturated
    }

    pub fn has_in_flight(&self) -> bool {
        self.blocks
            .iter()
            .any(|block| matches!(block.status, CommandStatus::Queued | CommandStatus::Running))
    }

    pub fn validate_submission(&self, input: &str) -> Result<(), Error> {
        if self.has_in_flight() {
            return Err(Error::CommandAlreadyRunning);
        }
        validate_input(input)?;
        if !self.can_retain_block(input) {
            return Err(Error::DocumentLimitReached);
        }
        Ok(())
    }

    pub fn allocate_id(&mut self) -> u64 {
        let id = self.next_id.max(1);
        self.next_id = id.saturating_add(1);
        id
    }

    pub fn queue_local(&mut self, id: u64, input: String, at: u64) -> Result<(), Error> {
        self.validate_submission(&input)?;
        if self.blocks.iter().any(|block| block.id == id) {
            return Err(Error::DuplicateCommandId(id));
        }
        self.prompt_ready = false;
        self.next_id = self.next_id.max(id.saturating_add(1));
        self.retained_bytes += input.len() + self.directory.as_ref().map_or(0, String::len);
        self.blocks.push(CommandBlock {
            id,
            input,
            status: CommandStatus::Queued,
            queued_at: at,
            started_at: None,
            finished_at: None,
            truncated: false,
            directory: self.directory.clone(),
            output: Vec::new(),
        });
        Ok(())
    }

    pub fn ingest_output(&mut self, at: u64, bytes: &[u8]) -> bool {
        let mut changed = false;
        for piece in self.scanner.push(bytes) {
            let piece = match piece {
                Piece::Marker(Marker::NativeStart(context)) => {
                    if self
                        .submitted
                        .as_ref()
                        .is_none_or(|(_, owner, _, _)| *owner != context)
                    {
                        continue;
                    }
                    let Some((id, _, input, submitted_at)) = self.submitted.take() else {
                        continue;
                    };
                    // Retain the authored submission time without adding a queued
                    // execution dependency. queue_local is used only after start.
                    if self.queue_local(id, input.clone(), submitted_at).is_err() {
                        continue;
                    }
                    self.active_context = Some(context);
                    Piece::Marker(Marker::Start(id, input))
                }
                piece => piece,
            };
            match piece {
                Piece::Marker(Marker::NativeStart(_)) => unreachable!(),
                Piece::Data(data) => {
                    changed |= self.append_active_output(&data);
                }
                Piece::Marker(Marker::Prompt(directory)) => {
                    // Legacy prompt metadata carries no identity. A nested prompt
                    // must not finish an outer native execution block.
                    if self.active_context.is_none() {
                        self.abort_in_flight(at);
                    }
                    self.submitted = None;
                    self.directory = (!directory.is_empty()).then_some(directory);
                    self.prompt_ready = true;
                    changed = true;
                }
                Piece::Marker(Marker::Directory(directory)) => {
                    self.directory = Some(directory);
                    changed = true;
                }
                Piece::Marker(Marker::Start(id, input)) => {
                    self.prompt_ready = false;
                    if self.active.is_none() {
                        let index = if let Some(index) =
                            self.blocks.iter().position(|block| block.id == id)
                        {
                            index
                        } else {
                            if validate_input(&input).is_err() || !self.can_retain_block(&input) {
                                self.saturated = true;
                                continue;
                            }
                            self.next_id = self.next_id.max(id.saturating_add(1));
                            self.retained_bytes +=
                                input.len() + self.directory.as_ref().map_or(0, String::len);
                            self.blocks.push(CommandBlock {
                                id,
                                input: input.clone(),
                                status: CommandStatus::Queued,
                                queued_at: at,
                                started_at: None,
                                finished_at: None,
                                truncated: false,
                                directory: self.directory.clone(),
                                output: Vec::new(),
                            });
                            self.blocks.len() - 1
                        };
                        let block = &mut self.blocks[index];
                        if block.status == CommandStatus::Queued && block.input == input {
                            block.status = CommandStatus::Running;
                            block.started_at = Some(at);
                            self.active = Some(id);
                            changed = true;
                        }
                    }
                }
                Piece::Marker(Marker::Done(id, status)) => {
                    if self.active_context.is_none() {
                        changed |= self.finish_block(id, status, at);
                    }
                }
                Piece::Marker(Marker::NativeDone(context, status)) => {
                    if self.active_context.as_ref() == Some(&context) {
                        if let Some(id) = self.active {
                            changed |= self.finish_block(id, status, at);
                        }
                    }
                }
            }
        }
        changed
    }

    pub fn finish(&mut self, at: u64) -> bool {
        let pending = self.scanner.finish();
        self.append_active_output(&pending) | self.abort_in_flight(at)
    }

    pub fn abort_in_flight(&mut self, at: u64) -> bool {
        let mut changed = false;
        for block in &mut self.blocks {
            if matches!(block.status, CommandStatus::Queued | CommandStatus::Running) {
                block.status = CommandStatus::Aborted;
                block.finished_at = Some(at);
                changed = true;
            }
        }
        self.active = None;
        self.submitted = None;
        self.active_context = None;
        changed
    }

    pub fn text(&self) -> String {
        let mut out = String::new();
        for block in &self.blocks {
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            out.push_str("$ ");
            out.push_str(&block.input);
            let output = block.plain_output();
            if !output.is_empty() {
                out.push('\n');
                out.push_str(&output);
            }
            out.push('\n');
            out.push_str(&status_label(block));
        }
        if self.saturated {
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            out.push_str("[Kea document retention limit reached]");
        }
        out
    }

    fn can_retain_block(&self, input: &str) -> bool {
        self.blocks.len() < MAX_BLOCKS
            && input.len() + self.directory.as_ref().map_or(0, String::len)
                <= MAX_DOCUMENT_BYTES.saturating_sub(self.retained_bytes)
    }

    fn block_mut(&mut self, id: u64) -> Option<&mut CommandBlock> {
        self.blocks.iter_mut().find(|block| block.id == id)
    }

    fn append_active_output(&mut self, data: &[u8]) -> bool {
        let Some(id) = self.active else {
            return false;
        };
        let Some(index) = self.blocks.iter().position(|block| block.id == id) else {
            return false;
        };
        let remaining = MAX_DOCUMENT_BYTES.saturating_sub(self.retained_bytes);
        let retained = self.blocks[index].append_output(data, remaining);
        self.retained_bytes += retained;
        if retained != data.len() {
            self.saturated |= remaining == 0;
        }
        !data.is_empty()
    }

    fn finish_block(&mut self, id: u64, status: i32, at: u64) -> bool {
        if self.active != Some(id) {
            return false;
        }
        let mut changed = false;
        if let Some(block) = self.block_mut(id) {
            if block.status == CommandStatus::Running {
                block.status = CommandStatus::Finished(status);
                block.finished_at = Some(at);
                changed = true;
            }
        }
        self.active = None;
        self.active_context = None;
        changed
    }
}

pub fn status_label(block: &CommandBlock) -> String {
    match block.status {
        CommandStatus::Queued => "queued".into(),
        CommandStatus::Running => "running".into(),
        CommandStatus::Finished(code) => match block.duration_micros() {
            Some(duration) => format!("exit {code} · {:.3}s", duration as f64 / 1_000_000.0),
            None => format!("exit {code}"),
        },
        CommandStatus::Aborted => "aborted".into(),
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Error {
    EmptyCommand,
    CommandTooLarge,
    NulInCommand,
    CommandAlreadyRunning,
    DuplicateCommandId(u64),
    DocumentLimitReached,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyCommand => f.write_str("command is empty"),
            Self::CommandTooLarge => f.write_str("command exceeds 64 KiB"),
            Self::NulInCommand => f.write_str("command contains NUL"),
            Self::CommandAlreadyRunning => f.write_str("a document command is still running"),
            Self::DuplicateCommandId(id) => write!(f, "duplicate command id {id}"),
            Self::DocumentLimitReached => f.write_str("document retention limit reached"),
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
#[path = "../tests/unit/model.rs"]
mod tests;
