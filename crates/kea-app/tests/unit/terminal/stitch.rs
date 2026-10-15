use super::*;

fn transcript(lines: usize) -> Vec<String> {
    (1..=lines)
        .map(|i| format!("message {i}: unique text ä{}", "·".repeat(i % 5)))
        .collect()
}

/// What a full-screen app shows: fixed chrome around a window into `content`.
fn frame(content: &[String], top: usize, height: usize) -> Vec<String> {
    let mut rows = vec!["┌ header ┐".to_owned()];
    rows.extend((top..top + height).map(|i| content.get(i).cloned().unwrap_or_default() + "   "));
    rows.push("└ input box ┘".to_owned());
    rows
}

fn expected(content: &[String], range: std::ops::Range<usize>) -> String {
    let mut rows = vec!["┌ header ┐".to_owned()];
    rows.extend(content[range].iter().cloned());
    rows.push("└ input box ┘".to_owned());
    rows.join("\n")
}

#[test]
fn scrolling_down_and_up_rebuilds_the_exact_transcript() {
    let content = transcript(60);
    let height = 8;
    let down: Vec<_> = (0..=52)
        .step_by(3)
        .map(|top| frame(&content, top, height))
        .collect();
    let stitched = stitch(&down);
    assert_eq!(stitched.gaps, 0);
    assert_eq!(stitched.text, expected(&content, 0..51 + height));

    // Starting at the newest text and scrolling back towards older messages.
    let up: Vec<_> = (0..=10)
        .rev()
        .map(|step| frame(&content, step * 5, height))
        .collect();
    let stitched = stitch(&up);
    assert_eq!(stitched.gaps, 0);
    assert_eq!(stitched.text, expected(&content, 0..50 + height));
}

#[test]
fn back_and_forth_scrolling_does_not_duplicate_rows() {
    let content = transcript(30);
    let tops = [0, 4, 8, 4, 2, 6, 11, 9];
    let frames: Vec<_> = tops.iter().map(|&top| frame(&content, top, 8)).collect();
    let stitched = stitch(&frames);
    assert_eq!(stitched.gaps, 0);
    assert_eq!(stitched.text, expected(&content, 0..19));

    // Two overlapping rows are too few anchors to trust the join.
    let short = stitch(&[frame(&content, 0, 8), frame(&content, 6, 8)]);
    assert_eq!(short.gaps, 1);
}

#[test]
fn a_jump_without_overlap_is_a_visible_gap() {
    let content = transcript(60);
    let frames = vec![
        frame(&content, 0, 8),
        frame(&content, 3, 8),
        frame(&content, 30, 8),
        frame(&content, 32, 8),
    ];
    let stitched = stitch(&frames);
    assert_eq!(stitched.gaps, 1);
    let text = stitched.text;
    let before = text.split(GAP_MARKER).next().unwrap();
    assert!(before.contains("message 11:") && !before.contains("message 31:"));
    let after = text.split(GAP_MARKER).nth(1).unwrap();
    assert!(after.contains("message 31:") && after.contains("message 40:"));
}

#[test]
fn repeated_or_changing_rows_are_gaps_not_guesses() {
    // Identical lines fit several shifts, so the scroll distance is unknown.
    let repeated: Vec<String> = (0..40).map(|_| "same line".to_owned()).collect();
    let stitched = stitch(&[frame(&repeated, 0, 8), frame(&repeated, 2, 8)]);
    // Equal frames are deduplicated; nothing to join and nothing invented.
    assert_eq!(stitched.gaps, 0);
    assert_eq!(stitched.text.matches("same line").count(), 8);

    let mut periodic: Vec<String> = Vec::new();
    for i in 0..40 {
        periodic.push(["alpha", "beta"][i % 2].to_owned());
    }
    let stitched = stitch(&[frame(&periodic, 0, 8), frame(&periodic, 1, 8)]);
    assert_eq!(stitched.gaps, 1);

    // A spinner fixed on screen inside the captured area mismatches in both
    // snapshots; with enough anchors that is tolerated and the first capture of
    // the row is kept.
    let content = transcript(40);
    let mut first = frame(&content, 0, 12);
    let mut second = frame(&content, 2, 12);
    first[5] = "⠋ thinking".into();
    second[5] = "⠙ thinking".into();
    let stitched = stitch(&[first.clone(), second.clone()]);
    assert_eq!(stitched.gaps, 0);
    assert!(stitched.text.contains("message 14:"));
    assert!(!stitched.text.contains("⠙"));
    // A second changing row is too much to trust the join.
    first[8] = "12:00:01".into();
    second[8] = "12:00:02".into();
    assert_eq!(stitch(&[first, second]).gaps, 1);
}

#[test]
fn a_torn_frame_between_identical_frames_is_skipped() {
    let content = transcript(30);
    let mut torn = frame(&content, 0, 8);
    torn[3] = "partially redrawn".into();
    torn[4] = "garbage".into();
    let frames = vec![
        frame(&content, 0, 8),
        torn,
        frame(&content, 0, 8),
        frame(&content, 3, 8),
    ];
    let stitched = stitch(&frames);
    assert_eq!(stitched.gaps, 0);
    assert_eq!(stitched.text, expected(&content, 0..11));
}

#[test]
fn trivial_inputs_are_returned_without_invention() {
    assert_eq!(
        stitch(&[]),
        Stitched {
            text: String::new(),
            gaps: 0
        }
    );
    let content = transcript(10);
    let one = frame(&content, 0, 4);
    let stitched = stitch(&[one.clone(), one.clone()]);
    assert_eq!(stitched.gaps, 0);
    assert_eq!(stitched.text, expected(&content, 0..4));
    // A resized region keeps only its latest snapshot.
    let resized = stitch(&[one, frame(&content, 0, 6)]);
    assert_eq!(resized.text, expected(&content, 0..6));
}

fn screen(rows: &[&str], selected: impl Fn(usize, usize) -> bool) -> kea_alacritty::Screen {
    let columns = rows
        .iter()
        .map(|row| row.chars().count())
        .max()
        .unwrap_or(0);
    let mut cells = Vec::new();
    for (r, row) in rows.iter().enumerate() {
        let chars: Vec<char> = row.chars().collect();
        for c in 0..columns {
            cells.push(kea_alacritty::Cell {
                text: chars.get(c).map_or(" ".into(), |ch| ch.to_string()),
                foreground: 0,
                background: 0,
                bold: false,
                italic: false,
                underline: false,
                wide: false,
                spacer: false,
                selected: selected(r, c),
            });
        }
    }
    kea_alacritty::Screen {
        size: kea_core::Size {
            columns: columns as u16,
            rows: rows.len() as u16,
        },
        cells,
        cursor: None,
        display_offset: 0,
        history_size: 0,
        local_caret: None,
        selection_invalidated: false,
    }
}

#[test]
fn a_region_starting_inside_a_wide_character_keeps_it() {
    let mut screen = screen(&["ab日x", "cd本y"], |_, c| c >= 3);
    // Mark the cell after each wide glyph as its spacer, like the emulator does.
    for row in 0..2 {
        screen.cells[row * 4 + 2].wide = true;
        screen.cells[row * 4 + 3].text = String::new();
        screen.cells[row * 4 + 3].spacer = true;
    }
    let region = Region {
        rows: 0..2,
        columns: 3..4,
    };
    assert_eq!(region_rows(&screen, &region).unwrap(), ["日", "本"]);
}

#[test]
fn a_block_selection_defines_the_captured_rectangle() {
    let rows = [
        "side │ one  ",
        "bar  │ two  ",
        "     │ three",
        "status line ",
    ];
    // Block-select the message column on the first three rows.
    let screen = screen(&rows, |r, c| r < 3 && (7..12).contains(&c));
    let region = selected_region(&screen).unwrap();
    assert_eq!(
        region,
        Region {
            rows: 0..3,
            columns: 7..12
        }
    );
    assert_eq!(
        region_rows(&screen, &region).unwrap(),
        ["one", "two", "three"]
    );
    assert!(selected_region(&super::tests::screen(&rows, |_, _| false)).is_none());
    let outside = Region {
        rows: 2..5,
        columns: 0..3,
    };
    assert!(region_rows(&screen, &outside).is_none());
}

#[test]
fn a_half_drawn_snapshot_between_good_ones_is_skipped() {
    let content = transcript(40);
    let mut torn = frame(&content, 3, 8);
    // Top half already shows the new position, bottom half still the old one.
    let old = frame(&content, 0, 8);
    torn[5..].clone_from_slice(&old[5..]);
    let frames = vec![
        frame(&content, 0, 8),
        torn,
        frame(&content, 3, 8),
        frame(&content, 6, 8),
    ];
    let stitched = stitch(&frames);
    assert_eq!(stitched.gaps, 0);
    assert_eq!(stitched.text, expected(&content, 0..14));
}

#[test]
fn padding_shared_by_every_line_is_removed() {
    let rows =
        |top: usize| -> Vec<String> { (top..top + 5).map(|i| format!("   message {i}")).collect() };
    let mut first = rows(0);
    first[1] = "     indented reply".into();
    let stitched = stitch(&[first, rows(5)]);
    // No overlap: a gap, and indentation relative to the rest is preserved.
    assert_eq!(stitched.gaps, 1);
    assert!(stitched
        .text
        .starts_with("message 0\n  indented reply\nmessage 2"));
    assert!(stitched
        .text
        .contains(&format!("\n{GAP_MARKER}\nmessage 5")));
}
