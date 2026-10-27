//! Deterministic, side-effect-free recall candidate selection and ranking.

use crate::{
    keyword::{Keyword, TaskCategory},
    memory::{MemoryIndex, MemoryKind},
    url_scope::{CanonicalUrl, RouteMatchLevel},
};
use std::collections::BTreeSet;

const MAX_RESULTS: usize = 3;
const MAX_KEYWORDS: usize = 8;

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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MatchDetails {
    pub route: RouteMatchLevel,
    pub task: bool,
    pub keyword_matches: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecallHit<'a> {
    pub memory: &'a MemoryIndex,
    pub matched_on: MatchDetails,
    pub score: u16,
}

pub fn recall<'a>(memories: &'a [MemoryIndex], query: &RecallQuery) -> Vec<RecallHit<'a>> {
    let query_keywords: BTreeSet<_> = query.keywords.iter().collect();
    let mut hits = Vec::new();

    for memory in memories {
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

        hits.push(RecallHit {
            memory,
            matched_on: MatchDetails {
                route,
                task: task_match,
                keyword_matches,
            },
            score,
        });
    }

    hits.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| {
                left.memory
                    .status()
                    .freshness_order()
                    .cmp(&right.memory.status().freshness_order())
            })
            .then_with(|| right.memory.verified_at().cmp(&left.memory.verified_at()))
            .then_with(|| left.memory.id().cmp(right.memory.id()))
    });
    hits.truncate(query.limit);
    hits
}
