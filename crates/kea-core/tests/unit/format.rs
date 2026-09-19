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

#[test]
fn v2_submission_round_trips_without_rewriting_output_and_reads_v1() {
    let mut r = recording();
    r.append(
        1,
        Kind::Submitted {
            id: 9,
            context: "remote-1".into(),
            input: "printf 'ä😀'\nprintf ok".into(),
        },
    )
    .unwrap();
    r.append(2, Kind::Output(vec![0xff, 0, 27])).unwrap();
    let mut bytes = Vec::new();
    r.write_to(&mut bytes).unwrap();
    assert_eq!(bytes[3], 2);
    assert_eq!(
        read_from(bytes.as_slice()).unwrap().recording.events(),
        r.events()
    );
    bytes[3] = 1;
    assert!(
        read_from(bytes.as_slice()).is_err(),
        "v1 must not accept unknown v2 frames"
    );
    let mut old = recording();
    old.append(1, Kind::Output(b"legacy".to_vec())).unwrap();
    let mut bytes = Vec::new();
    old.write_to(&mut bytes).unwrap();
    bytes[3] = 1;
    assert_eq!(
        read_from(bytes.as_slice()).unwrap().recording.events(),
        old.events()
    );
}

#[test]
fn submission_metadata_is_bounded_and_never_replayed_as_input_or_output() {
    let mut r = recording();
    for context in ["", "contains;delimiter", "bad\ncontext"] {
        assert!(r
            .append(
                1,
                Kind::Submitted {
                    id: 1,
                    context: context.into(),
                    input: "ls".into()
                }
            )
            .is_err());
    }
    assert!(r
        .append(
            1,
            Kind::Submitted {
                id: 1,
                context: "local".into(),
                input: "x".repeat(65537)
            }
        )
        .is_err());
    assert!(r.events().is_empty());
}
