use crate::encoding::base64_decode;

const MARKER_PREFIX: &[u8] = b"\x1b]777;kea;";
const MARKER_END: u8 = 0x07;
const MAX_MARKER_BYTES: usize = 96 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Marker {
    Start(u64, String),
    Done(u64, i32),
    Directory(String),
    Prompt(String),
    NativeStart(String),
    NativeDone(String, i32),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Piece {
    Data(Vec<u8>),
    Marker(Marker),
}

#[derive(Debug, Default)]
pub(crate) struct MarkerScanner {
    pending: Vec<u8>,
}

impl MarkerScanner {
    pub(crate) fn push(&mut self, bytes: &[u8]) -> Vec<Piece> {
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
        kind @ ("cwd" | "prompt") => {
            let encoded = parts.next()?;
            if encoded.len() > 8192 || parts.next().is_some() {
                return None;
            }
            let directory = String::from_utf8(base64_decode(encoded)?).ok()?;
            if (directory.is_empty() && kind != "prompt") || directory.chars().any(char::is_control)
            {
                return None;
            }
            Some(if kind == "prompt" {
                Marker::Prompt(directory)
            } else {
                Marker::Directory(directory)
            })
        }
        "native-start" => {
            let context = parts.next()?;
            (valid_context(context) && parts.next().is_none())
                .then(|| Marker::NativeStart(context.into()))
        }
        "native-done" => {
            let context = parts.next()?;
            let status = parts.next()?.parse().ok()?;
            (valid_context(context) && parts.next().is_none())
                .then(|| Marker::NativeDone(context.into(), status))
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

fn valid_context(context: &str) -> bool {
    !context.is_empty()
        && context.len() <= 128
        && context
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
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

#[cfg(test)]
#[path = "../tests/unit/markers.rs"]
mod tests;
