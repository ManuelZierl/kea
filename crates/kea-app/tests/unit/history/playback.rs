use super::*;

#[test]
fn timeline_fraction_uses_the_measured_track_and_clamps() {
    assert_eq!(fraction_at_x(25.0, 25.0, 200.0), Some(0.0));
    assert_eq!(fraction_at_x(125.0, 25.0, 200.0), Some(0.5));
    assert_eq!(fraction_at_x(225.0, 25.0, 200.0), Some(1.0));
    assert_eq!(fraction_at_x(-50.0, 25.0, 200.0), Some(0.0));
    assert_eq!(fraction_at_x(500.0, 25.0, 200.0), Some(1.0));
    assert_eq!(fraction_at_x(25.0, 25.0, 0.0), None);
}

#[test]
fn timeline_fraction_maps_to_recording_time() {
    assert_eq!(micros_at_fraction(0.5, 8_000_000), 4_000_000);
    assert_eq!(micros_at_fraction(-1.0, 8_000_000), 0);
    assert_eq!(micros_at_fraction(2.0, 8_000_000), 8_000_000);
}

#[test]
fn playback_time_is_compact_and_stable() {
    assert_eq!(format_micros(0), "00:00.000");
    assert_eq!(format_micros(61_234_000), "01:01.234");
    assert_eq!(format_micros(3_661_234_000), "1:01:01.234");
}
