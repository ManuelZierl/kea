//! A bounded worker owns disk I/O and ranking. Failure cannot delay PTY submission.
use crate::{
    reverse_search::{now_ms, Entry, Hit, Library},
    reverse_search_store::Store,
};
use std::{collections::HashSet, path::PathBuf, sync::mpsc};

pub enum Request {
    Refresh,
    Search { generation: u64, query: String, directory: Option<String> },
    Record(Entry),
    Save(Entry),
    Delete(Vec<String>),
}

pub enum Reply {
    Results { generation: u64, hits: Vec<Hit>, error: Option<String> },
    Saved,
    Deleted,
    Warning(String),
    OperationFailed(String),
}

pub struct Worker {
    requests: mpsc::SyncSender<Request>,
    pub replies: mpsc::Receiver<Reply>,
}

impl Worker {
    pub fn start(directory: Option<PathBuf>, persist_history: bool) -> Result<Self, String> {
        let (requests, receiver) = mpsc::sync_channel(32);
        let (sender, replies) = mpsc::sync_channel(32);
        std::thread::Builder::new().name("kea-input-memory".into()).spawn(move || {
            let mut worker = State {
                store: directory.map(Store::new), library: Library::default(), ephemeral: HashSet::new(), persist_history,
            };
            // Load existing durable records before accepting new writes so retention
            // accounting does not start from zero in each process.
            if let Some(reply) = worker.handle(Request::Refresh) {
                if sender.send(reply).is_err() { return; }
            }
            for request in receiver {
                let reply = worker.handle(request);
                if let Some(reply) = reply {
                    if sender.send(reply).is_err() { break; }
                }
            }
        }).map_err(|e| format!("Input-memory worker could not start: {e}"))?;
        Ok(Self { requests, replies })
    }

    pub fn send(&self, request: Request) -> Result<(), String> {
        self.requests.try_send(request).map_err(|error| match error {
            mpsc::TrySendError::Full(_) => "Input-memory worker is busy; this memory operation was not queued. Terminal execution is unaffected.".into(),
            mpsc::TrySendError::Disconnected(_) => "Input-memory worker stopped. Terminal execution is unaffected.".into(),
        })
    }
}

struct State {
    store: Option<Store>,
    library: Library,
    ephemeral: HashSet<String>,
    persist_history: bool,
}

impl State {
    fn refresh(&mut self) -> Result<Vec<String>, String> {
        let Some(store) = &self.store else { return Ok(vec![]); };
        let (mut loaded, mut warnings) = store.load()?;
        for entry in self.library.entries().iter().filter(|entry| self.ephemeral.contains(&entry.id)) {
            if let Err(error) = loaded.insert(entry.clone()) {
                warnings.push(error);
                break;
            }
        }
        self.library = loaded;
        Ok(warnings)
    }

    fn handle(&mut self, request: Request) -> Option<Reply> {
        match request {
            Request::Refresh => match self.refresh() {
                Ok(warnings) if warnings.is_empty() => None,
                Ok(warnings) => Some(Reply::Warning(warnings.join(" "))),
                Err(error) => Some(Reply::Warning(error)),
            },
            Request::Search { generation, query, directory } => {
                let (hits, error) = match self.library.search(&query, directory.as_deref(), now_ms()) {
                    Ok(hits) => (hits, None), Err(error) => (vec![], Some(error)),
                };
                Some(Reply::Results { generation, hits, error })
            }
            Request::Record(entry) => {
                // Leading-space input is deliberately excluded from automatic memory.
                // Explicit Save is still allowed. This is not a secret scanner.
                if entry.text.starts_with(' ') || entry.text.trim().is_empty() { return None; }
                if let Err(error) = self.library.insert(entry.clone()) { return Some(Reply::Warning(error)); }
                self.ephemeral.insert(entry.id.clone());
                if self.persist_history {
                    match self.store.as_ref().ok_or_else(|| "No input-memory data directory is available.".to_string())
                        .and_then(|store| store.write(&entry)) {
                        Ok(()) => { self.ephemeral.remove(&entry.id); }
                        Err(error) => return Some(Reply::Warning(error)),
                    }
                }
                None
            }
            Request::Save(entry) => {
                if entry.name.is_none() { return Some(Reply::OperationFailed("Saved memories require a name.".into())); }
                if self.library.entries().iter().any(|old| old.id != entry.id && old.name == entry.name && old.scope == entry.scope && old.kind == entry.kind) {
                    return Some(Reply::OperationFailed("A memory with that name and scope already exists. Choose a different name or rename the existing memory.".into()));
                }
                // Save is explicit and infrequent. Validate the complete prospective
                // state before touching disk; a failed write leaves both copies intact.
                let mut prospective = self.library.clone();
                let result = prospective.insert(entry.clone()).and_then(|()| {
                    self.store.as_ref().ok_or_else(|| "No input-memory data directory is available.".to_string())?.write(&entry)
                });
                match result {
                    Ok(()) => {
                        self.library = prospective;
                        self.ephemeral.remove(&entry.id);
                        Some(Reply::Saved)
                    }
                    Err(error) => Some(Reply::OperationFailed(error)),
                }
            }
            Request::Delete(ids) => {
                for id in ids {
                    // Do not claim to forget a durable entry if disk deletion failed.
                    if !self.ephemeral.contains(&id) {
                        if let Some(store) = &self.store {
                            if let Err(error) = store.remove(&id) { return Some(Reply::OperationFailed(error)); }
                        }
                    }
                    self.library.remove(&id);
                    self.ephemeral.remove(&id);
                }
                Some(Reply::Deleted)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reverse_search::InputKind;
    #[test]
    fn private_submissions_are_not_recorded_and_search_does_not_write() {
        let mut state = State { store: None, library: Library::default(), ephemeral: HashSet::new(), persist_history: false };
        state.handle(Request::Record(Entry::submitted(" secret".into(), InputKind::Posix, None)));
        state.handle(Request::Record(Entry::submitted("echo public".into(), InputKind::Posix, None)));
        assert_eq!(state.library.entries().len(), 1);
        assert!(state.ephemeral.contains(&state.library.entries()[0].id));
        match state.handle(Request::Search { generation: 7, query: "public".into(), directory: None }) {
            Some(Reply::Results { generation, hits, error }) => { assert_eq!(generation, 7); assert_eq!(hits.len(), 1); assert!(error.is_none()); }
            _ => panic!("Expected search result"),
        }
    }
    #[test]
    fn persistence_failure_keeps_submitted_input_searchable() {
        let mut state = State { store: None, library: Library::default(), ephemeral: HashSet::new(), persist_history: true };
        let reply = state.handle(Request::Record(Entry::submitted("echo public".into(), InputKind::Posix, None)));
        assert!(matches!(reply, Some(Reply::Warning(_))));
        assert_eq!(state.library.entries().len(), 1);
    }
    #[test]
    fn worker_searches_with_generation_and_disconnects_cleanly() {
        let worker = Worker::start(None, false).unwrap();
        worker.send(Request::Record(Entry::submitted("echo one".into(), InputKind::Posix, None))).unwrap();
        worker.send(Request::Search { generation: 9, query: "one".into(), directory: None }).unwrap();
        match worker.replies.recv_timeout(std::time::Duration::from_secs(5)).unwrap() {
            Reply::Results { generation, hits, .. } => { assert_eq!(generation, 9); assert_eq!(hits.len(), 1); }
            _ => panic!("Expected search result"),
        }
    }
}
