# Keep test assertions

AGENTS.md: do not disable tests to obtain a green build; preserve real assertions. Detect reasonless ignore attributes, literal assert!(true), and line-commented assert macros in crate tests and source-hosted tests. A reasoned #[ignore = "..."] and active assert_eq! are valid. Comments quoting disabled code for documentation can need review.
