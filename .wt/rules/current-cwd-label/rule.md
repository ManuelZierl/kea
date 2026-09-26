# Current shell directory label

AGENTS.md: current shell directory only while the integrated local shell has explicitly reported idle/ready; otherwise last reported. Review each rendered label. The current branch in rendering.rs uses input_context.shell_ready(), which also accepts remote/nested shell contexts. A remote ready marker may therefore label a retained earlier cwd current; review the exact context ownership. Docs prose is not a UI label.
