//! Explicit, live input ownership. Never infer readiness from screen contents,
//! process names, idleness, or the optional command-block document.
const PREFIX: &[u8] = b"\x1b]779;kea;input;1;";
const MAX_MARKER: usize = 512;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ReceiverKind {
    Posix,
    PowerShell,
    Application,
    #[default]
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Readiness {
    Ready,
    Nonempty,
    Busy,
    #[default]
    Unknown,
}

#[derive(Debug, Default)]
pub struct InputContext {
    pending: Vec<u8>,
    id: String,
    kind: ReceiverKind,
    readiness: Readiness,
    generation: u64,
}

impl InputContext {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn kind(&self) -> ReceiverKind {
        self.kind
    }
    pub fn readiness(&self) -> Readiness {
        self.readiness
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn ready(&self) -> bool {
        self.readiness == Readiness::Ready
    }
    pub fn local_shell_ready(&self) -> bool {
        self.ready()
            && self.id == "local"
            && matches!(self.kind, ReceiverKind::Posix | ReceiverKind::PowerShell)
    }
    pub fn shell_ready(&self) -> bool {
        self.ready() && matches!(self.kind, ReceiverKind::Posix | ReceiverKind::PowerShell)
    }
    pub fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.readiness = Readiness::Unknown;
    }

    /// Bounded streaming parser; a fresh marker is an epoch even if its fields
    /// match the previous one. Output is only observed, never removed or trusted
    /// as authentication. All normal input paths must invalidate readiness.
    pub fn ingest(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            if self.pending.is_empty() && byte != PREFIX[0] {
                continue;
            }
            self.pending.push(byte);
            if self.pending.len() <= PREFIX.len() {
                if !PREFIX.starts_with(&self.pending) {
                    self.pending.clear();
                    if byte == PREFIX[0] {
                        self.pending.push(byte);
                    }
                }
                continue;
            }
            if byte == 7 || self.pending.ends_with(b"\x1b\\") {
                let end = self.pending.len() - if byte == 7 { 1 } else { 2 };
                let parsed = std::str::from_utf8(&self.pending[PREFIX.len()..end])
                    .ok()
                    .and_then(parse);
                self.invalidate();
                if let Some((id, kind, readiness)) = parsed {
                    self.id = id;
                    self.kind = kind;
                    self.readiness = readiness;
                }
                self.pending.clear();
            } else if self.pending.len() >= MAX_MARKER {
                self.invalidate();
                self.pending.clear();
            }
        }
    }
}

fn parse(body: &str) -> Option<(String, ReceiverKind, Readiness)> {
    let mut parts = body.split(';');
    let id = parts.next()?;
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    {
        return None;
    }
    let kind = match parts.next()? {
        "posix" => ReceiverKind::Posix,
        "powershell" => ReceiverKind::PowerShell,
        "app" => ReceiverKind::Application,
        _ => return None,
    };
    let readiness = match parts.next()? {
        "ready" => Readiness::Ready,
        "nonempty" => Readiness::Nonempty,
        "busy" => Readiness::Busy,
        "unknown" => Readiness::Unknown,
        _ => return None,
    };
    parts
        .next()
        .is_none()
        .then(|| (id.to_owned(), kind, readiness))
}

#[cfg(test)]
#[path = "../../tests/unit/terminal/context.rs"]
mod tests;
