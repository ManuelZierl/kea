use super::*;
use kea_core::Projection;

#[test]
fn scrollback_projects_the_selected_viewport_and_returns_to_tail() {
    let mut engine = Engine::new(Size::new(8, 3).unwrap(), false);
    engine.output(b"one\r\ntwo\r\nthree\r\nfour");

    assert_eq!(engine.display_offset(), 0);
    assert!(engine.history_size() > 0);
    assert!(engine.screen().text().contains("four"));

    engine.scroll_lines(2);
    let scrolled = engine.screen();
    assert!(scrolled.display_offset > 0);
    assert!(scrolled.text().contains("one"));

    engine.scroll_bottom();
    assert_eq!(engine.display_offset(), 0);
    assert!(engine.screen().text().contains("four"));
}
