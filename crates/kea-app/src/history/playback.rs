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
#[path = "../../tests/unit/history/playback.rs"]
mod tests;
