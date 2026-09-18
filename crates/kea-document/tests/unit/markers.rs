use crate::encoding::encode_input;
use crate::{CommandStatus, Document};

fn marker_start(id: u64, input: &str) -> Vec<u8> {
    format!(
        "\x1b]777;kea;start;{id};{}\x07",
        encode_input(input).unwrap()
    )
    .into_bytes()
}

fn marker_done(id: u64, code: i32) -> Vec<u8> {
    format!("\x1b]777;kea;done;{id};{code}\x07").into_bytes()
}

#[test]
fn marker_scanner_handles_arbitrary_chunk_boundaries() {
    let mut document = Document::new();
    document.queue_local(7, "echo split".into(), 1).unwrap();
    let start = marker_start(7, "echo split");
    let split = start.len() / 2;
    let mut first = b"echo wrapper".to_vec();
    first.extend_from_slice(&start[..split]);
    assert!(!document.ingest_output(2, &first));
    let mut second = start[split..].to_vec();
    second.extend_from_slice(b"hel");
    assert!(document.ingest_output(3, &second));
    let mut third = b"lo".to_vec();
    third.extend(marker_done(7, 3));
    third.extend_from_slice(b"prompt");
    assert!(document.ingest_output(4, &third));
    let block = &document.blocks()[0];
    assert_eq!(block.plain_output(), "hello");
    assert_eq!(block.status, CommandStatus::Finished(3));
    assert_eq!(block.started_at, Some(3));
    assert_eq!(block.finished_at, Some(4));
}
