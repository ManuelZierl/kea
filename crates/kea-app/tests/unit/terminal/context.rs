use super::*;

#[test]
fn local_remote_and_apps_use_the_same_protocol() {
    for (id, kind, expected) in [
        ("local", "posix", ReceiverKind::Posix),
        ("remote-42", "powershell", ReceiverKind::PowerShell),
        ("application-1", "app", ReceiverKind::Application),
    ] {
        let mut context = InputContext::default();
        let report = format!("\x1b]779;kea;input;1;{id};{kind};ready\x07");
        for byte in report.bytes() {
            context.ingest(&[byte]);
        }
        assert!(context.ready());
        assert_eq!(context.kind(), expected);
        assert_eq!(context.id(), id);
        assert_eq!(context.local_shell_ready(), id == "local");
        let epoch = context.generation();
        context.ingest(report.as_bytes());
        assert_ne!(
            epoch,
            context.generation(),
            "identical prompt is a new epoch"
        );
        context.invalidate();
        assert!(!context.ready());
    }
}

#[test]
fn prompt_text_and_legacy_metadata_never_grant_readiness() {
    let mut context = InputContext::default();
    context.ingest(b"PS C:\\> $ # ssh bash opencode\x1b]777;kea;prompt;L3RtcA==\x07");
    assert!(!context.ready());
    assert_eq!(context.kind(), ReceiverKind::Unknown);
}

#[test]
fn malformed_oversized_and_nonempty_reports_fail_closed() {
    let mut context = InputContext::default();
    context.ingest(b"\x1b]779;kea;input;1;local;posix;ready\x07");
    context.ingest(b"\x1b]779;kea;input;1;local;posix;nonempty\x1b\\");
    assert_eq!(context.readiness(), Readiness::Nonempty);
    context.ingest(b"\x1b]779;kea;input;1;local;posix;ready;extra\x07");
    assert!(!context.ready());
    context.ingest(PREFIX);
    context.ingest(&vec![b'x'; MAX_MARKER * 4]);
    assert!(context.pending.len() < MAX_MARKER);
    assert!(!context.ready());
}
