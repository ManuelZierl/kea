//! A one-shot confirmation gate. It stores already encoded input, not signals.
//! Target identity/focus are checked by the host; generations are checked here.
#[derive(Default)]
pub struct InterruptGuard {
    pending: Option<(u64, Vec<u8>)>,
}

impl InterruptGuard {
    pub fn is_pending(&self) -> bool { self.pending.is_some() }

    /// Repeated requests cannot replace or accumulate an armed interrupt.
    pub fn arm(&mut self, generation: u64, bytes: Vec<u8>) {
        if self.pending.is_none() { self.pending = Some((generation, bytes)); }
    }

    pub fn cancel(&mut self) { self.pending = None; }

    pub fn validate(&mut self, generation: u64, target_active: bool) {
        if !target_active || self.pending.as_ref().is_some_and(|(old, _)| *old != generation) {
            self.cancel();
        }
    }

    pub fn confirm(&mut self, generation: u64, fresh_press: bool) -> Option<Vec<u8>> {
        self.validate(generation, true);
        if !fresh_press { return None; }
        self.pending.take().map(|(_, bytes)| bytes)
    }
}

/// Physical key names, not modifier chords: releasing Control while holding
/// Enter must not make autorepeat look like a new submission.
#[derive(Default)]
pub struct KeyLatch { down: Vec<String> }

impl KeyLatch {
    pub fn press(&mut self, key: &str) -> bool {
        let key = if key == "return" { "enter" } else { key };
        if self.down.iter().any(|held| held == key) { return false; }
        // Only host submission/confirmation keys are tracked, with a hard bound.
        if self.down.len() >= 16 { return false; }
        self.down.push(key.to_owned());
        true
    }

    pub fn release(&mut self, key: &str) {
        let key = if key == "return" { "enter" } else { key };
        self.down.retain(|held| held != key);
    }

    pub fn clear(&mut self) { self.down.clear(); }
}

#[cfg(test)]
#[path = "../../tests/unit/terminal/interrupt.rs"]
mod tests;
