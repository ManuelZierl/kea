use super::*;

#[test]
fn terminalish_text_hides_ansi_and_applies_carriage_return_overwrite() {
    assert_eq!(
        terminalish_text(b"\x1b[31mDownloading 10%\x1b[0m\rDownloading 100%\nDone\n"),
        "Downloading 100%\nDone"
    );
}
