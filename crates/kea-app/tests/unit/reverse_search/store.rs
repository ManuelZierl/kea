use super::*;

struct Temporary(PathBuf);
impl Temporary {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "kea-memory-{}",
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
fn round_trip_keeps_unicode_multiline_empty_options_and_scopes() {
    let root = Temporary::new();
    let store = Store::new(root.0.clone());
    let mut entry = Entry::submitted(
        "printf '😀\\n'\necho ä".into(),
        InputKind::Posix,
        Some("/tmp/a b".into()),
    );
    entry.name = Some("greeting".into());
    entry.scope = entry.directory.clone();
    store.write(&entry).unwrap();
    let (loaded, warnings) = store.load().unwrap();
    assert!(warnings.is_empty());
    assert_eq!(loaded.entries(), &[entry.clone()]);
    entry.name = Some("renamed".into());
    store.write(&entry).unwrap();
    assert_eq!(store.load().unwrap().0.entries(), &[entry.clone()]);
    store.remove(&entry.id).unwrap();
    assert!(store.load().unwrap().0.entries().is_empty());
}

#[test]
fn truncated_and_unknown_versions_fail_without_accepting_partial_records() {
    let entry = Entry::submitted("echo ok".into(), InputKind::Posix, None);
    let encoded = encode(&entry);
    for length in 0..encoded.len() {
        assert!(decode(&encoded[..length]).is_err());
    }
    let mut bad = encoded.clone();
    bad[0] = b'X';
    assert!(decode(&bad).is_err());
    let mut bad = encoded;
    bad.push(0);
    assert!(decode(&bad).is_err());
}

#[test]
fn independent_instances_do_not_overwrite_each_others_entries() {
    let root = Temporary::new();
    let a = Store::new(root.0.clone());
    let b = Store::new(root.0.clone());
    a.write(&Entry::submitted("one".into(), InputKind::Posix, None))
        .unwrap();
    b.write(&Entry::submitted("two".into(), InputKind::Posix, None))
        .unwrap();
    assert_eq!(a.load().unwrap().0.entries().len(), 2);
}

#[test]
fn opening_missing_storage_does_not_create_or_write_anything() {
    let root = Temporary::new();
    assert!(Store::new(root.0.clone())
        .load()
        .unwrap()
        .0
        .entries()
        .is_empty());
    assert!(!root.0.exists());
}

#[cfg(unix)]
#[test]
fn files_are_private_and_symlink_directories_are_rejected() {
    use std::os::unix::fs::{symlink, PermissionsExt as _};
    let root = Temporary::new();
    let store = Store::new(root.0.clone());
    let entry = Entry::submitted("secret".into(), InputKind::Posix, None);
    store.write(&entry).unwrap();
    assert_eq!(
        fs::metadata(&root.0).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(root.0.join(format!("{}.kmem", entry.id)))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let link = root.0.join("link");
    symlink(&root.0, &link).unwrap();
    assert!(Store::new(link).load().is_err());
}
