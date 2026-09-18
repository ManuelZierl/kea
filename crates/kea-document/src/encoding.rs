use crate::model::{Error, MAX_COMMAND_BYTES};

pub fn encode_input(input: &str) -> Result<String, Error> {
    validate_input(input)?;
    Ok(base64_encode(input.as_bytes()))
}

pub(crate) fn validate_input(input: &str) -> Result<(), Error> {
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

pub(crate) fn base64_encode(bytes: &[u8]) -> String {
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

pub(crate) fn base64_decode(input: &str) -> Option<Vec<u8>> {
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

#[cfg(test)]
#[path = "../tests/unit/encoding.rs"]
mod tests;
