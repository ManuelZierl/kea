# Canonical recording safety

AGENTS.md: raw output bytes are canonical and persistence is non-overwriting. Review lossy UTF-8 conversion or overwrite-prone file creation in kea-core recording/format and kea-session observation/journal paths. Display-only kea-document/src/text.rs legitimately uses lossy conversion; scratch files in tests are not recording destinations. Aliases and other write APIs are not covered.
