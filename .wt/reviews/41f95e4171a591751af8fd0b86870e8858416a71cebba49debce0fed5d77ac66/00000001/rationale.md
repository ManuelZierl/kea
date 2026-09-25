model.rs:46-56 checks MAX_BLOCK_OUTPUT and document_remaining before extend_from_slice; only the bounded prefix is appended and truncated is set if data is dropped.
