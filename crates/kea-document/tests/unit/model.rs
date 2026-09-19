use super::*;
use crate::encoding::encode_input;
use kea_core::{Kind, Recording, Size};

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
fn reconstructs_real_command_blocks_without_prompt_guessing() {
    let mut recording = Recording::new(Size::new(80, 24).unwrap()).unwrap();
    let mut output = b"wrapper echo and prompt".to_vec();
    output.extend(marker_start(1, "printf hi"));
    output.extend(b"\x1b[32mhi\x1b[0m");
    output.extend(marker_done(1, 0));
    output.extend(b"prompt again");
    recording.append(2, Kind::Output(output)).unwrap();
    let document = Document::from_recording(&recording);
    assert_eq!(document.blocks().len(), 1);
    let block = &document.blocks()[0];
    assert_eq!(block.input, "printf hi");
    assert_eq!(block.status, CommandStatus::Finished(0));
    assert_eq!(block.plain_output(), "hi");
}

#[test]
fn another_command_is_rejected_until_completion() {
    let mut document = Document::new();
    document.queue_local(1, "sleep 1".into(), 0).unwrap();
    assert_eq!(
        document.queue_local(2, "echo nope".into(), 0),
        Err(Error::CommandAlreadyRunning)
    );
    document.ingest_output(1, &marker_start(1, "sleep 1"));
    document.ingest_output(2, &marker_done(1, 0));
    document.queue_local(2, "echo yes".into(), 3).unwrap();
}

#[test]
fn exit_aborts_running_block() {
    let mut document = Document::new();
    document.queue_local(1, "exit".into(), 0).unwrap();
    document.ingest_output(1, &marker_start(1, "exit"));
    assert!(document.abort_in_flight(2));
    assert_eq!(document.blocks()[0].status, CommandStatus::Aborted);
}

#[test]
fn local_queue_applies_command_validation() {
    let mut document = Document::new();
    assert_eq!(
        document.queue_local(1, String::new(), 0),
        Err(Error::EmptyCommand)
    );
    assert_eq!(
        document.queue_local(1, "x".repeat(MAX_COMMAND_BYTES + 1), 0),
        Err(Error::CommandTooLarge)
    );
}

#[test]
fn explicit_directory_reports_are_chunk_independent() {
    let mut d = Document::new();
    let marker = format!(
        "\x1b]777;kea;prompt;{}\x07",
        crate::encoding::base64_encode("/tmp/space ä;dir".as_bytes())
    );
    for byte in marker.as_bytes() {
        d.ingest_output(1, &[*byte]);
    }
    assert_eq!(d.directory(), Some("/tmp/space ä;dir"));
    assert!(d.prompt_ready());
    d.note_terminal_input();
    assert!(!d.prompt_ready());
    d.ingest_output(2, marker.as_bytes());
    d.queue_local(1, "pwd".into(), 3).unwrap();
    assert!(!d.prompt_ready());
    assert_eq!(d.blocks()[0].directory.as_deref(), d.directory());
    d.ingest_output(4, b"\x1b]777;kea;prompt;AA==\x07");
    assert_eq!(d.directory(), Some("/tmp/space ä;dir"));
}

#[test]
fn native_blocks_require_scoped_observations_and_survive_replay() {
    let mut recording = kea_core::Recording::new(kea_core::Size::new(80, 24).unwrap()).unwrap();
    recording
        .append(
            1,
            Kind::Submitted {
                id: 12,
                context: "local".into(),
                input: "ssh destination".into(),
            },
        )
        .unwrap();
    let pending = Document::from_recording(&recording);
    assert!(
        pending.blocks().is_empty(),
        "accepted metadata is not a queued block"
    );
    recording.append(2, Kind::Output(b"\x1b]777;kea;native-start;local\x07outer output\x1b]777;kea;native-done;remote;0\x07\x1b]777;kea;prompt;L3RtcA==\x07".to_vec())).unwrap();
    let running = Document::from_recording(&recording);
    assert_eq!(running.blocks().len(), 1);
    assert_eq!(
        running.blocks()[0].status,
        CommandStatus::Running,
        "a nested shell's prompt/done cannot finish the outer block"
    );
    recording
        .append(
            3,
            Kind::Output(b"\x1b]777;kea;native-done;local;7\x07".to_vec()),
        )
        .unwrap();
    let finished = Document::from_recording(&recording);
    assert_eq!(finished.blocks()[0].status, CommandStatus::Finished(7));
    assert_eq!(finished.blocks()[0].input, "ssh destination");
    assert_eq!(finished.blocks()[0].queued_at, 1);
}

#[test]
fn missing_start_discards_pending_metadata_without_creating_stuck_blocks() {
    let mut doc = Document::new();
    doc.note_submission(1, "local", "command", 1);
    doc.ingest_output(2, b"\x1b]777;kea;prompt;L3RtcA==\x07");
    doc.ingest_output(3, b"\x1b]777;kea;native-start;local\x07");
    assert!(doc.blocks().is_empty());
    assert!(!doc.has_in_flight());
}
