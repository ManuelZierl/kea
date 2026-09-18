use super::*;

#[test]
fn command_input_encoding_round_trips_unicode_and_newlines() {
    let input = "printf 'ä'\nsecond line";
    let encoded = encode_input(input).unwrap();
    assert_eq!(
        String::from_utf8(base64_decode(&encoded).unwrap()).unwrap(),
        input
    );
}

#[test]
fn malformed_base64_markers_are_rejected() {
    for malformed in ["=AAA", "AA=A", "AA==AAAA", "AB==", "AAB="] {
        assert!(base64_decode(malformed).is_none(), "accepted {malformed}");
    }
}
