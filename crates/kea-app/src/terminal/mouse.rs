use kea_alacritty::{MouseEncoding, TerminalPoint};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WheelDirection {
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerButton {
    Left,
    Middle,
    Right,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerEvent {
    Press,
    Release,
    Motion,
}

pub fn accumulate_wheel_delta(delta: f32, remainder: &mut f32, maximum: i32) -> i32 {
    if !delta.is_finite() || !remainder.is_finite() {
        *remainder = 0.0;
        return 0;
    }
    let maximum = maximum.max(0) as f32;
    let total = (*remainder + delta).clamp(-maximum, maximum);
    let whole = total.trunc();
    let emitted = whole as i32;
    *remainder = total - emitted as f32;
    emitted
}

pub fn encode_wheel(
    encoding: MouseEncoding,
    direction: WheelDirection,
    point: TerminalPoint,
    alt: bool,
    control: bool,
) -> Result<Vec<u8>, &'static str> {
    let mut button = match direction {
        WheelDirection::Up => 64,
        WheelDirection::Down => 65,
    };
    if alt {
        button += 8;
    }
    if control {
        button += 16;
    }
    encode(encoding, button, point, false)
}

pub fn encode_pointer(
    encoding: MouseEncoding,
    event: PointerEvent,
    button: Option<PointerButton>,
    point: TerminalPoint,
    shift: bool,
    alt: bool,
    control: bool,
) -> Result<Vec<u8>, &'static str> {
    let base = match button {
        Some(PointerButton::Left) => 0,
        Some(PointerButton::Middle) => 1,
        Some(PointerButton::Right) => 2,
        None => 3,
    };
    let mut code = match event {
        PointerEvent::Press => base,
        PointerEvent::Release if encoding == MouseEncoding::Sgr => base,
        PointerEvent::Release => 3,
        PointerEvent::Motion => base + 32,
    };
    if shift {
        code += 4;
    }
    if alt {
        code += 8;
    }
    if control {
        code += 16;
    }
    encode(
        encoding,
        code,
        point,
        event == PointerEvent::Release && encoding == MouseEncoding::Sgr,
    )
}

fn encode(
    encoding: MouseEncoding,
    button: usize,
    point: TerminalPoint,
    sgr_release: bool,
) -> Result<Vec<u8>, &'static str> {
    let column = point.column.checked_add(1).ok_or("mouse column overflow")?;
    let row = point.row.checked_add(1).ok_or("mouse row overflow")?;

    match encoding {
        MouseEncoding::Sgr => Ok(format!(
            "\x1b[<{button};{column};{row}{}",
            if sgr_release { 'm' } else { 'M' }
        )
        .into_bytes()),
        MouseEncoding::Legacy => {
            let button = legacy_byte(button)?;
            let column = legacy_byte(column)?;
            let row = legacy_byte(row)?;
            Ok(vec![0x1b, b'[', b'M', button, column, row])
        }
        MouseEncoding::Utf8 => {
            let mut bytes = b"\x1b[M".to_vec();
            push_utf8_field(&mut bytes, button)?;
            push_utf8_field(&mut bytes, column)?;
            push_utf8_field(&mut bytes, row)?;
            Ok(bytes)
        }
    }
}

fn legacy_byte(value: usize) -> Result<u8, &'static str> {
    value
        .checked_add(32)
        .and_then(|value| u8::try_from(value).ok())
        .ok_or("mouse position is outside the legacy protocol range")
}

fn push_utf8_field(bytes: &mut Vec<u8>, value: usize) -> Result<(), &'static str> {
    let value = value
        .checked_add(32)
        .and_then(|value| u32::try_from(value).ok())
        .and_then(char::from_u32)
        .ok_or("mouse position is outside the UTF-8 protocol range")?;
    let mut encoded = [0; 4];
    bytes.extend_from_slice(value.encode_utf8(&mut encoded).as_bytes());
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/terminal/mouse.rs"]
mod tests;
