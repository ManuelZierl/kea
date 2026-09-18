use alacritty_terminal::{
    term::{cell::Flags, TermMode},
    vte::ansi::{Color, NamedColor},
};
use kea_core::Size;

use crate::Engine;

#[derive(Clone, Debug)]
pub struct Cell {
    pub text: String,
    pub foreground: u32,
    pub background: u32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub wide: bool,
    pub spacer: bool,
    pub selected: bool,
}

#[derive(Clone, Debug)]
pub struct Screen {
    pub size: Size,
    pub cells: Vec<Cell>,
    pub cursor: Option<(usize, usize)>,
    pub display_offset: usize,
    pub history_size: usize,
    pub local_caret: Option<(usize, usize)>,
    pub selection_invalidated: bool,
}

impl Screen {
    pub fn text(&self) -> String {
        self.cells
            .chunks(usize::from(self.size.columns))
            .map(|row| {
                row.iter()
                    .filter(|c| !c.spacer)
                    .map(|c| c.text.as_str())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl Engine {
    pub fn screen(&self) -> Screen {
        let history_size = self.history_size();
        let content = self.terminal.renderable_content();
        let selection = content.selection;
        let cursor_shape = content.cursor.shape;
        let cursor_point = content.cursor.point;
        let display_offset = content.display_offset;
        let mut cells =
            Vec::with_capacity(usize::from(self.size.rows) * usize::from(self.size.columns));
        for indexed in content.display_iter {
            let cell = indexed.cell;
            let mut text = cell.c.to_string();
            if let Some(extra) = cell.zerowidth() {
                text.extend(extra);
            }
            let mut foreground = self.color(cell.fg);
            let mut background = self.color(cell.bg);
            if cell.flags.contains(Flags::INVERSE) {
                std::mem::swap(&mut foreground, &mut background);
            }
            if cell.flags.contains(Flags::HIDDEN) {
                foreground = background;
            }
            cells.push(Cell {
                text,
                foreground,
                background,
                bold: cell.flags.contains(Flags::BOLD),
                italic: cell.flags.contains(Flags::ITALIC),
                underline: cell.flags.contains(Flags::UNDERLINE),
                wide: cell.flags.contains(Flags::WIDE_CHAR),
                spacer: cell
                    .flags
                    .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER),
                selected: selection.is_some_and(|selection| {
                    selection.contains_cell(&indexed, cursor_point, cursor_shape)
                }),
            });
        }
        let cursor = (display_offset == 0 && self.terminal.mode().contains(TermMode::SHOW_CURSOR))
            .then(|| {
                let point = self.terminal.grid().cursor.point;
                (point.line.0.max(0) as usize, point.column.0)
            });
        Screen {
            size: self.size,
            cells,
            cursor,
            display_offset,
            history_size,
            local_caret: self.selection_head.and_then(|point| {
                alacritty_terminal::term::point_to_viewport(display_offset, point)
                    .filter(|point| {
                        point.line < usize::from(self.size.rows)
                            && point.column.0 < usize::from(self.size.columns)
                    })
                    .map(|point| (point.line, point.column.0))
            }),
            selection_invalidated: self.selection_invalidated,
        }
    }

    fn color(&self, color: Color) -> u32 {
        match color {
            Color::Spec(c) => (u32::from(c.r) << 16) | (u32::from(c.g) << 8) | u32::from(c.b),
            Color::Named(c) => self.indexed(c as usize),
            Color::Indexed(c) => self.indexed(usize::from(c)),
        }
    }

    fn indexed(&self, index: usize) -> u32 {
        if let Some(c) = self.terminal.colors()[index] {
            return (u32::from(c.r) << 16) | (u32::from(c.g) << 8) | u32::from(c.b);
        }
        const ANSI: [u32; 16] = [
            0x171b20, 0xe06c75, 0x98c379, 0xe5c07b, 0x61afef, 0xc678dd, 0x56b6c2, 0xd7dae0,
            0x5c6370, 0xff7a85, 0xb5e890, 0xffdb96, 0x82cfff, 0xe9a0ff, 0x7de0ec, 0xffffff,
        ];
        match index {
            0..=15 => ANSI[index],
            16..=231 => {
                let n = index - 16;
                let channel = |v: usize| if v == 0 { 0 } else { 55 + 40 * v } as u32;
                (channel(n / 36) << 16) | (channel(n / 6 % 6) << 8) | channel(n % 6)
            }
            232..=255 => {
                let v = (8 + 10 * (index - 232)) as u32;
                (v << 16) | (v << 8) | v
            }
            n if n == NamedColor::Background as usize => 0x11151a,
            _ => 0xd7dae0,
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/screen.rs"]
mod tests;
