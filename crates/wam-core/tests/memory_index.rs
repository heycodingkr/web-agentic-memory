use wam_core::{
    CanonicalUrl, KeywordVocabulary, MemoryIndex, MemoryIndexError, MemoryKind, MemoryStatus,
    TaskVocabulary, UrlMode,
};

fn seed() -> (TaskVocabulary, KeywordVocabulary) {
    (
        TaskVocabulary::new("tasks.v1", ["compose-draft"]).unwrap(),
        KeywordVocabulary::new(
            "keywords.v1",
            [
                "draft",
                "editor",
                "save",
                "title",
                "body",
                "paragraph",
                "format",
                "preview",
                "publish",
            ],
        )
        .unwrap(),
    )
}

fn create(
    patterns: Vec<wam_core::UrlPattern>,
    confidence: u8,
    keywords: Vec<wam_core::Keyword>,
) -> Result<MemoryIndex, MemoryIndexError> {
    let (tasks, _) = seed();
    MemoryIndex::new(
        "web:medium-draft",
        "v1",
        patterns,
        tasks.resolve("compose-draft").unwrap(),
        MemoryKind::Workflow,
        "compose-draft",
        keywords,
        confidence,
        MemoryStatus::Active,
        1,
    )
}

#[test]
fn index_rejects_mixed_hosts_and_duplicate_patterns() {
    let medium = CanonicalUrl::parse("https://medium.com/editor", UrlMode::Public).unwrap();
    let naver = CanonicalUrl::parse("https://naver.com/editor", UrlMode::Public).unwrap();
    let words = seed().1.filter(["draft"]).unwrap();

    assert_eq!(
        create(
            vec![medium.exact_pattern(), naver.exact_pattern()],
            1,
            words.clone()
        ),
        Err(MemoryIndexError::MixedHosts)
    );
    assert_eq!(
        create(
            vec![medium.exact_pattern(), medium.exact_pattern()],
            1,
            words
        ),
        Err(MemoryIndexError::DuplicateUrlPattern)
    );
}

#[test]
fn index_enforces_pattern_keyword_and_confidence_limits() {
    let (tasks, vocabulary) = seed();
    let terms = [
        "draft",
        "editor",
        "save",
        "title",
        "body",
        "paragraph",
        "format",
        "preview",
        "publish",
    ];
    let nine_keywords: Vec<_> = terms
        .iter()
        .map(|term| vocabulary.resolve(term).unwrap())
        .collect();
    assert_eq!(
        create(
            vec![
                CanonicalUrl::parse("https://medium.com/editor", UrlMode::Public)
                    .unwrap()
                    .exact_pattern()
            ],
            1,
            nine_keywords
        ),
        Err(MemoryIndexError::TooManyKeywords)
    );

    let patterns = ["one", "two", "three", "four", "five"]
        .iter()
        .map(|path| {
            CanonicalUrl::parse(&format!("https://medium.com/{path}"), UrlMode::Public)
                .unwrap()
                .exact_pattern()
        })
        .collect();
    assert_eq!(
        MemoryIndex::new(
            "web:medium-draft",
            "v1",
            patterns,
            tasks.resolve("compose-draft").unwrap(),
            MemoryKind::Workflow,
            "compose-draft",
            vocabulary.filter(["draft"]).unwrap(),
            1,
            MemoryStatus::Active,
            1,
        ),
        Err(MemoryIndexError::InvalidUrlPatterns)
    );

    assert_eq!(
        create(
            vec![
                CanonicalUrl::parse("https://medium.com/editor", UrlMode::Public)
                    .unwrap()
                    .exact_pattern()
            ],
            3,
            vocabulary.filter(["draft"]).unwrap()
        ),
        Err(MemoryIndexError::InvalidConfidenceBand)
    );
}
