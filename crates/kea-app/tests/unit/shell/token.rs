use super::*;

#[test]
fn session_tokens_are_random_and_well_formed() {
    let a = new_session_token().unwrap();
    let b = new_session_token().unwrap();
    assert_ne!(a, b);
    assert!(MarkerToken::parse(a.as_str()).is_some());
}

#[test]
fn the_remote_token_is_created_once_and_owner_only() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("kea").join(REMOTE_TOKEN_FILE);
    let first = remote_token_at(&path).unwrap();
    assert_eq!(remote_token_at(&path).unwrap(), first);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
    fs::write(&path, "not a token").unwrap();
    assert!(remote_token_at(&path).is_err());
}
