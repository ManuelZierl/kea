//! Live shell metadata that does not belong in the portable document model.
//! Cwd/readiness use the recorded OSC 777 document protocol. The effective shell
//! PATH is host-only completion context and arrives on OSC 778.
const PREFIX: &[u8] = b"\x1b]778;kea;path;";
const END: u8 = 0x07;
const MAX_MARKER: usize = 128 * 1024;
const MAX_PATH_BYTES: usize = 64 * 1024;

#[derive(Debug, Default)]
pub struct ShellMetadata {
    pending: Vec<u8>,
    path: Option<String>,
}

impl ShellMetadata {
    pub fn path(&self) -> Option<&str> {
        self.path.as_deref()
    }

    /// Consume arbitrary PTY chunking. Returns true if the effective PATH changed.
    pub fn ingest(&mut self, bytes: &[u8]) -> bool {
        self.pending.extend_from_slice(bytes);
        let mut changed = false;
        loop {
            let Some(start) = find_bytes(&self.pending, PREFIX) else {
                let keep = suffix_prefix_len(&self.pending, PREFIX);
                let drop = self.pending.len().saturating_sub(keep);
                if drop != 0 {
                    self.pending.drain(..drop);
                }
                break;
            };
            if start != 0 {
                self.pending.drain(..start);
            }
            let Some(end) = self.pending.iter().position(|byte| *byte == END) else {
                if self.pending.len() > MAX_MARKER {
                    self.pending.remove(0);
                    continue;
                }
                break;
            };
            let raw: Vec<u8> = self.pending.drain(..=end).collect();
            if let Some(path) = parse(&raw) {
                if self.path.as_deref() != Some(&path) {
                    self.path = Some(path);
                    changed = true;
                }
            }
        }
        changed
    }
}

fn parse(raw: &[u8]) -> Option<String> {
    if !raw.starts_with(PREFIX) || raw.last().copied() != Some(END) {
        return None;
    }
    let encoded = std::str::from_utf8(&raw[PREFIX.len()..raw.len() - 1]).ok()?;
    if encoded.len() > MAX_MARKER || encoded.contains(';') {
        return None;
    }
    let bytes = base64_decode(encoded)?;
    if bytes.len() > MAX_PATH_BYTES {
        return None;
    }
    let path = String::from_utf8(bytes).ok()?;
    (!path.contains('\0')).then_some(path)
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

fn suffix_prefix_len(bytes: &[u8], prefix: &[u8]) -> usize {
    (1..=bytes.len().min(prefix.len().saturating_sub(1)))
        .rev()
        .find(|length| bytes[bytes.len() - length..] == prefix[..*length])
        .unwrap_or(0)
}

fn base64_decode(input: &str) -> Option<Vec<u8>> {
    if !input.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    for (index, chunk) in input.as_bytes().chunks_exact(4).enumerate() {
        let last = index + 1 == input.len() / 4;
        let a = value(chunk[0])?;
        let b = value(chunk[1])?;
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
                let c = value(c)?;
                if c & 0x03 != 0 {
                    return None;
                }
                out.push((a << 2) | (b >> 4));
                out.push((b << 4) | (c >> 2));
            }
            (c, d) => {
                let c = value(c)?;
                let d = value(d)?;
                out.push((a << 2) | (b >> 4));
                out.push((b << 4) | (c >> 2));
                out.push((c << 6) | d);
            }
        }
    }
    Some(out)
}

fn value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_marker_is_chunk_safe() {
        let marker = b"\x1b]778;kea;path;L2Jpbg==\x07"; // /bin
        for split in 0..=marker.len() {
            let mut metadata = ShellMetadata::default();
            metadata.ingest(&marker[..split]);
            metadata.ingest(&marker[split..]);
            assert_eq!(metadata.path(), Some("/bin"));
        }
    }

    #[test]
    fn malformed_or_oversized_markers_do_not_replace_state() {
        let mut metadata = ShellMetadata::default();
        metadata.ingest(b"\x1b]778;kea;path;L2Jpbg==\x07");
        metadata.ingest(b"\x1b]778;kea;path;AA==\x07");
        assert_eq!(metadata.path(), Some("/bin"));
        metadata.ingest(b"\x1b]778;kea;path;not base64\x07");
        assert_eq!(metadata.path(), Some("/bin"));
    }
}
