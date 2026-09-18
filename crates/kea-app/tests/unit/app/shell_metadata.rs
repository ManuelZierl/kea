use super::*;

#[test]
fn path_marker_is_chunk_safe() {
    let marker = b"\x1b]778;kea;path;L2Jpbg==\x07";
    for split in 0..=marker.len() {
        let mut metadata = ShellMetadata::default();
        assert!(!metadata.ingest(&marker[..split]));
        assert!(!metadata.ingest(&marker[split..]));
        assert_eq!(metadata.path(), Some("/bin"));
    }
}

#[test]
fn malformed_or_oversized_markers_do_not_replace_state() {
    let mut metadata = ShellMetadata::default();
    metadata.ingest(b"\x1b]778;kea;path;L2Jpbg==\x07");
    metadata.ingest(b"\x1b]778;kea;path;AA==\x07");
    assert_eq!(metadata.path(), Some("/bin"));
    metadata.ingest(b"\x1b]778;kea;path;not base64\x07");
    assert_eq!(metadata.path(), Some("/bin"));
}
