from pathlib import Path
p=Path('crates/kea-app/src/main.rs');s=p.read_text()
a='''        let result = input::paste(&text, self.session.bracketed_paste())
            .and_then(|bytes| self.session.send(bytes));
        self.result(result, cx);'''
b='''        let result = input::paste(&text, self.session.bracketed_paste())
            .and_then(|bytes| self.session.send(bytes));
        if result.is_ok() { self.document.note_terminal_input(); }
        self.result(result, cx);'''
assert a in s;s=s.replace(a,b,1)
a='''        if self.completion_rx.is_some() {'''
b='''        if !self.candidates.is_empty() && text == self.completion_text && cursor == self.completion_cursor {
            self.apply_completion(0, window, cx);
            return;
        }
        if self.completion_rx.is_some() {'''
assert a in s;s=s.replace(a,b,1).replace('self.candidates.iter().take(8).enumerate()','self.candidates.iter().enumerate()')
s=s.replace('"Choose a completion below. Nothing is executed."','"Choose a completion below, or press Tab again to accept the first. Nothing is executed."')
s=s.replace('Return to LIVE before executing a document command.','Return to LIVE before sending input.').replace('Document execution is unavailable for this program.','Run in shell requires an integrated shell; use Send to app instead.')
a='''                        div()
                            .flex_1()
                            .overflow_y_scroll()
                            .id("completion-list")'''
b='''                        div()
                            .id("completion-list")
                            .flex_1()
                            .overflow_y_scroll()'''
assert a in s;s=s.replace(a,b,1)
p.write_text(s)
p=Path('crates/kea-app/src/document_view.rs');s=p.read_text()
s=s.replace('                    self.document_scroll.scroll_to_bottom();','''                    if self.document_ui.page_start.is_none() {
                        self.document_scroll.scroll_to_bottom();
                    }''',1)
p.write_text(s)
p=Path('docs/editor-integration.md');s=p.read_text()
s='\n'.join(line for line in s.split('\n') if not line.startswith('> **Current interaction:**'))
s=s.replace('Execute applies only to the focused command editor, not a block, filter, embedded find field or Direct PTY.', 'Run in shell and Send to app apply only to the focused command editor, not a block, filter, embedded find field or terminal. Run in shell additionally requires an explicit prompt-ready report; Send to app never wraps its text in shell source.')
s=s.replace('Direct PTY has its own focus/key context and retains the same process. Editor-only controls such as Undo, Execute and Focus editor must not steal control sequences there.', 'The simultaneously visible terminal has its own focus/key context and retains the same process. No Kea action reserves a live-terminal shortcut. Toolbar buttons remain available without stealing control sequences. The optional block inspector changes presentation only; see unified-session.md.')
s=s.replace('theme = system\n','theme = system\nshow_blocks = false\n')
p.write_text(s)
