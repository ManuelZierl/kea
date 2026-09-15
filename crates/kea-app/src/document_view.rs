use gpui::{prelude::*, *};
use kea_document::{status_label, CommandBlock, CommandStatus, Document};

const MAX_RENDER_LINES: usize = 4_000;

pub fn render_document(document: &Document, scroll: &ScrollHandle) -> Stateful<Div> {
    let mut content = div()
        .id("session-document")
        .size_full()
        .overflow_y_scroll()
        .overflow_x_hidden()
        .track_scroll(scroll)
        .flex()
        .flex_col()
        .gap_3()
        .p_2();

    if document.blocks().is_empty() {
        return content.child(
            div()
                .p_4()
                .text_color(rgb(0x8b949e))
                .child("No commands yet. Write in the editor below; Enter adds a line and the execute shortcut runs the block."),
        );
    }

    for block in document.blocks() {
        content = content.child(render_block(block));
    }
    if document.saturated() {
        content = content.child(
            div()
                .p_2()
                .rounded_md()
                .bg(rgb(0x2b2418))
                .text_color(rgb(0xe5c07b))
                .child("Kea's structured-document retention limit was reached. The terminal itself continues, but later document output may not be retained."),
        );
    }
    content
}

fn render_block(block: &CommandBlock) -> Div {
    let status_color = match block.status {
        CommandStatus::Finished(0) => 0x98c379,
        CommandStatus::Finished(_) | CommandStatus::Aborted => 0xe06c75,
        CommandStatus::Running => 0x61afef,
        CommandStatus::Queued => 0xe5c07b,
    };
    let output = block.plain_output();
    let (visible_output, omitted_lines) = bounded_lines(&output, MAX_RENDER_LINES);

    let command = div()
        .p_2()
        .rounded_t_md()
        .bg(rgb(0x1b222c))
        .border_1()
        .border_color(rgb(0x303844))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_color(rgb(0x61afef))
                        .child(format!("#{}", block.id)),
                )
                .child(
                    div()
                        .text_color(rgb(status_color))
                        .child(status_label(block)),
                ),
        )
        .child(preformatted(&block.input, 0xd7dae0));

    let mut output_panel = div()
        .p_2()
        .rounded_b_md()
        .bg(rgb(0x11151a))
        .border_1()
        .border_color(rgb(0x303844));

    if visible_output.is_empty() {
        if matches!(block.status, CommandStatus::Running | CommandStatus::Queued) {
            output_panel =
                output_panel.child(div().text_color(rgb(0x8b949e)).child("Waiting for output…"));
        }
    } else {
        output_panel = output_panel.child(preformatted(&visible_output, 0xc9d1d9));
    }

    if omitted_lines != 0 {
        output_panel = output_panel.child(
            div()
                .mt_2()
                .text_color(rgb(0xe5c07b))
                .child(format!(
                    "… {omitted_lines} earlier output lines hidden in the view; copy still includes the complete retained block."
                )),
        );
    }
    if block.truncated {
        output_panel = output_panel.child(
            div()
                .mt_2()
                .text_color(rgb(0xe06c75))
                .child("Output block reached Kea's 4 MiB in-memory block limit; later bytes are not retained in this document projection."),
        );
    }

    div().flex().flex_col().child(command).child(output_panel)
}

fn preformatted(text: &str, color: u32) -> Div {
    let mut element = div().flex().flex_col().text_color(rgb(color));
    for line in text.split('\n') {
        element = element.child(div().min_h(px(18.0)).child(if line.is_empty() {
            " ".to_string()
        } else {
            line.to_string()
        }));
    }
    element
}

fn bounded_lines(text: &str, max_lines: usize) -> (String, usize) {
    let lines = text.split('\n').collect::<Vec<_>>();
    if lines.len() <= max_lines {
        return (text.to_string(), 0);
    }
    let omitted = lines.len() - max_lines;
    (lines[omitted..].join("\n"), omitted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_bounding_keeps_latest_output() {
        let (visible, omitted) = bounded_lines("1\n2\n3\n4", 2);
        assert_eq!(visible, "3\n4");
        assert_eq!(omitted, 2);
    }
}
