# Native workspace UI

The default Kea window should read immediately as a developer tool, not as a demo of its internal architecture.

## Visual hierarchy

1. **Terminal** — the largest surface and the actual running application.
2. **Composer** — always available below it and resizable like an editor/terminal split in an IDE.
3. **Compact workspace toolbar** — Terminal, Composer, command-history inspector, copy, Save session, and History.
4. **Status bar** — cwd, transient status, and persistence state.
5. **Replay controls** — visible only while inspecting history.

The user should not need to understand event indices, block counts, PTY ownership, prompt markers, or Kea's recording implementation during ordinary work.

## Native interaction expectations

- Clicking a surface focuses it; keyboard focus has a visible but restrained indication.
- Terminal and Composer remain directly addressable by keyboard.
- The main split is draggable and remembers the conceptual relationship: output above, authored input below.
- Detailed replay controls appear when History is entered, rather than occupying permanent chrome.
- Optional command blocks remain a side inspector, not the primary output model.
- Save Session is visible as a normal workspace action. The status bar says `Temporary`, `Saving · <file>`, or `Saving stopped`.
- The status-bar cwd keeps the explicit contract: `Current shell directory: <path>` only while the integrated shell has reported an idle prompt, otherwise `Shell directory (last reported): <path>`.
- Errors and explanatory text use the status bar instead of adding persistent instruction paragraphs around the work surfaces.

## Alpha acceptance

A developer opening Kea for the first time should be able to identify the terminal, the place to write a multiline command/prompt, how to switch between them, how to save the session, and how to inspect history without reading documentation first.
