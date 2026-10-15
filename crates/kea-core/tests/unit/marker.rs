use super::*;

fn token(byte: u8) -> MarkerToken {
    MarkerToken::from_bytes([byte; 16])
}

#[test]
fn tokens_are_distinct_from_marker_kinds_and_never_debug_printed() {
    let token = token(0xab);
    assert_eq!(token.as_str().len(), 33);
    assert!(token.as_str().starts_with("kabab"));
    assert_eq!(MarkerToken::parse(token.as_str()), Some(token.clone()));
    for kind in [
        "prompt",
        "cwd",
        "input",
        "path",
        "native-start",
        "kXYZ",
        "k12",
    ] {
        assert!(MarkerToken::parse(kind).is_none(), "{kind}");
    }
    assert!(!format!("{token:?}").contains("abab"));
}

#[test]
fn live_sessions_require_their_token() {
    let session = token(1);
    let remote = token(2);
    let live = MarkerAuth::Live {
        session: session.clone(),
        remote: Some(remote.clone()),
    };
    let body = |token: &MarkerToken| format!("{};prompt;L2V0Yw==", token.as_str());
    assert_eq!(
        live.check(&body(&session)),
        Some((MarkerOrigin::Session, "prompt;L2V0Yw=="))
    );
    assert_eq!(
        live.check(&body(&remote)),
        Some((MarkerOrigin::Remote, "prompt;L2V0Yw=="))
    );
    // Forged output: no token, a wrong token, or a token-shaped prefix only.
    assert_eq!(live.check("prompt;L2V0Yw=="), None);
    assert_eq!(live.check(&body(&token(3))), None);
    assert_eq!(
        live.check(session.as_str()),
        Some((MarkerOrigin::Session, ""))
    );
    let no_remote = MarkerAuth::Live {
        session,
        remote: None,
    };
    assert_eq!(no_remote.check(&body(&remote)), None);
}

#[test]
fn replay_accepts_old_and_new_recordings() {
    let replay = MarkerAuth::Replay;
    assert_eq!(
        replay.check("prompt;L2V0Yw=="),
        Some((MarkerOrigin::Replay, "prompt;L2V0Yw=="))
    );
    let tokened = format!("{};native-done;local;0", token(9).as_str());
    assert_eq!(
        replay.check(&tokened),
        Some((MarkerOrigin::Replay, "native-done;local;0"))
    );
}
