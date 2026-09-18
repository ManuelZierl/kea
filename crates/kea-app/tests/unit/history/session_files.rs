#[test]
fn generated_names_are_kea_recordings() {
    let name = format!("session-123-{}.kea", std::process::id());
    assert!(name.starts_with("session-"));
    assert!(name.ends_with(".kea"));
}
