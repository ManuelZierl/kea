use super::*;

fn entry(text: &str, directory: Option<&str>) -> Entry {
    Entry::submitted(text.into(), InputKind::Posix, directory.map(str::to_string))
}

#[test]
fn fuzzy_words_unicode_and_multiline_are_supported() {
    let mut library = Library::default();
    library
        .insert(entry(
            "docker compose logs worker\nprintf 'Überprüfung 😀'",
            None,
        ))
        .unwrap();
    assert_eq!(library.search("dcl work", None, now_ms()).unwrap().len(), 1);
    assert_eq!(library.search("über 😀", None, now_ms()).unwrap().len(), 1);
    assert!(library
        .search("not-present", None, now_ms())
        .unwrap()
        .is_empty());
}

#[test]
fn groups_occurrences_but_never_conflates_shells_or_saved_memories() {
    let mut library = Library::default();
    library.insert(entry("echo hello", Some("/a"))).unwrap();
    library.insert(entry("echo hello", Some("/b"))).unwrap();
    let mut app = entry("echo hello", None);
    app.kind = InputKind::Application;
    library.insert(app).unwrap();
    let mut saved = entry("echo hello", None);
    saved.name = Some("greeting".into());
    library.insert(saved).unwrap();
    let hits = library.search("hello", Some("/a"), now_ms()).unwrap();
    assert_eq!(hits.len(), 3);
    assert_eq!(
        hits.iter()
            .map(|hit| hit.occurrence_ids.len())
            .sum::<usize>(),
        3
    );
    assert!(hits
        .iter()
        .any(|hit| hit.local && hit.occurrence_ids.len() == 2));
}

#[test]
fn scope_is_exact_and_unknown_is_not_local() {
    let mut library = Library::default();
    let mut saved = entry("pwd", None);
    saved.name = Some("where".into());
    saved.scope = Some("/work/a".into());
    library.insert(saved).unwrap();
    assert_eq!(
        library.search("", Some("/work/a"), now_ms()).unwrap().len(),
        1
    );
    for directory in [None, Some("/work/ab"), Some("/work/a/child")] {
        assert!(library.search("", directory, now_ms()).unwrap().is_empty());
    }
    assert!(!same_directory(None, None));
}

#[test]
fn ranking_boosts_local_saved_and_frequent_matches() {
    let mut library = Library::default();
    library
        .insert(entry("docker logs remote", Some("/other")))
        .unwrap();
    library
        .insert(entry("docker logs local", Some("/here")))
        .unwrap();
    let mut saved = entry("docker logs saved", None);
    saved.name = Some("logs".into());
    library.insert(saved).unwrap();
    let hits = library.search("docker", Some("/here"), now_ms()).unwrap();
    assert_eq!(hits[0].entry.text, "docker logs saved");
    assert_eq!(hits[1].entry.text, "docker logs local");
}

#[test]
fn invalid_and_oversized_data_are_rejected_without_mutation() {
    let mut library = Library::default();
    assert!(library
        .insert(entry(&"x".repeat(MAX_TEXT_BYTES + 1), None))
        .is_err());
    let mut bad = entry("pwd", None);
    bad.id = "../../secret".into();
    assert!(library.insert(bad).is_err());
    assert!(library.entries().is_empty());
    assert!(library.search(&"x".repeat(513), None, 0).is_err());
}

#[test]
fn forgetting_does_not_reappear_in_search_and_limits_are_bounded() {
    let mut library = Library::default();
    for n in 0..100 {
        library
            .insert(entry(&format!("command-{n}"), None))
            .unwrap();
    }
    let hits = library.search("", None, now_ms()).unwrap();
    assert_eq!(hits.len(), MAX_RESULTS);
    let id = hits[0].entry.id.clone();
    library.remove(&id);
    assert!(library.entries().iter().all(|entry| entry.id != id));
}

#[test]
fn display_summary_does_not_change_stored_text() {
    assert_eq!(summary("a\n😀b", 3), "a↵😀…");
}
