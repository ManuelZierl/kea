//! UI-independent ownership and selection of terminal contexts.
//! A tab owns its whole terminal/composer context; indices are never identities.

pub const MAX_TERMINALS: usize = 32;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TabId(pub u64);

pub struct Tabs<T> {
    entries: Vec<(TabId, T)>,
    active: Option<TabId>,
    next_id: u64,
}

impl<T> Default for Tabs<T> {
    fn default() -> Self {
        Self { entries: Vec::new(), active: None, next_id: 0 }
    }
}

impl<T> Tabs<T> {
    pub fn len(&self) -> usize { self.entries.len() }
    pub fn is_empty(&self) -> bool { self.entries.is_empty() }
    pub fn is_full(&self) -> bool { self.len() >= MAX_TERMINALS }
    pub fn next_id(&self) -> TabId { TabId(self.next_id) }
    pub fn active_id(&self) -> Option<TabId> { self.active }
    pub fn iter(&self) -> impl Iterator<Item = (TabId, &T)> {
        self.entries.iter().map(|(id, value)| (*id, value))
    }
    pub fn get(&self, id: TabId) -> Option<&T> {
        self.entries.iter().find(|entry| entry.0 == id).map(|entry| &entry.1)
    }
    pub fn get_mut(&mut self, id: TabId) -> Option<&mut T> {
        self.entries.iter_mut().find(|entry| entry.0 == id).map(|entry| &mut entry.1)
    }
    pub fn active(&self) -> Option<&T> { self.active.and_then(|id| self.get(id)) }

    /// Failure returns ownership to the caller rather than dropping a context.
    pub fn insert(&mut self, value: T) -> Result<TabId, T> {
        if self.is_full() || self.next_id == u64::MAX { return Err(value); }
        let id = self.next_id();
        self.next_id += 1;
        self.entries.push((id, value));
        self.active = Some(id);
        Ok(id)
    }

    pub fn activate(&mut self, id: TabId) -> bool {
        if self.get(id).is_none() { return false; }
        self.active = Some(id);
        true
    }

    pub fn adjacent(&self, forward: bool) -> Option<TabId> {
        let at = self.entries.iter().position(|entry| Some(entry.0) == self.active)?;
        let next = if forward { (at + 1) % self.len() } else { (at + self.len() - 1) % self.len() };
        Some(self.entries[next].0)
    }

    /// Close by identity, including when a confirmation outlives a reorder.
    /// Closing a background tab must not change the selected context.
    pub fn remove(&mut self, id: TabId) -> Option<T> {
        let at = self.entries.iter().position(|entry| entry.0 == id)?;
        let (_, value) = self.entries.remove(at);
        if self.active == Some(id) {
            self.active = self.entries.get(at.min(self.len().saturating_sub(1))).map(|entry| entry.0);
        }
        Some(value)
    }

    /// Move to the target's slot, preserving the selected terminal's identity.
    pub fn reorder(&mut self, id: TabId, target: TabId) -> bool {
        let Some(from) = self.entries.iter().position(|entry| entry.0 == id) else { return false; };
        let Some(to) = self.entries.iter().position(|entry| entry.0 == target) else { return false; };
        if from == to { return false; }
        let entry = self.entries.remove(from);
        self.entries.insert(to, entry);
        true
    }

    pub fn move_active(&mut self, forward: bool) -> bool {
        let Some(id) = self.active else { return false; };
        let Some(at) = self.entries.iter().position(|entry| entry.0 == id) else { return false; };
        let target = if forward { at.checked_add(1) } else { at.checked_sub(1) };
        let Some(target) = target.and_then(|index| self.entries.get(index)).map(|entry| entry.0) else { return false; };
        self.reorder(id, target)
    }
}

#[cfg(test)]
#[path = "../tests/unit/tabs.rs"]
mod tests;
