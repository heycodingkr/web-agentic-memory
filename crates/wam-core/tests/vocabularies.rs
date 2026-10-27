use wam_core::{KeywordVocabulary, TaskVocabulary, VocabularyError};

#[test]
fn task_codes_are_closed_and_versioned() {
    let tasks = TaskVocabulary::new("tasks.v1", ["compose-draft", "search"]).unwrap();

    assert_eq!(tasks.version(), "tasks.v1");
    assert_eq!(
        tasks.resolve("compose-draft").unwrap().as_str(),
        "compose-draft"
    );
    assert_eq!(
        tasks.resolve("delete-account"),
        Err(VocabularyError::UnknownCode)
    );
    assert_eq!(
        TaskVocabulary::new("tasks.v1", ["compose-draft", "compose-draft"]),
        Err(VocabularyError::DuplicateCode)
    );
}

#[test]
fn keywords_nfkc_normalize_and_only_allow_approved_values() {
    let keywords = KeywordVocabulary::new("keywords.v1", ["draft", "editor", "save"]).unwrap();

    assert_eq!(keywords.resolve("ＤＲＡＦＴ").unwrap().as_str(), "draft");
    assert_eq!(keywords.resolve("unknown"), None);
    assert_eq!(keywords.resolve("private@example.com"), None);
    assert_eq!(keywords.resolve("https://medium.com"), None);

    let filtered = keywords
        .filter(["ＤＲＡＦＴ", "unknown", "editor", "draft", "secret"])
        .unwrap();
    let values: Vec<_> = filtered.iter().map(|word| word.as_str()).collect();
    assert_eq!(values, vec!["draft", "editor"]);
}

#[test]
fn vocabulary_rejects_free_text_and_more_than_eight_query_terms() {
    assert_eq!(
        KeywordVocabulary::new("keywords.v1", ["write a new article"]),
        Err(VocabularyError::InvalidKeyword)
    );

    let approved = [
        "one", "two", "three", "four", "five", "six", "seven", "eight", "nine",
    ];
    let vocabulary = KeywordVocabulary::new("keywords.v1", approved).unwrap();
    assert_eq!(
        vocabulary.filter([
            "one", "two", "three", "four", "five", "six", "seven", "eight", "nine"
        ]),
        Err(VocabularyError::InvalidKeyword)
    );
}
