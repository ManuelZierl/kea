use super::*;

fn recording() -> Recording {
    Recording::new(Size::new(80, 24).unwrap()).unwrap()
}

#[test]
fn round_trip_preserves_non_utf8_and_equal_timestamp_order() {
    let mut r = recording();
    r.append(3, Kind::Output(vec![0xff, 0, 27])).unwrap();
    r.append(3, Kind::Resize(Size::new(40, 12).unwrap()))
        .unwrap();
    r.append(4, Kind::Exit(7)).unwrap();
    let mut bytes = Vec::new();
    r.write_to(&mut bytes).unwrap();
    let loaded = read_from(bytes.as_slice()).unwrap();
    assert_eq!(loaded.recording.events(), r.events());
    assert_eq!(loaded.recording.initial_size(), r.initial_size());
    assert_eq!(loaded.recording.end_at(3), 2);
    assert!(!loaded.truncated_tail);
}

#[test]
fn truncated_tail_recovers_only_complete_events() {
    let mut r = recording();
    r.append(1, Kind::Output(b"visible".to_vec())).unwrap();
    let mut prefix = Vec::new();
    r.write_to(&mut prefix).unwrap();
    r.append(2, Kind::Output(b"tail".to_vec())).unwrap();
    let mut bytes = Vec::new();
    r.write_to(&mut bytes).unwrap();
    for len in prefix.len() + 1..bytes.len() {
        let loaded = read_from(&bytes[..len]).unwrap();
        assert!(loaded.truncated_tail);
        assert_eq!(loaded.recording.events().len(), 1);
    }
}

#[test]
fn corruption_is_not_treated_as_truncation() {
    let mut r = recording();
    r.append(1, Kind::Output(b"hello".to_vec())).unwrap();
    let mut bytes = Vec::new();
    r.write_to(&mut bytes).unwrap();
    bytes[25] ^= 1;
    assert!(read_from(bytes.as_slice()).is_err());
}

#[test]
fn rejects_large_frames_before_allocation() {
    let mut bytes = Vec::new();
    write_header(&mut bytes, Size::new(80, 24).unwrap()).unwrap();
    bytes.extend_from_slice(&u32::MAX.to_le_bytes());
    assert!(read_from(bytes.as_slice()).is_err());
}

#[test]
fn checksum_matches_standard_vector() {
    assert_eq!(checksum(b"123456789"), 0xcbf43926);
}
