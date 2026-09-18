use super::*;
use crate::reverse_search::InputKind;
use std::fs;

struct Temporary(PathBuf);

impl Temporary {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "kea-worker-{}",
            Entry::submitted("test".into(), InputKind::Posix, None).id
        )))
    }
}

impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn private_submissions_are_not_recorded_and_search_does_not_write() {
    let mut state = State {
        store: None,
        library: Library::default(),
        ephemeral: HashSet::new(),
        persist_history: false,
    };
    state.handle(Request::Record(Entry::submitted(
        " secret".into(),
        InputKind::Posix,
        None,
    )));
    state.handle(Request::Record(Entry::submitted(
        "echo public".into(),
        InputKind::Posix,
        None,
    )));
    assert_eq!(state.library.entries().len(), 1);
    assert!(state.ephemeral.contains(&state.library.entries()[0].id));
    match state.handle(Request::Search {
        generation: 7,
        query: "public".into(),
        directory: None,
    }) {
        Some(Reply::Results {
            generation,
            hits,
            error,
        }) => {
            assert_eq!(generation, 7);
            assert_eq!(hits.len(), 1);
            assert!(error.is_none());
        }
        _ => panic!("Expected search result"),
    }
}

#[test]
fn persistence_failure_keeps_submitted_input_searchable() {
    let mut state = State {
        store: None,
        library: Library::default(),
        ephemeral: HashSet::new(),
        persist_history: true,
    };
    let reply = state.handle(Request::Record(Entry::submitted(
        "echo public".into(),
        InputKind::Posix,
        None,
    )));
    assert!(matches!(reply, Some(Reply::Warning(_))));
    assert_eq!(state.library.entries().len(), 1);
}

#[test]
fn deleting_grouped_ephemeral_records_keeps_search_and_refresh_empty() {
    let mut state = State {
        store: None,
        library: Library::default(),
        ephemeral: HashSet::new(),
        persist_history: false,
    };
    let first = Entry::submitted("echo grouped".into(), InputKind::Posix, None);
    let second = Entry::submitted("echo grouped".into(), InputKind::Posix, None);
    state.handle(Request::Record(first.clone()));
    state.handle(Request::Record(second.clone()));

    let hit = match state.handle(Request::Search {
        generation: 1,
        query: "grouped".into(),
        directory: None,
    }) {
        Some(Reply::Results { hits, .. }) => {
            assert_eq!(hits.len(), 1);
            hits.into_iter().next().unwrap()
        }
        _ => panic!("Expected grouped search result"),
    };
    assert_eq!(
        hit.occurrence_ids,
        vec![first.id.clone(), second.id.clone()]
    );

    assert!(matches!(
        state.handle(Request::Delete(hit.occurrence_ids)),
        Some(Reply::Deleted)
    ));
    assert!(state.library.entries().is_empty());
    assert!(state.ephemeral.is_empty());
    assert!(matches!(state.handle(Request::Refresh), None));
    match state.handle(Request::Search {
        generation: 2,
        query: "grouped".into(),
        directory: None,
    }) {
        Some(Reply::Results { hits, .. }) => assert!(hits.is_empty()),
        _ => panic!("Expected refreshed search result"),
    }
}

#[test]
fn deleting_persisted_named_memory_survives_reload() {
    let root = Temporary::new();
    let mut state = State {
        store: Some(Store::new(root.0.clone())),
        library: Library::default(),
        ephemeral: HashSet::new(),
        persist_history: false,
    };
    let mut entry = Entry::submitted("echo named".into(), InputKind::Posix, None);
    entry.name = Some("named memory".into());
    state.handle(Request::Save(entry.clone()));
    assert!(root.0.join(format!("{}.kmem", entry.id)).is_file());

    assert!(matches!(
        state.handle(Request::Delete(vec![entry.id.clone()])),
        Some(Reply::Deleted)
    ));
    assert!(!root.0.join(format!("{}.kmem", entry.id)).exists());

    let mut reloaded = State {
        store: Some(Store::new(root.0.clone())),
        library: Library::default(),
        ephemeral: HashSet::new(),
        persist_history: false,
    };
    assert!(reloaded.handle(Request::Refresh).is_none());
    assert!(reloaded.library.entries().is_empty());
}

#[test]
fn deleting_named_memory_preserves_same_text_historical_occurrences() {
    let root = Temporary::new();
    let mut state = State {
        store: Some(Store::new(root.0.clone())),
        library: Library::default(),
        ephemeral: HashSet::new(),
        persist_history: false,
    };
    let history = Entry::submitted("echo same text".into(), InputKind::Posix, None);
    let mut named = Entry::submitted("echo same text".into(), InputKind::Posix, None);
    named.name = Some("saved command".into());
    state.handle(Request::Record(history.clone()));
    assert!(matches!(
        state.handle(Request::Save(named.clone())),
        Some(Reply::Saved)
    ));

    assert!(matches!(
        state.handle(Request::Delete(vec![named.id.clone()])),
        Some(Reply::Deleted)
    ));
    match state.handle(Request::Search {
        generation: 1,
        query: "same text".into(),
        directory: None,
    }) {
        Some(Reply::Results { hits, .. }) => {
            assert_eq!(hits.len(), 1);
            assert_eq!(hits[0].entry.id, history.id);
            assert!(hits[0].entry.name.is_none());
            assert_eq!(hits[0].occurrence_ids, vec![history.id]);
        }
        _ => panic!("Expected historical occurrence"),
    }
}

#[test]
fn partial_delete_reports_progress_and_keeps_failed_target() {
    let root = Temporary::new();
    let store = Store::new(root.0.clone());
    let mut state = State {
        store: Some(store),
        library: Library::default(),
        ephemeral: HashSet::new(),
        persist_history: false,
    };
    let mut first = Entry::submitted("echo first".into(), InputKind::Posix, None);
    first.name = Some("first".into());
    let mut second = Entry::submitted("echo second".into(), InputKind::Posix, None);
    second.name = Some("second".into());
    assert!(matches!(
        state.handle(Request::Save(first.clone())),
        Some(Reply::Saved)
    ));
    assert!(matches!(
        state.handle(Request::Save(second.clone())),
        Some(Reply::Saved)
    ));

    let first_path = root.0.join(format!("{}.kmem", first.id));
    let second_path = root.0.join(format!("{}.kmem", second.id));
    fs::remove_file(&second_path).unwrap();
    fs::create_dir(&second_path).unwrap();
    let storage_error = Store::new(root.0.clone()).remove(&second.id).unwrap_err();

    let reply = state.handle(Request::Delete(vec![first.id.clone(), second.id.clone()]));
    assert!(matches!(
        reply,
        Some(Reply::DeleteFailed(message))
            if message == format!("Forgot 1 of 2 records. {storage_error}")
    ));
    assert!(!first_path.exists());
    assert!(second_path.is_dir());
    assert!(state
        .library
        .entries()
        .iter()
        .any(|entry| entry.id == second.id));
    assert!(!state
        .library
        .entries()
        .iter()
        .any(|entry| entry.id == first.id));
}

#[test]
fn worker_searches_with_generation_and_disconnects_cleanly() {
    let worker = Worker::start(None, false).unwrap();
    worker
        .send(Request::Record(Entry::submitted(
            "echo one".into(),
            InputKind::Posix,
            None,
        )))
        .unwrap();
    worker
        .send(Request::Search {
            generation: 9,
            query: "one".into(),
            directory: None,
        })
        .unwrap();
    match worker
        .replies
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap()
    {
        Reply::Results {
            generation, hits, ..
        } => {
            assert_eq!(generation, 9);
            assert_eq!(hits.len(), 1);
        }
        _ => panic!("Expected search result"),
    }
}
