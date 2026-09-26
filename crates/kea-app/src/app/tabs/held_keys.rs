//! Physical key ownership follows the window, not the selected terminal.
//! No pending command, interrupt bytes, or receiver authorization is transferred.
use super::KeaView;
use kea_app::terminal::interrupt::KeyLatch;
use std::mem;

#[derive(Default)]
pub(super) struct HeldWorkflowKeys {
    composer_latch: KeyLatch,
    composer_owned: Vec<String>,
    interrupt_latch: KeyLatch,
    interrupt_owned: Vec<String>,
}

impl HeldWorkflowKeys {
    pub(super) fn take(view: &mut KeaView) -> Self {
        Self {
            composer_latch: mem::take(&mut view.composer_workflow.key_latch),
            composer_owned: mem::take(&mut view.composer_workflow.owned_keys),
            interrupt_latch: mem::take(&mut view.interrupt.key_latch),
            interrupt_owned: mem::take(&mut view.interrupt.swallowed),
        }
    }

    pub(super) fn restore(self, view: &mut KeaView) {
        view.composer_workflow.key_latch = self.composer_latch;
        view.composer_workflow.owned_keys = self.composer_owned;
        view.interrupt.key_latch = self.interrupt_latch;
        view.interrupt.swallowed = self.interrupt_owned;
    }

    pub(super) fn release(&mut self, key: &str) {
        let physical = if key == "return" { "enter" } else { key };
        self.composer_latch.release(physical);
        self.interrupt_latch.release(physical);
        self.composer_owned.retain(|held| held != physical);
        self.interrupt_owned.retain(|held| held != physical);
    }
}
