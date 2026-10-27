#![forbid(unsafe_code)]

//! Pure, deterministic domain code shared by the future HTTP, MCP, CLI, and web adapters.
//! This crate deliberately owns no network, database, filesystem, clock, or model clients.

pub mod keyword;
pub mod leaderboard;
pub mod memory;
pub mod profile;
pub mod recall;
pub mod url_scope;

pub use keyword::{Keyword, KeywordVocabulary, TaskCategory, TaskVocabulary, VocabularyError};
pub use memory::{MemoryIndex, MemoryIndexError, MemoryKind, MemoryStatus};
pub use recall::{MatchDetails, RecallHit, RecallQuery, RecallQueryError, recall};
pub use url_scope::{
    CanonicalUrl, DynamicSegmentKind, RouteCoverage, RouteMatchLevel, UrlMode, UrlPattern,
    UrlPolicyError,
};
