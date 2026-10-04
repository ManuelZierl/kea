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

#[test]
fn retained_append_evicts_event_batches_and_preserves_suffix() {
    let mut r = recording();
    for at in 0..MAX_EVENTS as u64 {
        r.append(at, Kind::Output(vec![b'x'])).unwrap();
    }
    let evicted = r
        .append_retained(MAX_EVENTS as u64, Kind::Output(vec![b'y']))
        .unwrap();
    assert!(evicted >= MAX_EVENTS / 4);
    assert_eq!(r.events().len(), MAX_EVENTS - evicted + 1);
    assert_eq!(r.events().last().unwrap().kind, Kind::Output(vec![b'y']));
    assert_eq!(r.discarded_events(), evicted as u64);
    assert_eq!(r.start_time(), (evicted - 1) as u64);
}

#[test]
fn invalid_retained_append_does_not_evict_or_mutate_metadata() {
    let mut r = recording();
    for at in 0..MAX_EVENTS as u64 {
        r.append(at, Kind::Output(vec![b'x'])).unwrap();
    }
    let prior = r.events().to_vec();
    assert!(r.append_retained(0, Kind::Output(vec![])).is_err());
    assert_eq!(r.events(), prior);
    assert_eq!(r.discarded_events(), 0);
    assert_eq!(r.start_time(), 0);
}

#[test]
fn byte_limit_retains_latest_chunk_and_evicts_in_batches() {
    let mut r = recording();
    let chunk = vec![b'x'; MAX_OUTPUT];
    for at in 0..MAX_BYTES / (MAX_OUTPUT + 64) {
        r.append(at as u64, Kind::Output(chunk.clone())).unwrap();
    }
    let evicted = r.append_retained(100, Kind::Output(chunk.clone())).unwrap();
    assert!(evicted > 1);
    assert_eq!(r.events().last().unwrap().kind, Kind::Output(chunk));
    assert!(r.bytes() <= MAX_BYTES);
}
