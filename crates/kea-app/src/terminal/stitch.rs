//! Rebuild text that a full-screen application scrolled through, from
//! successive snapshots of one rectangular screen region.
//!
//! This is reconstruction, not provenance: it only joins snapshots where their
//! overlap is unambiguous. Anything else becomes a visible gap marker rather
//! than a guess. Nothing here reads terminal state or sends input.

/// Inserted where two snapshots could not be aligned.
pub const GAP_MARKER: &str = "⟨gap: could not align⟩";
/// Snapshots kept by one capture; older ones are refused, not silently dropped.
pub const MAX_SNAPSHOTS: usize = 2_000;
/// Overlap needs at least this many matching non-blank rows.
const MIN_ANCHORS: usize = 3;
/// With this many anchors, up to two overlapping rows may differ: one row that
/// changes in place (a spinner, clock or status text inside the captured area)
/// mismatches once in each snapshot. The previously captured row is kept.
const TOLERANT_ANCHORS: usize = 6;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stitched {
    pub text: String,
    pub gaps: usize,
}

/// Join snapshots of the same region, in capture order. Each snapshot is that
/// region's rows (trailing spaces are ignored). Consecutive duplicates are
/// skipped; rows that are identical at the top or bottom of every snapshot are
/// treated as fixed chrome and kept once.
pub fn stitch(snapshots: &[Vec<String>]) -> Stitched {
    let mut frames: Vec<Vec<&str>> = Vec::new();
    for snapshot in snapshots.iter().take(MAX_SNAPSHOTS) {
        let rows: Vec<&str> = snapshot.iter().map(|row| row.trim_end()).collect();
        if frames.last() != Some(&rows) {
            frames.push(rows);
        }
    }
    let Some(first) = frames.first() else {
        return Stitched {
            text: String::new(),
            gaps: 0,
        };
    };
    let height = first.len();
    if frames.len() == 1 || frames.iter().any(|frame| frame.len() != height) {
        // A resized region cannot be aligned row by row; keep the latest view.
        return finish(frames.last().unwrap().clone(), 0);
    }

    // Fixed chrome: rows equal in every snapshot, contiguous from an edge.
    let fixed = |index: usize| frames.iter().all(|frame| frame[index] == first[index]);
    let top = (0..height).take_while(|&i| fixed(i)).count();
    let bottom = (top..height).rev().take_while(|&i| fixed(i)).count();
    fn middle<'a>(frame: &[&'a str], top: usize, end: usize) -> Vec<&'a str> {
        frame[top..end].to_vec()
    }
    let end = height - bottom;

    let mut document: Vec<&str> = middle(first, top, end);
    let mut window = 0usize; // where the previous snapshot starts in `document`
    let mut gaps = 0;
    let mut previous = middle(first, top, end);
    for index in 1..frames.len() {
        let next = middle(&frames[index], top, end);
        let shift = align(&previous, &next);
        // A half-drawn redraw fits neither neighbour; skip it when the snapshot
        // after it lines up with (or returns to) the last accepted one.
        if shift.is_none()
            && frames.get(index + 1).is_some_and(|after| {
                let after = middle(after, top, end);
                after == previous || align(&previous, &after).is_some()
            })
        {
            continue;
        }
        match shift {
            Some(shift) => {
                let start = window as isize + shift;
                if start < 0 {
                    let missing = start.unsigned_abs();
                    document.splice(0..0, next[..missing.min(next.len())].iter().copied());
                    window = 0;
                } else {
                    window = start as usize;
                }
                for (offset, row) in next.iter().enumerate() {
                    let at = window + offset;
                    if at >= document.len() {
                        document.push(row);
                    }
                }
            }
            None => {
                gaps += 1;
                document.push(GAP_MARKER);
                window = document.len();
                document.extend(next.iter().copied());
            }
        }
        previous = next;
    }
    let mut rows: Vec<&str> = first[..top].to_vec();
    rows.extend(document);
    rows.extend(&first[end..]);
    finish(rows, gaps)
}

/// The unique vertical shift that maps `next` onto `previous`: positive when
/// the content moved up (scrolled towards newer text). Overlapping rows must
/// agree, with at least `MIN_ANCHORS` non-blank matches; two differing rows are
/// tolerated from `TOLERANT_ANCHORS` matches. `None` when no shift fits, or when
/// more than one does (for example repeated lines), since either would be a guess.
fn align(previous: &[&str], next: &[&str]) -> Option<isize> {
    let height = previous.len() as isize;
    let consistent = |shift: isize| {
        let (mut anchors, mut mismatches) = (0, 0);
        for row in 0..height {
            let other = row + shift;
            if !(0..height).contains(&other) {
                continue;
            }
            let (a, b) = (next[row as usize], previous[other as usize]);
            if a == b {
                anchors += usize::from(!a.is_empty());
            } else {
                mismatches += 1;
            }
        }
        match mismatches {
            0 => anchors >= MIN_ANCHORS,
            1 | 2 => anchors >= TOLERANT_ANCHORS,
            _ => false,
        }
    };
    let mut fits = ((1 - height)..height).filter(|&shift| shift != 0 && consistent(shift));
    let shift = fits.next()?;
    fits.next().is_none().then_some(shift)
}

fn finish(rows: Vec<&str>, gaps: usize) -> Stitched {
    let start = rows
        .iter()
        .position(|row| !row.is_empty())
        .unwrap_or(rows.len());
    let end = rows
        .iter()
        .rposition(|row| !row.is_empty())
        .map_or(start, |i| i + 1);
    let rows = &rows[start..end];
    // Spaces shared by every line are layout padding of the captured region.
    let indent = rows
        .iter()
        .filter(|row| !row.is_empty() && **row != GAP_MARKER)
        .map(|row| row.len() - row.trim_start_matches(' ').len())
        .min()
        .unwrap_or(0);
    let text = rows
        .iter()
        .map(|row| {
            if *row == GAP_MARKER {
                row
            } else {
                row.get(indent..).unwrap_or("")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    Stitched { text, gaps }
}

/// A rectangle of the visible terminal screen, in cells.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Region {
    pub rows: std::ops::Range<usize>,
    pub columns: std::ops::Range<usize>,
}

/// The bounding rectangle of the visible selection. A block selection gives
/// exactly its columns; a line selection spanning rows covers full rows.
pub fn selected_region(screen: &kea_alacritty::Screen) -> Option<Region> {
    let width = usize::from(screen.size.columns).max(1);
    let mut bounds: Option<(usize, usize, usize, usize)> = None;
    for (index, cell) in screen.cells.iter().enumerate() {
        if cell.selected {
            let (row, column) = (index / width, index % width);
            let (top, bottom, left, right) = bounds.unwrap_or((row, row, column, column));
            bounds = Some((
                top.min(row),
                bottom.max(row),
                left.min(column),
                right.max(column),
            ));
        }
    }
    bounds.map(|(top, bottom, left, right)| Region {
        rows: top..bottom + 1,
        columns: left..right + 1,
    })
}

/// The text of `region` on `screen`, one string per row. `None` when the region
/// no longer fits the screen (for example after a resize).
pub fn region_rows(screen: &kea_alacritty::Screen, region: &Region) -> Option<Vec<String>> {
    let width = usize::from(screen.size.columns);
    let height = usize::from(screen.size.rows);
    if region.rows.end > height || region.columns.end > width {
        return None;
    }
    Some(
        region
            .rows
            .clone()
            .map(|row| {
                // A region starting on the right half of a wide character keeps
                // that character instead of silently dropping it.
                let mut start = row * width + region.columns.start;
                if region.columns.start > 0 && screen.cells[start].spacer {
                    start -= 1;
                }
                screen.cells[start..row * width + region.columns.end]
                    .iter()
                    .filter(|cell| !cell.spacer)
                    .map(|cell| cell.text.as_str())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect(),
    )
}

#[cfg(test)]
#[path = "../../tests/unit/terminal/stitch.rs"]
mod tests;
