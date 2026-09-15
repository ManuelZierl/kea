use kea_alacritty::{MouseEncoding, TerminalPoint};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WheelDirection {
    Up,
    Down,
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
    let column = point.column.checked_add(1).ok_or("mouse column overflow")?;
    let row = point.row.checked_add(1).ok_or("mouse row overflow")?;

    match encoding {
        MouseEncoding::Sgr => Ok(format!("\x1b[<{button};{column};{row}M").into_bytes()),
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
mod tests {
    use super::*;

    #[test]
    fn sgr_wheel_uses_one_based_coordinates_and_modifiers() {
        let point = TerminalPoint { row: 4, column: 9 };
        assert_eq!(
            encode_wheel(MouseEncoding::Sgr, WheelDirection::Up, point, false, false).unwrap(),
            b"\x1b[<64;10;5M"
        );
        assert_eq!(
            encode_wheel(MouseEncoding::Sgr, WheelDirection::Down, point, true, true).unwrap(),
            b"\x1b[<89;10;5M"
        );
    }

    #[test]
    fn legacy_wheel_rejects_unrepresentable_coordinates() {
        assert_eq!(
            encode_wheel(
                MouseEncoding::Legacy,
                WheelDirection::Up,
                TerminalPoint { row: 0, column: 0 },
                false,
                false,
            )
            .unwrap(),
            b"\x1b[M`!!"
        );
        assert!(encode_wheel(
            MouseEncoding::Legacy,
            WheelDirection::Up,
            TerminalPoint {
                row: 0,
                column: 223
            },
            false,
            false,
        )
        .is_err());
    }

    #[test]
    fn utf8_wheel_preserves_extended_coordinates() {
        let encoded = encode_wheel(
            MouseEncoding::Utf8,
            WheelDirection::Down,
            TerminalPoint {
                row: 199,
                column: 299,
            },
            false,
            false,
        )
        .unwrap();
        assert_eq!(std::str::from_utf8(&encoded).unwrap(), "\x1b[MaŌè");
    }

    #[test]
    fn fractional_wheel_input_accumulates_and_large_events_are_bounded() {
        let mut remainder = 0.0;
        assert_eq!(accumulate_wheel_delta(0.4, &mut remainder, 4), 0);
        assert_eq!(accumulate_wheel_delta(0.7, &mut remainder, 4), 1);
        assert!((remainder - 0.1).abs() < f32::EPSILON * 4.0);
        assert_eq!(accumulate_wheel_delta(-1.3, &mut remainder, 4), -1);
        assert!((remainder + 0.2).abs() < f32::EPSILON * 4.0);
        assert_eq!(accumulate_wheel_delta(20.0, &mut remainder, 4), 4);
        assert_eq!(remainder, 0.0);
    }
}
