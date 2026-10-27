//! Validated, immutable-in-memory projection of a published memory index.
//!
//! Full content layers and persistence live outside this crate. A database adapter
//! constructs this index only from validated immutable versions.

use crate::{
    keyword::{Keyword, TaskCategory},
    url_scope::UrlPattern,
};
use std::collections::BTreeSet;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MemoryKind {
    Workflow,
    SiteNote,
    Gotcha,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MemoryStatus {
    Active,
    NeedsReverify,
    Superseded,
    Retired,
}

impl MemoryStatus {
    pub(crate) fn is_recallable(self) -> bool {
        matches!(self, Self::Active | Self::NeedsReverify)
    }

    pub(crate) fn freshness_order(self) -> u8 {
        match self {
            Self::Active => 0,
            Self::NeedsReverify => 1,
            Self::Superseded | Self::Retired => 2,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MemoryIndexError {
    InvalidId,
    InvalidVersionId,
    InvalidIntentKey,
    InvalidUrlPatterns,
    MixedHosts,
    DuplicateUrlPattern,
    TooManyKeywords,
    DuplicateKeyword,
    InvalidConfidenceBand,
}

impl fmt::Display for MemoryIndexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let code = match self {
            Self::InvalidId => "invalid_memory_id",
            Self::InvalidVersionId => "invalid_version_id",
            Self::InvalidIntentKey => "invalid_intent_key",
            Self::InvalidUrlPatterns => "invalid_url_patterns",
            Self::MixedHosts => "mixed_hosts",
            Self::DuplicateUrlPattern => "duplicate_url_pattern",
            Self::TooManyKeywords => "too_many_keywords",
            Self::DuplicateKeyword => "duplicate_keyword",
            Self::InvalidConfidenceBand => "invalid_confidence_band",
        };
        f.write_str(code)
    }
}

impl std::error::Error for MemoryIndexError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryIndex {
    id: String,
    version_id: String,
    url_patterns: Vec<UrlPattern>,
    task_category: TaskCategory,
    kind: MemoryKind,
    intent_key: String,
    keywords: Vec<Keyword>,
    confidence_band: u8,
    status: MemoryStatus,
    verified_at: u64,
}

impl MemoryIndex {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        version_id: impl Into<String>,
        url_patterns: Vec<UrlPattern>,
        task_category: TaskCategory,
        kind: MemoryKind,
        intent_key: impl Into<String>,
        keywords: Vec<Keyword>,
        confidence_band: u8,
        status: MemoryStatus,
        verified_at: u64,
    ) -> Result<Self, MemoryIndexError> {
        let id = id.into();
        let version_id = version_id.into();
        let intent_key = intent_key.into();

        if !valid_key(&id, true) {
            return Err(MemoryIndexError::InvalidId);
        }
        if !valid_key(&version_id, false) {
            return Err(MemoryIndexError::InvalidVersionId);
        }
        if !valid_key(&intent_key, false) {
            return Err(MemoryIndexError::InvalidIntentKey);
        }
        if url_patterns.is_empty() || url_patterns.len() > 4 {
            return Err(MemoryIndexError::InvalidUrlPatterns);
        }
        let first_host = url_patterns[0].host();
        if url_patterns
            .iter()
            .any(|pattern| pattern.host() != first_host)
        {
            return Err(MemoryIndexError::MixedHosts);
        }
        let unique_patterns: BTreeSet<_> = url_patterns.iter().collect();
        if unique_patterns.len() != url_patterns.len() {
            return Err(MemoryIndexError::DuplicateUrlPattern);
        }
        if keywords.len() > 8 {
            return Err(MemoryIndexError::TooManyKeywords);
        }
        let unique_keywords: BTreeSet<_> = keywords.iter().collect();
        if unique_keywords.len() != keywords.len() {
            return Err(MemoryIndexError::DuplicateKeyword);
        }
        if confidence_band > 2 {
            return Err(MemoryIndexError::InvalidConfidenceBand);
        }

        Ok(Self {
            id,
            version_id,
            url_patterns,
            task_category,
            kind,
            intent_key,
            keywords,
            confidence_band,
            status,
            verified_at,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn version_id(&self) -> &str {
        &self.version_id
    }

    pub fn url_patterns(&self) -> &[UrlPattern] {
        &self.url_patterns
    }

    pub fn task_category(&self) -> &TaskCategory {
        &self.task_category
    }

    pub fn kind(&self) -> MemoryKind {
        self.kind
    }

    pub fn intent_key(&self) -> &str {
        &self.intent_key
    }

    pub fn keywords(&self) -> &[Keyword] {
        &self.keywords
    }

    pub fn confidence_band(&self) -> u8 {
        self.confidence_band
    }

    pub fn status(&self) -> MemoryStatus {
        self.status
    }

    pub fn verified_at(&self) -> u64 {
        self.verified_at
    }
}

fn valid_key(value: &str, allow_colon: bool) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.as_bytes()[0].is_ascii_lowercase()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'-' | b'_' | b'.')
                || (allow_colon && byte == b':')
        })
}
