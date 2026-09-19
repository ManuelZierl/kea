use super::*;

#[test]
fn hunt_path_and_prompt_in_same_chunk_reach_both_parsers() {
    let bytes = b"\x1b]778;kea;path;L2Jpbg==\x07\x1b]777;kea;prompt;L3RtcA==\x07";
    for split in 0..=bytes.len() {
        let mut metadata = ShellMetadata::default();
        let mut document = kea_document::Document::new();
        for chunk in [&bytes[..split], &bytes[split..]] {
            // Same short-circuit composition as KeaView::pump_session.
            let _changed = metadata.ingest(chunk) || document.ingest_output(1, chunk);
        }
        assert_eq!(metadata.path(), Some("/bin"), "split {split}");
        assert_eq!(document.directory(), Some("/tmp"), "split {split}");
        assert!(document.prompt_ready(), "split {split}");
    }
}

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

#[test]
fn hunt_decoded_path_limit_accepts_boundary_and_rejects_overflow() {
    let mut metadata = ShellMetadata::default();
    let at_limit = format!(
        "\x1b]778;kea;path;{}YQ==\x07",
        "YWFh".repeat(MAX_PATH_BYTES / 3)
    );
    metadata.ingest(at_limit.as_bytes());
    assert_eq!(metadata.path(), Some("a".repeat(MAX_PATH_BYTES).as_str()));
    let oversized = format!(
        "\x1b]778;kea;path;{}\x07",
        "YWFh".repeat(MAX_PATH_BYTES / 3 + 1)
    );
    metadata.ingest(oversized.as_bytes());
    assert_eq!(metadata.path(), Some("a".repeat(MAX_PATH_BYTES).as_str()));
    metadata.ingest(b"\x1b]778;kea;path;L2Jpbg==\x07");
    assert_eq!(metadata.path(), Some("/bin"));
}

#[test]
fn hunt_unterminated_oversized_marker_recovers_at_next_marker() {
    let mut metadata = ShellMetadata::default();
    metadata.ingest(b"\x1b]778;kea;path;L2Jpbg==\x07");
    let oversized = format!("\x1b]778;kea;path;{}", "A".repeat(MAX_MARKER + 1));
    metadata.ingest(oversized.as_bytes());
    assert_eq!(metadata.path(), Some("/bin"));
    metadata.ingest(b"\x1b]778;kea;path;L3RtcA==\x07");
    assert_eq!(metadata.path(), Some("/tmp"));
}
