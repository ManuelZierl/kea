use super::*;

#[test]
fn recognizing_typed_or_pasted_text_is_pure() {
    let text = String::from("echo first\n---\necho next");
    let original = text.clone();
    let suggestions = recommend(&text, 13, "::");
    assert!(matches!(suggestions[0], Recommendation::SplitSeparator(_)));
    assert_eq!(text, original);
    let plan = EditPlan::for_recommendation(&text, 13, &suggestions[0], None).unwrap();
    assert_eq!(plan.sections, ["echo first", "echo next"]);
    assert!(plan.is_current(&text, 13));
    assert!(!plan.is_current(&(text.clone() + "!"), 13));
    assert!(!plan.is_current(&text, 0));
}

#[test]
fn only_a_standalone_current_line_is_a_separator() {
    for text in ["echo ---", "----", "x---", "---x", "echo\n---\nnext"] {
        assert!(!recommend(text, text.len(), "::")
            .iter()
            .any(|s| matches!(s, Recommendation::SplitSeparator(_))));
    }
    assert!(matches!(
        recommend("  ---  ", 5, "::")[0],
        Recommendation::SplitSeparator(_)
    ));
}

#[test]
fn split_preserves_unicode_and_conditional_lines() {
    let text = "echo 🦜\nvalidate &&\nrestart";
    let cursor = "echo 🦜\n".len();
    let plan = EditPlan::split(text, cursor).unwrap();
    assert_eq!(plan.sections.concat(), text);
    assert_eq!(plan.sections[1], "validate &&\nrestart");
    assert!(EditPlan::split(text, 7).is_none());
    assert!(EditPlan::split(text, text.len() + 1).is_none());
}

#[test]
fn crlf_separator_removes_only_its_boundary() {
    let text = "a\r\n---\r\nb";
    let suggestion = recommend(text, 5, "::").remove(0);
    assert_eq!(
        EditPlan::for_recommendation(text, 5, &suggestion, None)
            .unwrap()
            .sections,
        ["a", "b"]
    );
}

#[test]
fn enclosing_fences_are_explicit_transformations() {
    let text = "```bash\necho 'literal ---'\n```\n";
    let suggestion = Recommendation::UnwrapFence;
    let plan = EditPlan::for_recommendation(text, text.len(), &suggestion, None).unwrap();
    assert_eq!(plan.sections, ["echo 'literal ---'"]);
    for invalid in ["```sh\necho x", "prose\n```sh\nx\n```", "```sh\nx\n~~~"] {
        assert!(EditPlan::for_recommendation(invalid, 0, &suggestion, None).is_none());
    }
    assert_eq!(unwrap_fence("~~~~sh\n```\n~~~~"), Some("```".into()));
}

#[test]
fn multiple_blocks_preserve_all_prose() {
    let text = "Before\n```sh\na\n```\nBetween\n```sh\nb\n```\nAfter";
    let plan =
        EditPlan::for_recommendation(text, 0, &Recommendation::SeparateCodeBlocks, None).unwrap();
    assert_eq!(plan.sections, ["Before\n", "a", "Between\n", "b", "After"]);
}

#[test]
fn placeholders_are_literal_and_bounded() {
    let text = "ssh {{host}}; scp x {{host}}:/tmp; echo {{other}}";
    let recommendation = Recommendation::FillPlaceholder {
        name: "host".into(),
        count: 2,
    };
    assert_eq!(
        placeholders(text),
        [("host".into(), 2), ("other".into(), 1)]
    );
    let plan = EditPlan::for_recommendation(text, 0, &recommendation, Some("$HOME")).unwrap();
    assert_eq!(
        plan.sections[0],
        "ssh $HOME; scp x $HOME:/tmp; echo {{other}}"
    );
    assert!(EditPlan::for_recommendation(text, 0, &recommendation, None).is_none());
    assert!(EditPlan::for_recommendation(
        text,
        0,
        &recommendation,
        Some(&"x".repeat(MAX_ACTION_BYTES))
    )
    .is_none());
}

#[test]
fn shorthand_is_a_recommendation_not_a_directive() {
    assert!(matches!(
        recommend("::split", 7, "::")[0],
        Recommendation::Actions { .. }
    ));
    assert!(recommend("echo ::split", 12, "::").is_empty());
    assert!(recommend("::split", 7, "").is_empty());
    assert!(valid_prefix("::"));
    assert!(valid_prefix(""));
    assert!(!valid_prefix("#"));
    assert!(!valid_prefix("a b"));
}

#[test]
fn analysis_and_section_counts_are_bounded() {
    assert!(recommend(&"x".repeat(MAX_ACTION_BYTES + 1), 0, "::").is_empty());
    let many = "```\nx\n```\n".repeat(MAX_SECTIONS + 1);
    assert!(separate_fences(&many).is_none());
}

#[test]
fn placeholder_counts_and_replacements_use_identical_boundaries() {
    let text = "{{host}} {{{host}}} {{host}}}";
    assert_eq!(placeholders(text), [("host".into(), 1)]);
    let rec = Recommendation::FillPlaceholder {
        name: "host".into(),
        count: 1,
    };
    let plan = EditPlan::for_recommendation(text, 0, &rec, Some("server")).unwrap();
    assert_eq!(plan.sections, ["server {{{host}}} {{host}}}"]);
}
