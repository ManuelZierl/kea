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
fn sgr_pointer_preserves_button_release_and_motion() {
    let point = TerminalPoint { row: 2, column: 4 };
    assert_eq!(
        encode_pointer(
            MouseEncoding::Sgr,
            PointerEvent::Press,
            Some(PointerButton::Left),
            point,
            false,
            false,
            false,
        )
        .unwrap(),
        b"\x1b[<0;5;3M"
    );
    assert_eq!(
        encode_pointer(
            MouseEncoding::Sgr,
            PointerEvent::Motion,
            Some(PointerButton::Left),
            point,
            false,
            false,
            false,
        )
        .unwrap(),
        b"\x1b[<32;5;3M"
    );
    assert_eq!(
        encode_pointer(
            MouseEncoding::Sgr,
            PointerEvent::Release,
            Some(PointerButton::Left),
            point,
            false,
            false,
            false,
        )
        .unwrap(),
        b"\x1b[<0;5;3m"
    );
}

#[test]
fn classic_release_uses_button_three_and_modifiers() {
    let point = TerminalPoint { row: 0, column: 0 };
    assert_eq!(
        encode_pointer(
            MouseEncoding::Legacy,
            PointerEvent::Release,
            Some(PointerButton::Right),
            point,
            false,
            true,
            true,
        )
        .unwrap(),
        vec![0x1b, b'[', b'M', 59, 33, 33]
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
