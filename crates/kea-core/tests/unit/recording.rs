use super::*;

fn recording() -> Recording {
    Recording::new(Size::new(80, 24).unwrap()).unwrap()
}

#[test]
fn rejects_bad_dimensions_clock_and_events_after_exit() {
    assert!(Size::new(0, 24).is_err());
    assert!(Size::new(80, u16::MAX).is_err());
    let mut r = recording();
    r.append(2, Kind::Output(vec![1])).unwrap();
    assert!(r.append(1, Kind::Output(vec![1])).is_err());
    r.append(3, Kind::Exit(0)).unwrap();
    assert!(r.append(4, Kind::Output(vec![1])).is_err());
}

#[test]
fn event_count_is_bounded_without_destroying_prefix() {
    let mut r = recording();
    for at in 0..MAX_EVENTS as u64 {
        r.append(at, Kind::Output(vec![b'x'])).unwrap();
    }
    assert!(r
        .append(MAX_EVENTS as u64, Kind::Output(vec![b'x']))
        .is_err());
    assert_eq!(r.events().len(), MAX_EVENTS);
}
