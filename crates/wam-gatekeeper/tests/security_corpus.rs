use std::collections::BTreeMap;

use wam_gatekeeper::text::check_text;

const CORPUS: &str =
    include_str!("../../../tests/security/inj-corpus/curator-text-screening.jsonl");

#[test]
fn synthetic_injection_corpus_matches_the_text_screening_contract() {
    let mut count = 0;
    let mut categories = BTreeMap::<String, usize>::new();
    for line in CORPUS.lines() {
        let case: serde_json::Value = serde_json::from_str(line).expect("valid JSONL row");
        let case_id = case["case_id"].as_str().expect("case ID");
        let category = case["category"].as_str().expect("category");
        let text = case["text"].as_str().expect("synthetic malicious text");
        let expected = case["expected_reason"].as_str().expect("expected reason");
        let actual = check_text(text, "medium.com", false).expect_err(case_id);
        assert_eq!(
            actual, expected,
            "screening reason for {case_id} ({category})"
        );
        assert!(
            !actual.contains(text),
            "screening reason must not echo {case_id}"
        );
        *categories.entry(category.to_owned()).or_default() += 1;
        count += 1;
    }

    assert!(count >= 100, "corpus size must meet the security spec");
    assert!(
        categories.len() >= 7,
        "corpus must span distinct attack classes"
    );
}
