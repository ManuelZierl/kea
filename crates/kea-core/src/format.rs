use crate::types::invalid;
use crate::{Event, Kind, Recording, Size, MAX_OUTPUT};
use std::io::{self, Read, Write};

const MAGIC: &[u8; 8] = b"KEA\x02\r\n\x1a\n";

pub struct Loaded {
    pub recording: Recording,
    /// Only an incomplete final frame is recoverable; corruption is an error.
    pub truncated_tail: bool,
}

pub fn write_header(mut out: impl Write, size: Size) -> io::Result<()> {
    Size::new(size.columns, size.rows)?;
    out.write_all(MAGIC)?;
    out.write_all(&size.columns.to_le_bytes())?;
    out.write_all(&size.rows.to_le_bytes())
}

pub fn write_event(mut out: impl Write, event: &Event) -> io::Result<()> {
    let mut body = Vec::new();
    body.extend_from_slice(&event.at.to_le_bytes());
    match &event.kind {
        Kind::Output(bytes) => {
            if bytes.is_empty() || bytes.len() > MAX_OUTPUT {
                return Err(invalid("invalid output length"));
            }
            body.push(0);
            body.extend_from_slice(bytes);
        }
        Kind::Resize(size) => {
            Size::new(size.columns, size.rows)?;
            body.push(1);
            body.extend_from_slice(&size.columns.to_le_bytes());
            body.extend_from_slice(&size.rows.to_le_bytes());
        }
        Kind::Submitted { id, context, input } => {
            if input.is_empty() || input.len() > 64 * 1024 || input.contains('\0') {
                return Err(invalid("invalid submitted input"));
            }
            if context.is_empty()
                || context.len() > 128
                || !context
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
            {
                return Err(invalid("invalid input context"));
            }
            body.push(3);
            body.extend_from_slice(&id.to_le_bytes());
            body.extend_from_slice(&(context.len() as u16).to_le_bytes());
            body.extend_from_slice(context.as_bytes());
            body.extend_from_slice(input.as_bytes());
        }
        Kind::Exit(code) => {
            body.push(2);
            body.extend_from_slice(&code.to_le_bytes());
        }
    }
    out.write_all(&(body.len() as u32).to_le_bytes())?;
    out.write_all(&body)?;
    out.write_all(&checksum(&body).to_le_bytes())
}

pub fn read_from(mut input: impl Read) -> io::Result<Loaded> {
    let mut header = [0; 12];
    input.read_exact(&mut header)?;
    let version = header[3];
    if &header[..3] != b"KEA" || !matches!(version, 1 | 2) || &header[4..8] != &MAGIC[4..] {
        return Err(invalid("not a supported Kea v1/v2 recording"));
    }
    let initial = Size::new(
        u16::from_le_bytes([header[8], header[9]]),
        u16::from_le_bytes([header[10], header[11]]),
    )?;
    let mut recording = Recording::new(initial)?;
    loop {
        let mut length = [0; 4];
        let count = read_partial(&mut input, &mut length)?;
        if count == 0 {
            return Ok(Loaded {
                recording,
                truncated_tail: false,
            });
        }
        if count < 4 {
            return Ok(Loaded {
                recording,
                truncated_tail: true,
            });
        }
        let length = u32::from_le_bytes(length) as usize;
        if !(10..=MAX_OUTPUT + 9).contains(&length) {
            return Err(invalid("invalid frame length"));
        }
        let mut body = vec![0; length];
        let mut crc = [0; 4];
        if read_partial(&mut input, &mut body)? < length || read_partial(&mut input, &mut crc)? < 4
        {
            return Ok(Loaded {
                recording,
                truncated_tail: true,
            });
        }
        if checksum(&body) != u32::from_le_bytes(crc) {
            return Err(invalid("frame checksum mismatch"));
        }
        let at = u64::from_le_bytes(body[..8].try_into().map_err(|_| invalid("bad timestamp"))?);
        let kind = match body[8] {
            0 => Kind::Output(body[9..].to_vec()),
            1 if length == 13 => Kind::Resize(Size::new(
                u16::from_le_bytes([body[9], body[10]]),
                u16::from_le_bytes([body[11], body[12]]),
            )?),
            2 if length == 13 => Kind::Exit(u32::from_le_bytes(
                body[9..13]
                    .try_into()
                    .map_err(|_| invalid("bad exit code"))?,
            )),
            3 if version >= 2 && (21..=19 + 128 + 64 * 1024).contains(&length) => {
                let context_len = u16::from_le_bytes([body[17], body[18]]) as usize;
                if context_len == 0 || context_len > 128 || 19 + context_len >= length {
                    return Err(invalid("invalid input context length"));
                }
                Kind::Submitted {
                    id: u64::from_le_bytes(
                        body[9..17]
                            .try_into()
                            .map_err(|_| invalid("bad submission id"))?,
                    ),
                    context: String::from_utf8(body[19..19 + context_len].to_vec())
                        .map_err(|_| invalid("invalid context UTF-8"))?,
                    input: String::from_utf8(body[19 + context_len..].to_vec())
                        .map_err(|_| invalid("invalid input UTF-8"))?,
                }
            }
            _ => return Err(invalid("unknown or malformed event")),
        };
        recording.append(at, kind)?;
    }
}

fn read_partial(input: &mut impl Read, buffer: &mut [u8]) -> io::Result<usize> {
    let mut count = 0;
    while count < buffer.len() {
        match input.read(&mut buffer[count..]) {
            Ok(0) => break,
            Ok(n) => count += n,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(count)
}

// IEEE CRC-32. Integrity detection, not authentication or encryption.
fn checksum(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

#[cfg(test)]
#[path = "../tests/unit/format.rs"]
mod tests;
