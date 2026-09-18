use alacritty_terminal::{
    grid::Scroll,
    index::{Column, Direction, Point as GridPoint, Side},
    selection::{Selection, SelectionType},
    term::{cell::LineLength, TermMode},
    vi_mode::{ViModeCursor, ViMotion},
};

use crate::Engine;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalPoint {
    pub row: usize,
    pub column: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionMotion {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
}

impl Engine {
    pub fn scroll_lines(&mut self, lines: i32) {
        let had_selection = self.has_selection();
        self.terminal.scroll_display(Scroll::Delta(lines));
        self.reconcile_selection(had_selection, None);
    }

    pub fn scroll_bottom(&mut self) {
        let had_selection = self.has_selection();
        self.terminal.scroll_display(Scroll::Bottom);
        self.reconcile_selection(had_selection, None);
    }

    pub fn begin_selection(&mut self, point: TerminalPoint) {
        self.begin_selection_kind(point, false);
    }

    pub fn update_selection(&mut self, point: TerminalPoint) {
        self.extend_selection(point);
    }

    pub fn extend_selection(&mut self, point: TerminalPoint) {
        let point = self.grid_point(point);
        let anchor = self.selection_anchor.or(self.selection_head);
        if let Some(anchor) = anchor {
            self.selection_anchor = Some(anchor);
            self.selection_head = Some(point);
            self.apply_selection();
        }
    }

    pub fn clear_selection(&mut self) {
        self.terminal.selection = None;
        self.selection_anchor = None;
        self.selection_head = None;
        self.selection_explicit = false;
        self.selection_invalidated = false;
        self.selection_block = false;
    }

    pub fn local_selection_active(&self) -> bool {
        self.has_selection() || self.selection_head.is_some()
    }

    pub fn explicit_selection_active(&self) -> bool {
        self.selection_explicit && self.local_selection_active()
    }

    pub fn enter_local_selection(&mut self) {
        if self.local_selection_active() {
            return;
        }
        let point = if self.terminal.mode().contains(TermMode::SHOW_CURSOR) {
            let cursor = self.terminal.grid().cursor.point;
            alacritty_terminal::term::point_to_viewport(self.display_offset(), cursor)
                .filter(|point| {
                    point.line < usize::from(self.size.rows)
                        && point.column.0 < usize::from(self.size.columns)
                })
                .map(|_| cursor)
                .unwrap_or_else(|| self.grid_point(TerminalPoint { row: 0, column: 0 }))
        } else {
            self.grid_point(TerminalPoint { row: 0, column: 0 })
        };
        self.selection_head = Some(point);
        self.selection_explicit = true;
        self.selection_invalidated = false;
        self.selection_block = false;
    }

    pub fn begin_selection_kind(&mut self, point: TerminalPoint, block: bool) {
        let point = self.grid_point(point);
        self.selection_anchor = Some(point);
        self.selection_head = Some(point);
        self.selection_block = block;
        // An explicit interaction may own this drag; clearing the range does
        // not implicitly give ownership back to the child.
        self.selection_invalidated = false;
        self.terminal.selection = Some(Selection::new(
            if block {
                SelectionType::Block
            } else {
                SelectionType::Simple
            },
            point,
            Side::Left,
        ));
    }

    pub fn set_selection_block(&mut self, block: bool) {
        self.selection_block = block;
        if self.has_selection() {
            self.apply_selection();
        }
    }

    pub fn place_selection_caret(&mut self, point: TerminalPoint) {
        self.terminal.selection = None;
        self.selection_anchor = None;
        self.selection_head = Some(self.grid_point(point));
        self.selection_explicit = true;
        self.selection_invalidated = false;
        self.selection_block = false;
    }

    pub fn selection_focus_lost(&mut self) {
        if !self.has_selection() {
            self.clear_selection();
        }
    }

    pub fn move_selection(&mut self, motion: SelectionMotion, extend: bool) {
        if !self.local_selection_active() {
            return;
        }
        let Some(current) = self.selection_head else {
            return;
        };
        let had_range = self.has_selection();
        if !extend && had_range && matches!(motion, SelectionMotion::Left | SelectionMotion::Right)
        {
            let range = self
                .terminal
                .selection
                .as_ref()
                .and_then(|selection| selection.to_range(&self.terminal));
            let Some(range) = range else { return };
            let point = if motion == SelectionMotion::Left {
                range.start
            } else {
                range.end
            };
            self.terminal.selection = None;
            self.selection_anchor = None;
            self.selection_head = Some(point);
            self.selection_explicit = true;
            self.selection_block = false;
            self.selection_invalidated = false;
            self.scroll_to_caret();
            return;
        }
        if !extend && had_range {
            self.terminal.selection = None;
            self.selection_anchor = None;
        }
        let point = self.moved_point(current, motion);
        self.selection_head = Some(point);
        self.selection_explicit = true;
        if extend {
            self.selection_anchor.get_or_insert(current);
            self.apply_selection();
        } else {
            self.terminal.selection = None;
            self.selection_anchor = None;
            self.selection_block = false;
        }
        self.selection_invalidated = false;
        self.scroll_to_caret();
    }

    pub fn selection_invalidated(&self) -> bool {
        self.selection_invalidated
    }

    pub fn selection_text(&self) -> Option<String> {
        self.terminal
            .selection_to_string()
            .filter(|text| !text.is_empty())
    }

    pub fn has_selection(&self) -> bool {
        self.terminal.selection.as_ref().is_some_and(|selection| {
            !selection.is_empty() && selection.to_range(&self.terminal).is_some()
        })
    }

    fn apply_selection(&mut self) {
        let (Some(anchor), Some(head)) = (self.selection_anchor, self.selection_head) else {
            return;
        };
        let ty = if self.selection_block {
            SelectionType::Block
        } else {
            SelectionType::Simple
        };
        let mut selection = Selection::new(ty, anchor, Side::Left);
        selection.update(head, Side::Right);
        selection.include_all();
        self.terminal.selection = Some(selection);
    }

    fn moved_point(&mut self, current: GridPoint, motion: SelectionMotion) -> GridPoint {
        match motion {
            SelectionMotion::Home => GridPoint::new(current.line, Column(0)),
            SelectionMotion::End => {
                let row = &self.terminal.grid()[current.line];
                let column = row.line_length().saturating_sub(1);
                self.terminal.expand_wide(
                    GridPoint::new(current.line, Column(column)),
                    Direction::Left,
                )
            }
            SelectionMotion::PageUp | SelectionMotion::PageDown => {
                let lines = usize::from(self.size.rows) as i32;
                let lines = if motion == SelectionMotion::PageUp {
                    lines
                } else {
                    -lines
                };
                let cursor = ViModeCursor::new(current).scroll(&self.terminal, lines);
                self.terminal.scroll_display(Scroll::Delta(lines));
                self.terminal.scroll_to_point(cursor.point);
                cursor.point
            }
            SelectionMotion::Left => {
                ViModeCursor::new(current)
                    .motion(&mut self.terminal, ViMotion::Left)
                    .point
            }
            SelectionMotion::Right => {
                ViModeCursor::new(current)
                    .motion(&mut self.terminal, ViMotion::Right)
                    .point
            }
            SelectionMotion::Up => {
                ViModeCursor::new(current)
                    .motion(&mut self.terminal, ViMotion::Up)
                    .point
            }
            SelectionMotion::Down => {
                ViModeCursor::new(current)
                    .motion(&mut self.terminal, ViMotion::Down)
                    .point
            }
        }
    }

    fn scroll_to_caret(&mut self) {
        if let Some(point) = self.selection_head {
            self.terminal.scroll_to_point(point);
            if !self.has_selection() {
                self.clamp_caret_to_viewport();
            }
        }
    }

    fn clamp_caret_to_viewport(&mut self) {
        let Some(mut point) = self.selection_head else {
            return;
        };
        let top = -(self.display_offset() as i32);
        let bottom = top + i32::from(self.size.rows) - 1;
        point.line.0 = point.line.0.clamp(top, bottom);
        point.column.0 = point
            .column
            .0
            .min(usize::from(self.size.columns).saturating_sub(1));
        self.selection_head = Some(point);
    }

    fn grid_point(&self, point: TerminalPoint) -> GridPoint {
        let row = point.row.min(usize::from(self.size.rows).saturating_sub(1));
        let column = point
            .column
            .min(usize::from(self.size.columns).saturating_sub(1));
        alacritty_terminal::term::viewport_to_point(
            self.display_offset(),
            GridPoint::new(row, Column(column)),
        )
    }

    pub(crate) fn selection_snapshot(&self) -> Option<String> {
        self.terminal.selection_to_string()
    }

    fn invalidate_selection(&mut self) {
        self.terminal.selection = None;
        self.selection_anchor = None;
        self.selection_head = Some(self.grid_point(TerminalPoint { row: 0, column: 0 }));
        self.selection_explicit = true;
        self.selection_invalidated = true;
        self.selection_block = false;
        self.clamp_caret_to_viewport();
    }

    pub(crate) fn reconcile_selection(
        &mut self,
        had_selection: bool,
        old_snapshot: Option<String>,
    ) {
        if had_selection && self.has_selection() {
            if old_snapshot.is_some() && old_snapshot != self.selection_snapshot() {
                self.invalidate_selection();
                return;
            }
            if let Some(range) = self
                .terminal
                .selection
                .as_ref()
                .and_then(|selection| selection.to_range(&self.terminal))
            {
                let Some((anchor, head)) = self.selection_anchor.zip(self.selection_head) else {
                    return;
                };
                if self.selection_block {
                    let row_forward = anchor.line <= head.line;
                    let column_forward = anchor.column <= head.column;
                    self.selection_anchor = Some(GridPoint::new(
                        if row_forward {
                            range.start.line
                        } else {
                            range.end.line
                        },
                        if column_forward {
                            range.start.column
                        } else {
                            range.end.column
                        },
                    ));
                    self.selection_head = Some(GridPoint::new(
                        if row_forward {
                            range.end.line
                        } else {
                            range.start.line
                        },
                        if column_forward {
                            range.end.column
                        } else {
                            range.start.column
                        },
                    ));
                } else if anchor <= head {
                    self.selection_anchor = Some(range.start);
                    self.selection_head = Some(range.end);
                } else {
                    self.selection_anchor = Some(range.end);
                    self.selection_head = Some(range.start);
                }
            }
        } else if had_selection {
            self.invalidate_selection();
        } else if self.selection_head.is_some() {
            self.clamp_caret_to_viewport();
            if self.selection_anchor.is_some() {
                // An unextended pointer anchor is still empty, not a range to
                // resurrect at pre-resize or evicted coordinates.
                self.selection_anchor = self.selection_head;
                self.terminal.selection = None;
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/selection.rs"]
mod tests;
