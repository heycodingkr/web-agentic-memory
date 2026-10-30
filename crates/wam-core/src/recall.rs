//! Deterministic, side-effect-free recall candidate selection and ranking.

use crate::{
    keyword::{Keyword, TaskCategory},
    memory::{MemoryIndex, MemoryKind},
    text_search::{SearchDocument, bm25_scores, normalize_terms},
    url_scope::{CanonicalUrl, RouteMatchLevel},
};
use std::collections::BTreeSet;

const MAX_RESULTS: usize = 3;
const MAX_KEYWORDS: usize = 8;
const RRF_K: f64 = 60.0;
const RRF_SCALE: f64 = 1_000_000.0;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecallQueryError {
    InvalidLimit,
    TooManyKeywords,
    DuplicateKeyword,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecallQuery {
    url: CanonicalUrl,
    task_category: Option<TaskCategory>,
    kind: Option<MemoryKind>,
    keywords: Vec<Keyword>,
    limit: usize,
}

impl RecallQuery {
    pub fn new(
        url: CanonicalUrl,
        task_category: Option<TaskCategory>,
        kind: Option<MemoryKind>,
        keywords: Vec<Keyword>,
    ) -> Result<Self, RecallQueryError> {
        if keywords.len() > MAX_KEYWORDS {
            return Err(RecallQueryError::TooManyKeywords);
        }
        let unique: BTreeSet<_> = keywords.iter().collect();
        if unique.len() != keywords.len() {
            return Err(RecallQueryError::DuplicateKeyword);
        }
        Ok(Self {
            url,
            task_category,
            kind,
            keywords,
            limit: MAX_RESULTS,
        })
    }

    pub fn with_limit(mut self, limit: usize) -> Result<Self, RecallQueryError> {
        if !(1..=MAX_RESULTS).contains(&limit) {
            return Err(RecallQueryError::InvalidLimit);
        }
        self.limit = limit;
        Ok(self)
    }

    pub fn limit(&self) -> usize {
        self.limit
    }

    pub fn url(&self) -> &CanonicalUrl {
        &self.url
    }

    pub fn task_category(&self) -> Option<&TaskCategory> {
        self.task_category.as_ref()
    }

    pub fn kind(&self) -> Option<MemoryKind> {
        self.kind
    }

    pub fn keywords(&self) -> &[Keyword] {
        &self.keywords
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MatchDetails {
    pub route: RouteMatchLevel,
    pub task: bool,
    pub keyword_matches: u8,
    pub text_matches: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecallHit<'a> {
    pub memory: &'a MemoryIndex,
    pub matched_on: MatchDetails,
    pub score: u16,
}

#[derive(Clone, Copy, Debug)]
pub struct RecallDocument<'a> {
    pub memory: &'a MemoryIndex,
    pub title: &'a str,
    pub summary: &'a str,
    pub site: &'a str,
    pub domain: &'a str,
}

impl<'a> RecallDocument<'a> {
    pub fn new(
        memory: &'a MemoryIndex,
        title: &'a str,
        summary: &'a str,
        site: &'a str,
        domain: &'a str,
    ) -> Self {
        Self {
            memory,
            title,
            summary,
            site,
            domain,
        }
    }
}

pub fn recall<'a>(memories: &'a [MemoryIndex], query: &RecallQuery) -> Vec<RecallHit<'a>> {
    let documents: Vec<_> = memories
        .iter()
        .map(|memory| RecallDocument::new(memory, "", "", "", ""))
        .collect();
    recall_documents(&documents, query)
}

pub fn recall_documents<'a>(
    documents: &[RecallDocument<'a>],
    query: &RecallQuery,
) -> Vec<RecallHit<'a>> {
    let query_keywords: BTreeSet<_> = query.keywords.iter().collect();
    let mut candidates = Vec::new();

    for document in documents {
        let memory = document.memory;
        if !memory.status().is_recallable() {
            continue;
        }
        if query.kind.is_some_and(|kind| kind != memory.kind()) {
            continue;
        }
        if query
            .task_category
            .as_ref()
            .is_some_and(|task| task != memory.task_category())
        {
            continue;
        }

        let Some(route) = memory
            .url_patterns()
            .iter()
            .filter_map(|pattern| pattern.match_level(&query.url))
            .max_by_key(|level| *level as u8)
        else {
            continue;
        };

        let keyword_matches = memory
            .keywords()
            .iter()
            .filter(|keyword| query_keywords.contains(keyword))
            .count() as u8;
        let task_match = query.task_category.is_some();
        let score = 1000 * route as u16
            + 100 * u16::from(task_match)
            + 10 * u16::from(keyword_matches)
            + u16::from(memory.confidence_band());

        candidates.push(Candidate {
            document: *document,
            route,
            task_match,
            keyword_matches,
            base_score: score,
            text_score: 0.0,
            text_matches: 0,
            fusion_score: 0.0,
            score,
        });
    }

    let mut structural_order: Vec<usize> = (0..candidates.len()).collect();
    structural_order
        .sort_by(|left, right| compare_structural(&candidates[*left], &candidates[*right]));

    let text_terms = normalize_terms(query.keywords.iter().map(Keyword::as_str));
    if !text_terms.is_empty() {
        let keyword_texts: Vec<String> = candidates
            .iter()
            .map(|candidate| {
                candidate
                    .document
                    .memory
                    .keywords()
                    .iter()
                    .map(Keyword::as_str)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect();
        let search_documents: Vec<_> = candidates
            .iter()
            .zip(&keyword_texts)
            .map(|(candidate, keyword_text)| SearchDocument {
                fields: [
                    candidate.document.title,
                    candidate.document.memory.intent_key(),
                    keyword_text,
                    candidate.document.memory.task_category().as_str(),
                    candidate.document.summary,
                    candidate.document.site,
                    candidate.document.domain,
                    memory_kind_name(candidate.document.memory.kind()),
                ],
            })
            .collect();
        let text_scores = bm25_scores(&search_documents, &text_terms);
        for (candidate, (text_score, text_matches)) in candidates.iter_mut().zip(text_scores) {
            candidate.text_score = text_score;
            candidate.text_matches = text_matches;
        }

        let mut text_order: Vec<usize> = candidates
            .iter()
            .enumerate()
            .filter_map(|(index, candidate)| (candidate.text_score > 0.0).then_some(index))
            .collect();
        text_order.sort_by(|left, right| {
            candidates[*right]
                .text_score
                .total_cmp(&candidates[*left].text_score)
                .then_with(|| compare_structural(&candidates[*left], &candidates[*right]))
        });

        // RRF combines rankings without pretending their raw score scales match.
        for (rank, index) in structural_order.iter().copied().enumerate() {
            candidates[index].fusion_score = reciprocal_rank_score(rank + 1);
        }
        for (rank, index) in text_order.into_iter().enumerate() {
            candidates[index].fusion_score += reciprocal_rank_score(rank + 1);
        }
        for candidate in &mut candidates {
            candidate.score = candidate.fusion_score.round() as u16;
        }
    }

    candidates.sort_by(|left, right| {
        right
            .fusion_score
            .total_cmp(&left.fusion_score)
            .then_with(|| right.text_score.total_cmp(&left.text_score))
            .then_with(|| right.base_score.cmp(&left.base_score))
            .then_with(|| compare_freshness(left.document.memory, right.document.memory))
    });
    candidates.truncate(query.limit);
    candidates
        .into_iter()
        .map(|candidate| RecallHit {
            memory: candidate.document.memory,
            matched_on: MatchDetails {
                route: candidate.route,
                task: candidate.task_match,
                keyword_matches: candidate.keyword_matches,
                text_matches: candidate.text_matches,
            },
            score: candidate.score,
        })
        .collect()
}

struct Candidate<'a> {
    document: RecallDocument<'a>,
    route: RouteMatchLevel,
    task_match: bool,
    keyword_matches: u8,
    base_score: u16,
    text_score: f64,
    text_matches: u8,
    fusion_score: f64,
    score: u16,
}

fn compare_structural(left: &Candidate<'_>, right: &Candidate<'_>) -> std::cmp::Ordering {
    right
        .base_score
        .cmp(&left.base_score)
        .then_with(|| compare_freshness(left.document.memory, right.document.memory))
}

fn compare_freshness(left: &MemoryIndex, right: &MemoryIndex) -> std::cmp::Ordering {
    left.status()
        .freshness_order()
        .cmp(&right.status().freshness_order())
        .then_with(|| right.verified_at().cmp(&left.verified_at()))
        .then_with(|| left.id().cmp(right.id()))
}

fn reciprocal_rank_score(rank: usize) -> f64 {
    RRF_SCALE / (RRF_K + rank as f64)
}

fn memory_kind_name(kind: MemoryKind) -> &'static str {
    match kind {
        MemoryKind::Workflow => "workflow",
        MemoryKind::SiteNote => "site-note",
        MemoryKind::Gotcha => "gotcha",
    }
}
