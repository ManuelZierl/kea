//! Per-terminal draft sections and explicitly accepted editor transformations.
//! InputState remains the owner of all text, selection, IME and text undo.
use super::*;
use gpui_component::input::{Input, InputEvent};
use kea_app::{
    editor::workflow::{self, EditPlan, Recommendation},
    terminal::interrupt::KeyLatch,
};

const MAX_LAYOUT_HISTORY: usize = 8;
const MAX_LAYOUT_HISTORY_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone)]
struct LayoutSnapshot {
    editors: Vec<Entity<InputState>>,
    values: Vec<String>,
    active: usize,
}

impl LayoutSnapshot {
    fn bytes(&self) -> usize {
        self.values.iter().map(String::len).sum()
    }
    fn matches(&self, sections: &[Entity<InputState>], cx: &App) -> bool {
        self.editors == sections
            && self
                .editors
                .iter()
                .zip(&self.values)
                .all(|(editor, value)| editor.read(cx).value().as_ref() == value)
    }
}

struct LayoutChange {
    before: LayoutSnapshot,
    after: LayoutSnapshot,
}
impl LayoutChange {
    fn bytes(&self) -> usize {
        self.before.bytes() + self.after.bytes()
    }
}

#[derive(Clone)]
enum ComposerAction {
    Split,
    Add,
    Merge,
    Previous,
    Next,
    UndoLayout,
    RedoLayout,
    History,
    SendSelection,
    Recommendation(Recommendation),
}

impl ComposerAction {
    fn label(&self) -> String {
        match self {
            Self::Split => "Split at cursor".into(),
            Self::Add => "Add empty draft below".into(),
            Self::Merge => "Merge with previous draft".into(),
            Self::Previous => "Focus previous draft".into(),
            Self::Next => "Focus next draft".into(),
            Self::UndoLayout => "Undo composer split / merge".into(),
            Self::RedoLayout => "Redo composer split / merge".into(),
            Self::History => "Insert saved memory or submitted input".into(),
            Self::SendSelection => "Send selected text (keep draft)".into(),
            Self::Recommendation(rec) => rec.label(),
        }
    }
}

pub(super) struct ComposerMenu {
    target: Entity<InputState>,
    source: String,
    cursor: usize,
    items: Vec<ComposerAction>,
    selected: usize,
    prefix_range: Option<std::ops::Range<usize>>,
    placeholder: Option<Recommendation>,
    value: Entity<InputState>,
    preview: Option<EditPlan>,
    preview_editors: Vec<Entity<InputState>>,
    focus: FocusHandle,
}

type RecommendationCache = (
    Entity<InputState>,
    SharedString,
    usize,
    String,
    Vec<Recommendation>,
);

pub(super) struct ComposerWorkflow {
    pub sections: Vec<Entity<InputState>>,
    pub active: usize,
    pub menu: Option<ComposerMenu>,
    pub key_latch: KeyLatch,
    pub owned_keys: Vec<String>,
    scroll: ScrollHandle,
    subscriptions: Vec<Subscription>,
    undo: Vec<LayoutChange>,
    redo: Vec<LayoutChange>,
    recommendations: Option<RecommendationCache>,
}

impl ComposerWorkflow {
    pub fn new(editor: Entity<InputState>, _: &mut Window, cx: &mut Context<KeaView>) -> Self {
        let sections = vec![editor];
        let subscriptions = observe_sections(&sections, cx);
        Self {
            sections,
            active: 0,
            menu: None,
            key_latch: KeyLatch::default(),
            owned_keys: Vec::new(),
            scroll: ScrollHandle::new(),
            subscriptions,
            undo: Vec::new(),
            redo: Vec::new(),
            recommendations: None,
        }
    }
}

fn observe_sections(
    sections: &[Entity<InputState>],
    cx: &mut Context<KeaView>,
) -> Vec<Subscription> {
    sections
        .iter()
        .map(|editor| {
            cx.subscribe(editor, |this, editor, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Focus) {
                    if let Some(index) = this
                        .composer_workflow
                        .sections
                        .iter()
                        .position(|e| *e == editor)
                    {
                        if this.editor != editor {
                            this.select_composer_section(index, cx);
                        }
                    }
                }
                if matches!(event, InputEvent::Change | InputEvent::Focus) {
                    cx.notify();
                }
            })
        })
        .collect()
}

mod keyboard;
mod rendering;
mod sections;
mod suggestions;

#[cfg(test)]
#[path = "../../tests/unit/app/composer_workflow.rs"]
mod tests;
