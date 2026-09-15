pub fn fraction_at_x(x: f32, track_left: f32, track_width: f32) -> Option<f32> {
    if !x.is_finite() || !track_left.is_finite() || !track_width.is_finite() || track_width <= 0.0 {
        return None;
    }
    Some(((x - track_left) / track_width).clamp(0.0, 1.0))
}

pub fn micros_at_fraction(fraction: f32, duration: u64) -> u64 {
    (f64::from(fraction.clamp(0.0, 1.0)) * duration as f64) as u64
}

pub fn format_micros(micros: u64) -> String {
    let millis = micros / 1_000;
    let hours = millis / 3_600_000;
    let minutes = (millis / 60_000) % 60;
    let seconds = (millis / 1_000) % 60;
    let millis = millis % 1_000;
    if hours == 0 {
        format!("{minutes:02}:{seconds:02}.{millis:03}")
    } else {
        format!("{hours}:{minutes:02}:{seconds:02}.{millis:03}")
    }
}

#[cfg(test)]
mod tests {
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
}
