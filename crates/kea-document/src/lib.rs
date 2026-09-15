//! Structured command/output documents layered over an ordinary terminal session.
//!
//! The terminal byte stream remains the compatibility substrate. Document mode adds
//! private OSC markers so command boundaries never have
//! to be guessed from prompts or terminal text.
use kea_core::{Kind, Recording};
use std::fmt;

const MARKER_PREFIX: &[u8] = b"\x1b]777;kea;";
const MARKER_END: u8 = 0x07;
pub const MAX_COMMAND_BYTES: usize = 64 * 1024;
const MAX_MARKER_BYTES: usize = 96 * 1024;
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
                    document.abort_in_flight(event.at);
                }
                Kind::Resize(_) => {}
            }
        }
        document
    }

    /// Last explicitly reported shell directory, never inferred from screen text.
    /// A direct/remote application may have a different directory.
    pub fn current_directory(&self) -> Option<&str> {
        self.directory.as_deref()
    }

    pub fn blocks(&self) -> &[CommandBlock] {
        &self.blocks
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
        self.next_id = self.next_id.max(id.saturating_add(1));
        self.retained_bytes += input.len();
        self.blocks.push(CommandBlock {
            id,
            input,
            status: CommandStatus::Queued,
            queued_at: at,
            started_at: None,
            finished_at: None,
            truncated: false,
            output: Vec::new(),
        });
        Ok(())
    }

    pub fn ingest_output(&mut self, at: u64, bytes: &[u8]) -> bool {
        let mut changed = false;
        for piece in self.scanner.push(bytes) {
            match piece {
                Piece::Data(data) => {
                    if let Some(id) = self.active {
                        if let Some(index) = self.blocks.iter().position(|block| block.id == id) {
                            let remaining = MAX_DOCUMENT_BYTES.saturating_sub(self.retained_bytes);
                            let retained = self.blocks[index].append_output(&data, remaining);
                            self.retained_bytes += retained;
                            if retained != data.len() {
                                self.saturated |= remaining == 0;
                            }
                            changed |= !data.is_empty();
                        }
                    }
                }
                Piece::Marker(Marker::Directory(path)) => {
                    self.directory = Some(path);
                    changed = true;
                }
                Piece::Marker(Marker::Start(id, input)) => {
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
                            self.retained_bytes += input.len();
                            self.blocks.push(CommandBlock {
                                id,
                                input: input.clone(),
                                status: CommandStatus::Queued,
                                queued_at: at,
                                started_at: None,
                                finished_at: None,
                                truncated: false,
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
                    if self.active == Some(id) {
                        if let Some(block) = self.block_mut(id) {
                            if block.status == CommandStatus::Running {
                                block.status = CommandStatus::Finished(status);
                                block.finished_at = Some(at);
                                changed = true;
                            }
                        }
                        self.active = None;
                    }
                }
            }
        }
        changed
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
            && input.len() <= MAX_DOCUMENT_BYTES.saturating_sub(self.retained_bytes)
    }

    fn block_mut(&mut self, id: u64) -> Option<&mut CommandBlock> {
        self.blocks.iter_mut().find(|block| block.id == id)
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

#[derive(Clone, Debug, Eq, PartialEq)]
enum Marker {
    Directory(String),
    Start(u64, String),
    Done(u64, i32),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Piece {
    Data(Vec<u8>),
    Marker(Marker),
}

#[derive(Debug, Default)]
struct MarkerScanner {
    pending: Vec<u8>,
}

impl MarkerScanner {
    fn push(&mut self, bytes: &[u8]) -> Vec<Piece> {
        self.pending.extend_from_slice(bytes);
        let mut pieces = Vec::new();
        loop {
            let Some(prefix_at) = find_bytes(&self.pending, MARKER_PREFIX) else {
                let keep = suffix_prefix_len(&self.pending, MARKER_PREFIX);
                let emit = self.pending.len().saturating_sub(keep);
                if emit != 0 {
                    pieces.push(Piece::Data(self.pending.drain(..emit).collect()));
                }
                break;
            };
            if prefix_at != 0 {
                pieces.push(Piece::Data(self.pending.drain(..prefix_at).collect()));
            }
            let Some(end) = self.pending.iter().position(|byte| *byte == MARKER_END) else {
                if self.pending.len() > MAX_MARKER_BYTES {
                    pieces.push(Piece::Data(vec![self.pending.remove(0)]));
                    continue;
                }
                break;
            };
            let raw: Vec<u8> = self.pending.drain(..=end).collect();
            match parse_marker(&raw) {
                Some(marker) => pieces.push(Piece::Marker(marker)),
                None => pieces.push(Piece::Data(raw)),
            }
        }
        pieces
    }
}

fn parse_marker(raw: &[u8]) -> Option<Marker> {
    if !raw.starts_with(MARKER_PREFIX) || raw.last().copied() != Some(MARKER_END) {
        return None;
    }
    let body = std::str::from_utf8(&raw[MARKER_PREFIX.len()..raw.len() - 1]).ok()?;
    let mut parts = body.split(';');
    match parts.next()? {
        "cwd" => {
            let encoded = parts.next()?;
            if parts.next().is_some() || encoded.len() > 10924 {
                return None;
            }
            let path = String::from_utf8(base64_decode(encoded)?).ok()?;
            if path.is_empty() || path.len() > 8192 || path.contains('\0') {
                return None;
            }
            Some(Marker::Directory(path))
        }
        "start" => {
            let id = parts.next()?.parse().ok()?;
            let encoded = parts.next()?;
            if parts.next().is_some() {
                return None;
            }
            let input = String::from_utf8(base64_decode(encoded)?).ok()?;
            Some(Marker::Start(id, input))
        }
        "done" => {
            let id = parts.next()?.parse().ok()?;
            let status = parts.next()?.parse().ok()?;
            (parts.next().is_none()).then_some(Marker::Done(id, status))
        }
        _ => None,
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn suffix_prefix_len(bytes: &[u8], prefix: &[u8]) -> usize {
    (1..=bytes.len().min(prefix.len().saturating_sub(1)))
        .rev()
        .find(|length| bytes[bytes.len() - length..] == prefix[..*length])
        .unwrap_or(0)
}

pub fn encode_input(input: &str) -> Result<String, Error> {
    validate_input(input)?;
    Ok(base64_encode(input.as_bytes()))
}

fn validate_input(input: &str) -> Result<(), Error> {
    if input.is_empty() {
        return Err(Error::EmptyCommand);
    }
    if input.len() > MAX_COMMAND_BYTES {
        return Err(Error::CommandTooLarge);
    }
    if input.contains('\0') {
        return Err(Error::NulInCommand);
    }
    Ok(())
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = chunk.get(1).copied().unwrap_or(0);
        let c = chunk.get(2).copied().unwrap_or(0);
        out.push(TABLE[(a >> 2) as usize] as char);
        out.push(TABLE[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[(((b & 0x0f) << 2) | (c >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(c & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn base64_decode(input: &str) -> Option<Vec<u8>> {
    if !input.len().is_multiple_of(4) {
        return None;
    }
    let (chunks, remainder) = input.as_bytes().as_chunks::<4>();
    if !remainder.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(chunks.len() * 3);
    for (index, chunk) in chunks.iter().enumerate() {
        let last = index + 1 == chunks.len();
        let a = base64_value(chunk[0])?;
        let b = base64_value(chunk[1])?;
        match (chunk[2], chunk[3]) {
            (b'=', b'=') => {
                if !last || b & 0x0f != 0 {
                    return None;
                }
                out.push((a << 2) | (b >> 4));
            }
            (b'=', _) => return None,
            (c, b'=') => {
                if !last {
                    return None;
                }
                let c = base64_value(c)?;
                if c & 0x03 != 0 {
                    return None;
                }
                out.push((a << 2) | (b >> 4));
                out.push((b << 4) | (c >> 2));
            }
            (c, d) => {
                let c = base64_value(c)?;
                let d = base64_value(d)?;
                out.push((a << 2) | (b >> 4));
                out.push((b << 4) | (c >> 2));
                out.push((c << 6) | d);
            }
        }
    }
    Some(out)
}

fn base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn terminalish_text(bytes: &[u8]) -> String {
    let stripped = strip_escape_sequences(bytes);
    let text = String::from_utf8_lossy(&stripped);
    let mut lines: Vec<Vec<char>> = vec![Vec::new()];
    let mut row = 0usize;
    let mut column = 0usize;
    for ch in text.chars() {
        match ch {
            '\r' => column = 0,
            '\n' => {
                row += 1;
                column = 0;
                if lines.len() <= row {
                    lines.push(Vec::new());
                }
            }
            '\u{8}' => column = column.saturating_sub(1),
            '\t' => {
                let next = (column / 8 + 1) * 8;
                while column < next {
                    write_char(&mut lines[row], column, ' ');
                    column += 1;
                }
            }
            ch if !ch.is_control() => {
                write_char(&mut lines[row], column, ch);
                column += 1;
            }
            _ => {}
        }
    }
    let mut rendered = lines
        .into_iter()
        .map(|mut line| {
            while line.last() == Some(&' ') {
                line.pop();
            }
            line.into_iter().collect::<String>()
        })
        .collect::<Vec<_>>();
    while rendered.last().is_some_and(String::is_empty) {
        rendered.pop();
    }
    rendered.join("\n")
}

fn write_char(line: &mut Vec<char>, column: usize, ch: char) {
    if line.len() <= column {
        line.resize(column + 1, ' ');
    }
    line[column] = ch;
}

fn strip_escape_sequences(bytes: &[u8]) -> Vec<u8> {
    #[derive(Clone, Copy)]
    enum State {
        Ground,
        Escape,
        Csi,
        String,
        StringEscape,
    }
    let mut state = State::Ground;
    let mut out = Vec::with_capacity(bytes.len());
    for &byte in bytes {
        state = match state {
            State::Ground => match byte {
                0x1b => State::Escape,
                0x00..=0x06 | 0x07 | 0x0b..=0x0c | 0x0e..=0x1f | 0x7f => State::Ground,
                _ => {
                    out.push(byte);
                    State::Ground
                }
            },
            State::Escape => match byte {
                b'[' => State::Csi,
                b']' | b'P' | b'X' | b'^' | b'_' => State::String,
                _ => State::Ground,
            },
            State::Csi => {
                if (0x40..=0x7e).contains(&byte) {
                    State::Ground
                } else {
                    State::Csi
                }
            }
            State::String => match byte {
                0x07 => State::Ground,
                0x1b => State::StringEscape,
                _ => State::String,
            },
            State::StringEscape => {
                if byte == b'\\' {
                    State::Ground
                } else if byte == 0x1b {
                    State::StringEscape
                } else {
                    State::String
                }
            }
        };
    }
    out
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
mod tests {
    use super::*;
    use kea_core::Size;

    fn marker_start(id: u64, input: &str) -> Vec<u8> {
        format!(
            "\x1b]777;kea;start;{id};{}\x07",
            encode_input(input).unwrap()
        )
        .into_bytes()
    }

    fn marker_done(id: u64, code: i32) -> Vec<u8> {
        format!("\x1b]777;kea;done;{id};{code}\x07").into_bytes()
    }

    #[test]
    fn reconstructs_real_command_blocks_without_prompt_guessing() {
        let mut recording = Recording::new(Size::new(80, 24).unwrap()).unwrap();
        let mut output = b"wrapper echo and prompt".to_vec();
        output.extend(marker_start(1, "printf hi"));
        output.extend(b"\x1b[32mhi\x1b[0m");
        output.extend(marker_done(1, 0));
        output.extend(b"prompt again");
        recording.append(2, Kind::Output(output)).unwrap();
        let document = Document::from_recording(&recording);
        assert_eq!(document.blocks().len(), 1);
        let block = &document.blocks()[0];
        assert_eq!(block.input, "printf hi");
        assert_eq!(block.status, CommandStatus::Finished(0));
        assert_eq!(block.plain_output(), "hi");
    }

    #[test]
    fn marker_scanner_handles_arbitrary_chunk_boundaries() {
        let mut document = Document::new();
        document.queue_local(7, "echo split".into(), 1).unwrap();
        let start = marker_start(7, "echo split");
        let split = start.len() / 2;
        let mut first = b"echo wrapper".to_vec();
        first.extend_from_slice(&start[..split]);
        assert!(!document.ingest_output(2, &first));
        let mut second = start[split..].to_vec();
        second.extend_from_slice(b"hel");
        assert!(document.ingest_output(3, &second));
        let mut third = b"lo".to_vec();
        third.extend(marker_done(7, 3));
        third.extend_from_slice(b"prompt");
        assert!(document.ingest_output(4, &third));
        let block = &document.blocks()[0];
        assert_eq!(block.plain_output(), "hello");
        assert_eq!(block.status, CommandStatus::Finished(3));
        assert_eq!(block.started_at, Some(3));
        assert_eq!(block.finished_at, Some(4));
    }

    #[test]
    fn another_command_is_rejected_until_completion() {
        let mut document = Document::new();
        document.queue_local(1, "sleep 1".into(), 0).unwrap();
        assert_eq!(
            document.queue_local(2, "echo nope".into(), 0),
            Err(Error::CommandAlreadyRunning)
        );
        document.ingest_output(1, &marker_start(1, "sleep 1"));
        document.ingest_output(2, &marker_done(1, 0));
        document.queue_local(2, "echo yes".into(), 3).unwrap();
    }

    #[test]
    fn exit_aborts_running_block() {
        let mut document = Document::new();
        document.queue_local(1, "exit".into(), 0).unwrap();
        document.ingest_output(1, &marker_start(1, "exit"));
        assert!(document.abort_in_flight(2));
        assert_eq!(document.blocks()[0].status, CommandStatus::Aborted);
    }

    #[test]
    fn command_input_encoding_round_trips_unicode_and_newlines() {
        let input = "printf 'ä'\nsecond line";
        let encoded = encode_input(input).unwrap();
        assert_eq!(
            String::from_utf8(base64_decode(&encoded).unwrap()).unwrap(),
            input
        );
    }

    #[test]
    fn malformed_base64_markers_are_rejected() {
        for malformed in ["=AAA", "AA=A", "AA==AAAA", "AB==", "AAB="] {
            assert!(base64_decode(malformed).is_none(), "accepted {malformed}");
        }
    }

    #[test]
    fn local_queue_applies_command_validation() {
        let mut document = Document::new();
        assert_eq!(
            document.queue_local(1, String::new(), 0),
            Err(Error::EmptyCommand)
        );
        assert_eq!(
            document.queue_local(1, "x".repeat(MAX_COMMAND_BYTES + 1), 0),
            Err(Error::CommandTooLarge)
        );
    }

    #[test]
    fn terminalish_text_hides_ansi_and_applies_carriage_return_overwrite() {
        assert_eq!(
            terminalish_text(b"\x1b[31mDownloading 10%\x1b[0m\rDownloading 100%\nDone\n"),
            "Downloading 100%\nDone"
        );
    }
}

#[cfg(test)]
mod directory_tests {
    use super::*;
    #[test]
    fn directory_metadata_is_chunk_safe_and_not_command_output() {
        let path = "C:\\Users\\ä user;project";
        let marker = format!("\x1b]777;kea;cwd;{}\x07", base64_encode(path.as_bytes()));
        for split in 0..=marker.len() {
            let mut doc = Document::new();
            doc.ingest_output(0, &marker.as_bytes()[..split]);
            doc.ingest_output(1, &marker.as_bytes()[split..]);
            assert_eq!(doc.current_directory(), Some(path));
            assert!(doc.blocks().is_empty());
        }
    }
    #[test]
    fn directory_markers_do_not_close_or_corrupt_active_commands() {
        let mut doc = Document::new();
        doc.ingest_output(0, b"\x1b]777;kea;start;1;ZWNobyBoaQ==\x07");
        doc.ingest_output(1, b"hi\x1b]777;kea;cwd;L3RtcA==\x07");
        doc.ingest_output(2, b"\x1b]777;kea;done;1;0\x07");
        assert_eq!(doc.current_directory(), Some("/tmp"));
        assert_eq!(doc.blocks()[0].plain_output(), "hi");
        assert_eq!(doc.blocks()[0].status, CommandStatus::Finished(0));
        doc.ingest_output(3, b"\x1b]777;kea;cwd;AA==\x07");
        assert_eq!(doc.current_directory(), Some("/tmp"));
    }
}
