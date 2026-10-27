use wam_core::{
    CanonicalUrl, KeywordVocabulary, MemoryIndex, MemoryKind, MemoryStatus, RecallDocument,
    RecallQuery, RouteCoverage, TaskVocabulary, UrlMode, recall, recall_documents,
};

fn fixtures() -> (TaskVocabulary, KeywordVocabulary) {
    (
        TaskVocabulary::new("tasks.v1", ["compose-draft", "search"]).unwrap(),
        KeywordVocabulary::new("keywords.v1", ["draft", "editor", "save", "compose"]).unwrap(),
    )
}

#[allow(clippy::too_many_arguments)]
fn memory(
    id: &str,
    url: &str,
    coverage: RouteCoverage,
    task: &str,
    status: MemoryStatus,
    confidence: u8,
    verified_at: u64,
    task_vocab: &TaskVocabulary,
    keyword_vocab: &KeywordVocabulary,
) -> MemoryIndex {
    let canonical = CanonicalUrl::parse(url, UrlMode::Public).unwrap();
    let pattern = match coverage {
        RouteCoverage::Exact => canonical.exact_pattern(),
        RouteCoverage::Subtree => canonical.subtree_pattern(),
        RouteCoverage::Host => canonical.host_pattern(),
    };
    MemoryIndex::new(
        id,
        "v1",
        vec![pattern],
        task_vocab.resolve(task).unwrap(),
        MemoryKind::Workflow,
        task,
        keyword_vocab.filter(["draft", "compose"]).unwrap(),
        confidence,
        status,
        verified_at,
    )
    .unwrap()
}

#[test]
fn recall_orders_route_task_keyword_and_confidence_without_cross_host_leakage() {
    let (tasks, words) = fixtures();
    let memories = vec![
        memory(
            "web:exact",
            "https://medium.com/editor/post/123/comments",
            RouteCoverage::Exact,
            "compose-draft",
            MemoryStatus::Active,
            1,
            10,
            &tasks,
            &words,
        ),
        memory(
            "web:parent",
            "https://medium.com/editor/post/123",
            RouteCoverage::Subtree,
            "compose-draft",
            MemoryStatus::Active,
            2,
            20,
            &tasks,
            &words,
        ),
        memory(
            "web:host",
            "https://medium.com/",
            RouteCoverage::Host,
            "compose-draft",
            MemoryStatus::Active,
            2,
            30,
            &tasks,
            &words,
        ),
        memory(
            "web:wrong-task",
            "https://medium.com/editor/post/123/comments",
            RouteCoverage::Exact,
            "search",
            MemoryStatus::Active,
            2,
            40,
            &tasks,
            &words,
        ),
        memory(
            "web:retired",
            "https://medium.com/editor/post/123/comments",
            RouteCoverage::Exact,
            "compose-draft",
            MemoryStatus::Retired,
            2,
            50,
            &tasks,
            &words,
        ),
        memory(
            "web:other-host",
            "https://naver.com/editor/post/123/comments",
            RouteCoverage::Host,
            "compose-draft",
            MemoryStatus::Active,
            2,
            60,
            &tasks,
            &words,
        ),
    ];

    let url = CanonicalUrl::parse(
        "https://medium.com/editor/post/999/comments",
        UrlMode::Public,
    )
    .unwrap();
    let query = RecallQuery::new(
        url,
        Some(tasks.resolve("compose-draft").unwrap()),
        Some(MemoryKind::Workflow),
        words.filter(["ＤＲＡＦＴ"]).unwrap(),
    )
    .unwrap();

    let hits = recall(&memories, &query);
    let ids: Vec<_> = hits.iter().map(|hit| hit.memory.id()).collect();

    assert_eq!(ids, vec!["web:exact", "web:parent", "web:host"]);
    assert_eq!(hits[0].matched_on.route as u8, 3);
    assert!(hits[0].matched_on.task);
    assert_eq!(hits[0].matched_on.keyword_matches, 1);
}

#[test]
fn fresh_memories_beat_needs_reverify_on_equal_score_then_id_breaks_ties() {
    let (tasks, words) = fixtures();
    let url = "https://medium.com/editor/post/123";
    let memories = vec![
        memory(
            "web:z-active",
            url,
            RouteCoverage::Exact,
            "compose-draft",
            MemoryStatus::Active,
            1,
            10,
            &tasks,
            &words,
        ),
        memory(
            "web:a-stale",
            url,
            RouteCoverage::Exact,
            "compose-draft",
            MemoryStatus::NeedsReverify,
            1,
            100,
            &tasks,
            &words,
        ),
        memory(
            "web:b-active",
            url,
            RouteCoverage::Exact,
            "compose-draft",
            MemoryStatus::Active,
            1,
            10,
            &tasks,
            &words,
        ),
    ];
    let query = RecallQuery::new(
        CanonicalUrl::parse(url, UrlMode::Public).unwrap(),
        Some(tasks.resolve("compose-draft").unwrap()),
        None,
        words.filter(["draft"]).unwrap(),
    )
    .unwrap();

    let hits = recall(&memories, &query);
    let ids: Vec<_> = hits.iter().map(|hit| hit.memory.id()).collect();
    assert_eq!(ids, vec!["web:b-active", "web:z-active", "web:a-stale"]);
}

#[test]
fn no_task_query_uses_keyword_matches_and_result_limit_is_bounded() {
    let (tasks, words) = fixtures();
    let memories = vec![
        memory(
            "web:compose",
            "https://medium.com/editor/post/123",
            RouteCoverage::Exact,
            "compose-draft",
            MemoryStatus::Active,
            1,
            10,
            &tasks,
            &words,
        ),
        memory(
            "web:search",
            "https://medium.com/editor/post/123",
            RouteCoverage::Exact,
            "search",
            MemoryStatus::Active,
            1,
            10,
            &tasks,
            &words,
        ),
    ];
    let query = RecallQuery::new(
        CanonicalUrl::parse("https://medium.com/editor/post/123", UrlMode::Public).unwrap(),
        None,
        None,
        words.filter(["draft"]).unwrap(),
    )
    .unwrap()
    .with_limit(1)
    .unwrap();

    let hits = recall(&memories, &query);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].memory.id(), "web:compose");
    assert!(!hits[0].matched_on.task);
}

#[test]
fn fielded_bm25_and_rrf_promote_relevant_metadata_without_widening_url_scope() {
    let (tasks, words) = fixtures();
    let exact_url = CanonicalUrl::parse("https://example.com/editor/post/7", UrlMode::Public)
        .unwrap()
        .exact_pattern();
    let other_host = CanonicalUrl::parse("https://elsewhere.com/editor/post/7", UrlMode::Public)
        .unwrap()
        .exact_pattern();
    let keywords = words.filter(["draft"]).unwrap();
    let make_memory = |id: &str, pattern| {
        MemoryIndex::new(
            id,
            "v1",
            vec![pattern],
            tasks.resolve("compose-draft").unwrap(),
            MemoryKind::Workflow,
            "create-draft",
            keywords.clone(),
            1,
            MemoryStatus::Active,
            10,
        )
        .unwrap()
    };
    let memories = [
        make_memory("web:a-general", exact_url.clone()),
        make_memory("web:z-draft", exact_url),
        make_memory("web:other-host", other_host),
    ];
    let documents = [
        RecallDocument::new(
            &memories[0],
            "Editor guide",
            "A general page workflow",
            "Example",
            "example.com",
        ),
        RecallDocument::new(
            &memories[1],
            "Draft editor workflow",
            "Save a draft in the editor",
            "Example",
            "example.com",
        ),
        RecallDocument::new(
            &memories[2],
            "Draft editor workflow",
            "Save a draft in the editor",
            "Elsewhere",
            "elsewhere.com",
        ),
    ];
    let query = RecallQuery::new(
        CanonicalUrl::parse("https://example.com/editor/post/7", UrlMode::Public).unwrap(),
        Some(tasks.resolve("compose-draft").unwrap()),
        None,
        keywords,
    )
    .unwrap();

    let hits = recall_documents(&documents, &query);
    let ids: Vec<_> = hits.iter().map(|hit| hit.memory.id()).collect();
    assert_eq!(ids, vec!["web:z-draft", "web:a-general"]);
    assert_eq!(hits[0].matched_on.text_matches, 1);

    let reversed: Vec<_> = documents.iter().rev().copied().collect();
    let reversed_ids: Vec<_> = recall_documents(&reversed, &query)
        .iter()
        .map(|hit| hit.memory.id())
        .collect();
    assert_eq!(reversed_ids, ids);

    let scores: Vec<_> = hits.iter().map(|hit| hit.score).collect();
    let reversed_scores: Vec<_> = recall_documents(&reversed, &query)
        .iter()
        .map(|hit| hit.score)
        .collect();
    assert_eq!(reversed_scores, scores);
}
