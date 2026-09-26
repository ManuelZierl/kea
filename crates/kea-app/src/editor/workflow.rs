//! Deterministic, bounded recommendations for explicit composer actions.
//!
//! Recognition never edits text or produces terminal input. Plans keep the exact
//! source and cursor and must be revalidated by the host before acceptance.
use std::ops::Range;

pub const MAX_SECTIONS: usize = 32;
pub const MAX_ACTION_BYTES: usize = 256 * 1024;
pub const MAX_PLACEHOLDERS: usize = 16;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Recommendation {
    SplitSeparator(Range<usize>),
    UnwrapFence,
    SeparateCodeBlocks,
    FillPlaceholder { name: String, count: usize },
    Actions { query: String, range: Range<usize> },
}

impl Recommendation {
    pub fn label(&self) -> String {
        match self {
            Self::SplitSeparator(_) => "Split composer here".into(),
            Self::UnwrapFence => "Remove surrounding code fence".into(),
            Self::SeparateCodeBlocks => "Separate code blocks and prose into drafts".into(),
            Self::FillPlaceholder { name, count } => format!("Fill {name} in {count} places"),
            Self::Actions { .. } => "Open composer actions".into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditPlan {
    pub source: String,
    pub cursor: usize,
    /// Replacement sections, in order. A single section is a native text edit.
    pub sections: Vec<String>,
}

impl EditPlan {
    pub fn is_current(&self, text: &str, cursor: usize) -> bool {
        self.source == text && self.cursor == cursor
    }

    pub fn split(text: &str, cursor: usize) -> Option<Self> {
        if !valid_source(text, cursor) { return None; }
        Some(Self { source: text.into(), cursor,
            sections: vec![text[..cursor].into(), text[cursor..].into()] })
    }

    pub fn split_marker(text: &str, cursor: usize, range: Range<usize>) -> Option<Self> {
        if !valid_source(text, cursor) || range.start > range.end
            || !text.is_char_boundary(range.start) || !text.is_char_boundary(range.end)
            || range.end > text.len() { return None; }
        let before = strip_one_line_ending(&text[..range.start]);
        Some(Self { source: text.into(), cursor,
            sections: vec![before.into(), text[range.end..].into()] })
    }

    pub fn for_recommendation(text: &str, cursor: usize, recommendation: &Recommendation, value: Option<&str>) -> Option<Self> {
        if !valid_source(text, cursor) { return None; }
        let sections = match recommendation {
            Recommendation::SplitSeparator(range) => {
                // A forged/stale range is not an instruction to delete arbitrary text.
                if marker_line(text, cursor)?.1 != *range || marker_line(text, cursor)?.0.trim() != "---" { return None; }
                return Self::split_marker(text, cursor, range.clone());
            }
            Recommendation::UnwrapFence => vec![unwrap_fence(text)?],
            Recommendation::SeparateCodeBlocks => separate_fences(text)?,
            Recommendation::FillPlaceholder { name, .. } => {
                let value = value?;
                if !placeholders(text).iter().any(|(found, _)| found == name) { return None; }
                let ranges = placeholder_ranges(text).into_iter()
                    .filter_map(|(found, range)| (found == name).then_some(range)).collect::<Vec<_>>();
                let removed: usize = ranges.iter().map(|range| range.len()).sum();
                let size = text.len().checked_sub(removed)?.checked_add(ranges.len().checked_mul(value.len())?)?;
                if size > MAX_ACTION_BYTES { return None; }
                let mut replacement = String::with_capacity(size);
                let mut previous = 0;
                for range in ranges {
                    replacement.push_str(&text[previous..range.start]);
                    replacement.push_str(value);
                    previous = range.end;
                }
                replacement.push_str(&text[previous..]);
                vec![replacement]
            }
            Recommendation::Actions { .. } => return None,
        };
        if sections.len() > MAX_SECTIONS { return None; }
        Some(Self { source: text.into(), cursor, sections })
    }
}

pub fn valid_prefix(prefix: &str) -> bool {
    prefix.is_empty() || (prefix.len() <= 16
        && prefix.bytes().all(|b| b.is_ascii_punctuation() && !matches!(b, b'#' | b'=')))
}

pub fn recommend(text: &str, cursor: usize, prefix: &str) -> Vec<Recommendation> {
    if !valid_source(text, cursor) { return Vec::new(); }
    let mut result = Vec::new();
    if let Some((line, range)) = marker_line(text, cursor) {
        if line.trim() == "---" { result.push(Recommendation::SplitSeparator(range.clone())); }
        if !prefix.is_empty() && valid_prefix(prefix) {
            if let Some(query) = line.trim().strip_prefix(prefix) {
                if query.bytes().all(|b| b.is_ascii_alphabetic() || b == b'-') {
                    result.push(Recommendation::Actions { query: query.into(), range });
                }
            }
        }
    }
    if unwrap_fence(text).is_some() { result.push(Recommendation::UnwrapFence); }
    else if separate_fences(text).is_some() { result.push(Recommendation::SeparateCodeBlocks); }
    result.extend(placeholders(text).into_iter().map(|(name, count)| Recommendation::FillPlaceholder { name, count }));
    result
}

fn valid_source(text: &str, cursor: usize) -> bool {
    text.len() <= MAX_ACTION_BYTES && text.is_char_boundary(cursor)
}

/// The entire current logical line, including its trailing line ending.
fn marker_line(text: &str, cursor: usize) -> Option<(&str, Range<usize>)> {
    if !text.is_char_boundary(cursor) { return None; }
    let start = text[..cursor].rfind('\n').map_or(0, |i| i + 1);
    let end = text[cursor..].find('\n').map_or(text.len(), |i| cursor + i + 1);
    Some((&text[start..end], start..end))
}

fn strip_one_line_ending(text: &str) -> &str {
    text.strip_suffix("\r\n").or_else(|| text.strip_suffix('\n')).unwrap_or(text)
}

#[derive(Clone, Copy)]
struct Fence { character: u8, width: usize }

fn opening_fence(line: &str) -> Option<Fence> {
    let line = line.trim();
    let character = *line.as_bytes().first()?;
    if !matches!(character, b'`' | b'~') { return None; }
    let width = line.bytes().take_while(|b| *b == character).count();
    if width < 3 || !line[width..].trim().bytes().all(|b| {
        b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'+' | b'.')
    }) { return None; }
    Some(Fence { character, width })
}

fn closes_fence(line: &str, fence: Fence) -> bool {
    let line = line.trim();
    line.len() >= fence.width && line.bytes().all(|b| b == fence.character)
}

fn fence_ranges(text: &str) -> Option<Vec<(Range<usize>, Range<usize>)>> {
    let mut offset = 0;
    let mut open = None;
    let mut result = Vec::new();
    for line in text.split_inclusive('\n') {
        let end = offset + line.len();
        if let Some((fence, start, body_start)) = open {
            if closes_fence(line, fence) {
                let body_end = body_start + strip_one_line_ending(&text[body_start..offset]).len();
                result.push((start..end, body_start..body_end));
                if result.len() > MAX_SECTIONS { return None; }
                open = None;
            }
        } else if let Some(fence) = opening_fence(line) { open = Some((fence, offset, end)); }
        offset = end;
    }
    if open.is_some() { None } else { Some(result) }
}

fn unwrap_fence(text: &str) -> Option<String> {
    let ranges = fence_ranges(text)?;
    let [(outer, body)] = ranges.as_slice() else { return None };
    if !text[..outer.start].trim().is_empty() || !text[outer.end..].trim().is_empty() { return None; }
    Some(text[body.clone()].into())
}

fn separate_fences(text: &str) -> Option<Vec<String>> {
    let ranges = fence_ranges(text)?;
    if ranges.len() < 2 { return None; }
    let mut sections = Vec::new();
    let mut previous = 0;
    for (outer, body) in ranges {
        if outer.start > previous {
            // Keep prose and whitespace verbatim; never infer that it is disposable.
            sections.push(text[previous..outer.start].into());
        }
        sections.push(text[body].into());
        previous = outer.end;
    }
    if previous < text.len() { sections.push(text[previous..].into()); }
    (sections.len() <= MAX_SECTIONS).then_some(sections)
}

fn placeholder_ranges(text: &str) -> Vec<(&str, Range<usize>)> {
    if text.len() > MAX_ACTION_BYTES { return Vec::new(); }
    let mut found = Vec::new();
    let mut offset = 0;
    while let Some(relative) = text[offset..].find("{{") {
        let start = offset + relative;
        let body = start + 2;
        let Some(relative_end) = text[body..].find("}}") else { break };
        let end = body + relative_end;
        let name = &text[body..end];
        let valid = !name.is_empty() && name.len() <= 64
            && !text[..start].ends_with('{') && !text[end + 2..].starts_with('}')
            && name.bytes().enumerate().all(|(i, b)| {
                b.is_ascii_alphabetic() || b == b'_' || (i > 0 && b.is_ascii_digit())
            });
        if valid { found.push((name, start..end + 2)); }
        offset = end + 2;
    }
    found
}

pub fn placeholders(text: &str) -> Vec<(String, usize)> {
    let mut found: Vec<(String, usize)> = Vec::new();
    for (name, _) in placeholder_ranges(text) {
        if let Some((_, count)) = found.iter_mut().find(|(entry, _)| entry == name) { *count += 1; }
        else if found.len() < MAX_PLACEHOLDERS { found.push((name.into(), 1)); }
    }
    found
}

#[cfg(test)]
#[path = "../../tests/unit/editor/workflow.rs"]
mod tests;
