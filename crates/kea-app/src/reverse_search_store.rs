//! Private, versioned per-entry storage. No monolithic snapshot can overwrite
//! another Kea instance's unrelated memories. All callers run on a worker thread.
use crate::reverse_search::{Entry, InputKind, Library, MAX_ENTRIES, MAX_TEXT_BYTES};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

const MAGIC: &[u8] = b"KEA-MEMORY\0\x01";
const MAX_FILE_BYTES: usize = MAX_TEXT_BYTES + 10 * 1024;

pub fn data_directory() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("KEA_MEMORY_DIR") {
        return Some(path.into());
    }
    if cfg!(target_os = "windows") {
        return std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|p| p.join("Kea/input-memory"));
    }
    if cfg!(target_os = "macos") {
        return std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|p| p.join("Library/Application Support/Kea/input-memory"));
    }
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|p| p.join(".local/share"))
        })
        .map(|p| p.join("kea/input-memory"))
}

pub struct Store {
    directory: PathBuf,
}

impl Store {
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    fn check_directory(&self, create: bool) -> io::Result<()> {
        if !self.directory.is_absolute() {
            return Err(invalid("KEA_MEMORY_DIR must be absolute"));
        }
        if create && !self.directory.exists() {
            let mut builder = fs::DirBuilder::new();
            builder.recursive(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt as _;
                builder.mode(0o700);
            }
            builder.create(&self.directory)?;
        }
        let metadata = fs::symlink_metadata(&self.directory)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(invalid(
                "Input-memory directory must be a real directory, not a symlink",
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(invalid(
                    "Input-memory directory is not private; restrict its permissions to 0700",
                ));
            }
        }
        Ok(())
    }

    pub fn load(&self) -> Result<(Library, Vec<String>), String> {
        match self.check_directory(false) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok((Library::default(), vec![]))
            }
            result => result.map_err(|e| e.to_string())?,
        }
        let mut library = Library::default();
        let mut warnings = Vec::new();
        let mut paths = Vec::new();
        for (count, file) in fs::read_dir(&self.directory)
            .map_err(|e| e.to_string())?
            .enumerate()
        {
            if count >= MAX_ENTRIES * 2 {
                return Err(
                    "Too many files in the input-memory directory; loading stopped.".into(),
                );
            }
            let file = file.map_err(|e| e.to_string())?;
            let path = file.path();
            if path.extension().and_then(|s| s.to_str()) != Some("kmem") {
                continue;
            }
            if paths.len() >= MAX_ENTRIES {
                return Err("Stored input memory exceeds the 5,000-entry limit.".into());
            }
            paths.push(path);
        }
        // Deterministic partial recovery if individual files are damaged.
        paths.sort();
        for path in paths {
            let result = read_entry(&path)
                .map_err(|e| e.to_string())
                .and_then(|entry| library.insert(entry));
            if let Err(error) = result {
                if warnings.len() < 3 {
                    warnings.push(format!("Skipped an input-memory entry: {error}"));
                }
            }
        }
        Ok((library, warnings))
    }

    pub fn write(&self, entry: &Entry) -> Result<(), String> {
        entry.validate()?;
        let result = (|| -> io::Result<()> {
            self.check_directory(true)?;
            let target = self.directory.join(format!("{}.kmem", entry.id));
            match fs::symlink_metadata(&target) {
                Ok(metadata) if !metadata.is_file() || metadata.file_type().is_symlink() => {
                    return Err(invalid("Refusing to replace a non-regular memory file"));
                }
                Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
                _ => {}
            }
            let suffix = Entry::submitted("temporary".into(), InputKind::Application, None).id;
            let temporary = self.directory.join(format!("{suffix}.partial"));
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt as _;
                options.mode(0o600);
            }
            let mut file = options.open(&temporary)?;
            let result = (|| {
                file.write_all(&encode(entry))?;
                file.sync_all()?;
                drop(file);
                // Same-directory rename publishes a complete record on supported
                // filesystems, including Windows. Failures never erase the old file.
                fs::rename(&temporary, &target)
            })();
            if result.is_err() {
                let _ = fs::remove_file(&temporary);
            }
            result
        })();
        result.map_err(|e| format!("Input memory was not saved: {e}"))
    }

    pub fn remove(&self, id: &str) -> Result<(), String> {
        if id.is_empty()
            || id.len() > 100
            || !id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
        {
            return Err("Invalid memory identifier.".into());
        }
        match self.check_directory(false) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            result => result.map_err(|e| e.to_string())?,
        }
        match fs::remove_file(self.directory.join(format!("{id}.kmem"))) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("Input memory was not deleted: {error}")),
        }
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn encode(entry: &Entry) -> Vec<u8> {
    fn text(out: &mut Vec<u8>, text: &str) {
        out.extend_from_slice(&(text.len() as u32).to_le_bytes());
        out.extend_from_slice(text.as_bytes());
    }
    let mut out = MAGIC.to_vec();
    out.push(match entry.kind {
        InputKind::Posix => 0,
        InputKind::PowerShell => 1,
        InputKind::Application => 2,
    });
    out.extend_from_slice(&entry.submitted_ms.to_le_bytes());
    text(&mut out, &entry.id);
    text(&mut out, &entry.text);
    for value in [&entry.directory, &entry.name, &entry.scope] {
        out.push(u8::from(value.is_some()));
        if let Some(value) = value {
            text(&mut out, value);
        }
    }
    out
}

fn read_entry(path: &Path) -> io::Result<Entry> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_FILE_BYTES as u64
    {
        return Err(invalid("Not a bounded regular memory file"));
    }
    let mut bytes = Vec::new();
    File::open(path)?
        .take((MAX_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(invalid("Memory file grew beyond its limit"));
    }
    let entry = decode(&bytes)?;
    if path.file_stem().and_then(|s| s.to_str()) != Some(entry.id.as_str()) {
        return Err(invalid("Memory identifier does not match its file"));
    }
    Ok(entry)
}

fn decode(bytes: &[u8]) -> io::Result<Entry> {
    fn take<'a>(bytes: &mut &'a [u8], count: usize) -> io::Result<&'a [u8]> {
        let result = bytes
            .get(..count)
            .ok_or_else(|| invalid("Truncated memory file"))?;
        *bytes = &bytes[count..];
        Ok(result)
    }
    fn text(bytes: &mut &[u8]) -> io::Result<String> {
        let length = u32::from_le_bytes(take(bytes, 4)?.try_into().unwrap()) as usize;
        let text = std::str::from_utf8(take(bytes, length)?)
            .map_err(|_| invalid("Invalid UTF-8 in memory"))?;
        Ok(text.to_string())
    }
    fn optional(bytes: &mut &[u8]) -> io::Result<Option<String>> {
        match take(bytes, 1)?[0] {
            0 => Ok(None),
            1 => text(bytes).map(Some),
            _ => Err(invalid("Invalid optional field")),
        }
    }
    let mut bytes = bytes;
    if take(&mut bytes, MAGIC.len())? != MAGIC {
        return Err(invalid("Unsupported memory format/version"));
    }
    let kind = match take(&mut bytes, 1)?[0] {
        0 => InputKind::Posix,
        1 => InputKind::PowerShell,
        2 => InputKind::Application,
        _ => return Err(invalid("Unknown input kind")),
    };
    let submitted_ms = u64::from_le_bytes(take(&mut bytes, 8)?.try_into().unwrap());
    let entry = Entry {
        id: text(&mut bytes)?,
        text: text(&mut bytes)?,
        kind,
        submitted_ms,
        directory: optional(&mut bytes)?,
        name: optional(&mut bytes)?,
        scope: optional(&mut bytes)?,
    };
    if !bytes.is_empty() {
        return Err(invalid("Unexpected trailing memory data"));
    }
    entry.validate().map_err(|e| invalid(&e))?;
    Ok(entry)
}

#[cfg(test)]
mod tests {
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
}
