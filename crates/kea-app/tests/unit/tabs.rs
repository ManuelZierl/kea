use super::*;

#[test]
fn switching_retains_the_entire_owned_context() {
    let mut tabs = Tabs::default();
    let a = tabs.insert(("draft A".to_string(), vec![1, 2])).unwrap();
    let b = tabs.insert(("draft B".to_string(), vec![3])).unwrap();
    tabs.get_mut(a).unwrap().1.push(4); // background output
    assert!(tabs.activate(a));
    assert_eq!(tabs.active().unwrap(), &("draft A".into(), vec![1, 2, 4]));
    assert_eq!(tabs.get(b).unwrap().0, "draft B");
    assert!(!tabs.activate(TabId(999)));
    assert_eq!(tabs.active_id(), Some(a));
}

#[test]
fn close_and_reorder_use_ids_not_positions() {
    let mut tabs = Tabs::default();
    let a = tabs.insert("A").unwrap();
    let b = tabs.insert("B").unwrap();
    let c = tabs.insert("C").unwrap();
    assert!(tabs.reorder(a, c));
    assert_eq!(tabs.active_id(), Some(c));
    assert_eq!(tabs.remove(b), Some("B"));
    assert_eq!(tabs.active_id(), Some(c));
    assert_eq!(tabs.remove(c), Some("C"));
    assert_eq!(tabs.active_id(), Some(a));
    assert_eq!(tabs.remove(c), None); // stale confirmation is harmless
    assert_eq!(tabs.remove(a), Some("A"));
    assert!(tabs.is_empty());
    assert_eq!(tabs.active_id(), None);
    assert_eq!(tabs.adjacent(true), None);
    let d = tabs.insert("D").unwrap();
    assert!(d.0 > c.0); // closed identities are never reused
}

#[test]
fn cycling_wraps_but_reordering_stops_at_edges() {
    let mut tabs = Tabs::default();
    let a = tabs.insert("A").unwrap();
    let b = tabs.insert("B").unwrap();
    let c = tabs.insert("C").unwrap();
    assert_eq!(tabs.adjacent(true), Some(a));
    assert_eq!(tabs.adjacent(false), Some(b));
    assert!(!tabs.move_active(true));
    assert!(tabs.move_active(false));
    assert_eq!(tabs.iter().map(|(id, _)| id).collect::<Vec<_>>(), vec![a, c, b]);
    assert_eq!(tabs.active_id(), Some(c));
}

#[test]
fn removing_active_selects_neighbor_without_dropping_other_contexts() {
    let mut tabs = Tabs::default();
    let a = tabs.insert("A").unwrap();
    let b = tabs.insert("B").unwrap();
    let c = tabs.insert("C").unwrap();
    tabs.activate(b);
    tabs.remove(b);
    assert_eq!(tabs.active_id(), Some(c));
    tabs.remove(c);
    assert_eq!(tabs.active_id(), Some(a));
}

#[test]
fn capacity_failure_returns_ownership_and_does_not_change_selection() {
    let mut tabs = Tabs::default();
    for n in 0..MAX_TERMINALS { tabs.insert(n).unwrap(); }
    let active = tabs.active_id();
    assert!(tabs.is_full());
    assert_eq!(tabs.insert(12345), Err(12345));
    assert_eq!(tabs.len(), MAX_TERMINALS);
    assert_eq!(tabs.active_id(), active);
    tabs.remove(TabId(0));
    assert!(tabs.insert(12345).is_ok());
}
