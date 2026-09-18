use super::*;
use kea_core::Kind;
use std::{
    fs::File,
    time::{SystemTime, UNIX_EPOCH},
};

fn temp_path(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("kea-{name}-{}-{unique}.kea", std::process::id()))
}

#[test]
fn seeded_journal_contains_history_that_predates_saving() {
    let path = temp_path("seeded");
    let mut recording = Recording::new(Size::new(80, 24).unwrap()).unwrap();
    recording
        .append(5, Kind::Output(b"before save\r\n".to_vec()))
        .unwrap();
    {
        let mut journal = Journal::create_from_recording(&path, &recording).unwrap();
        journal
            .append(&Event {
                at: 10,
                kind: Kind::Output(b"after save\r\n".to_vec()),
            })
            .unwrap();
    }
    let loaded = kea_core::read_from(File::open(&path).unwrap()).unwrap();
    assert_eq!(loaded.recording.events().len(), 2);
    assert_eq!(loaded.recording.events()[0].at, 5);
    assert_eq!(loaded.recording.events()[1].at, 10);
    let _ = std::fs::remove_file(path);
}

#[test]
fn journal_is_exclusive_and_round_trips_after_close() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("kea-test-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("session.kea");
    let size = Size::new(80, 24).unwrap();
    {
        let mut journal = Journal::create(&path, size).unwrap();
        assert!(Journal::create(&path, size).is_err());
        journal
            .append(&kea_core::Event {
                at: 0,
                kind: Kind::Output(b"ok".to_vec()),
            })
            .unwrap();
    }
    let loaded = kea_core::read_from(std::fs::File::open(&path).unwrap()).unwrap();
    assert_eq!(loaded.recording.events().len(), 1);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(directory).unwrap();
}
