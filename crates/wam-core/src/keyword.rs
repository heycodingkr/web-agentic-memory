//! Versioned closed vocabularies for task categories and recall keywords.
//!
//! Transport adapters resolve untrusted strings through these registries before
//! they can enter the typed core API. Unknown task categories fail closed; unknown
//! keywords are omitted from recall input.

use std::collections::BTreeSet;
use unicode_normalization::UnicodeNormalization;

const MAX_CODE_LENGTH: usize = 48;
const MAX_KEYWORD_LENGTH: usize = 32;
const MAX_KEYWORD_WORDS: usize = 3;
const MAX_QUERY_KEYWORDS: usize = 8;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VocabularyError {
    InvalidVersion,
    Empty,
    InvalidCode,
    DuplicateCode,
    UnknownCode,
    InvalidKeyword,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TaskCategory(String);

impl TaskCategory {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskVocabulary {
    version: String,
    categories: BTreeSet<String>,
}

impl TaskVocabulary {
    pub fn new<I, S>(version: impl Into<String>, categories: I) -> Result<Self, VocabularyError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let version = version.into();
        if !valid_version(&version) {
            return Err(VocabularyError::InvalidVersion);
        }

        let mut values = BTreeSet::new();
        for value in categories {
            let value = value.as_ref();
            if !valid_code(value) {
                return Err(VocabularyError::InvalidCode);
            }
            if !values.insert(value.to_owned()) {
                return Err(VocabularyError::DuplicateCode);
            }
        }
        if values.is_empty() {
            return Err(VocabularyError::Empty);
        }

        Ok(Self {
            version,
            categories: values,
        })
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn resolve(&self, raw: &str) -> Result<TaskCategory, VocabularyError> {
        if self.categories.contains(raw) {
            Ok(TaskCategory(raw.to_owned()))
        } else {
            Err(VocabularyError::UnknownCode)
        }
    }

    pub fn contains(&self, raw: &str) -> bool {
        self.categories.contains(raw)
    }

    pub fn codes(&self) -> impl Iterator<Item = &str> {
        self.categories.iter().map(String::as_str)
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Keyword(String);

impl Keyword {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn parse(raw: &str) -> Result<Self, VocabularyError> {
        let folded: String = raw.nfkc().flat_map(char::to_lowercase).collect();
        let normalized = folded.split_whitespace().collect::<Vec<_>>().join(" ");
        let length = normalized.chars().count();
        if length == 0
            || length > MAX_KEYWORD_LENGTH
            || !normalized.chars().all(|character| {
                character.is_alphanumeric() || character == '-' || character == ' '
            })
            || normalized.starts_with('-')
            || normalized.ends_with('-')
            || normalized.split_whitespace().count() > MAX_KEYWORD_WORDS
            || contains_sensitive_word(&normalized)
        {
            return Err(VocabularyError::InvalidKeyword);
        }
        Ok(Self(normalized))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeywordVocabulary {
    version: String,
    keywords: BTreeSet<Keyword>,
}

impl KeywordVocabulary {
    pub fn new<I, S>(version: impl Into<String>, keywords: I) -> Result<Self, VocabularyError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let version = version.into();
        if !valid_version(&version) {
            return Err(VocabularyError::InvalidVersion);
        }

        let mut values = BTreeSet::new();
        for value in keywords {
            let keyword = Keyword::parse(value.as_ref())?;
            if !values.insert(keyword) {
                return Err(VocabularyError::DuplicateCode);
            }
        }
        if values.is_empty() {
            return Err(VocabularyError::Empty);
        }

        Ok(Self {
            version,
            keywords: values,
        })
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn resolve(&self, raw: &str) -> Option<Keyword> {
        let candidate = Keyword::parse(raw).ok()?;
        self.keywords.get(&candidate).cloned()
    }

    /// Keeps only vocabulary-approved, normalized, distinct keywords.
    /// More than eight approved terms is an invalid query; unapproved terms are dropped.
    pub fn filter<I, S>(&self, raw: I) -> Result<Vec<Keyword>, VocabularyError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut selected = BTreeSet::new();
        for value in raw {
            if let Some(keyword) = self.resolve(value.as_ref()) {
                selected.insert(keyword);
                if selected.len() > MAX_QUERY_KEYWORDS {
                    return Err(VocabularyError::InvalidKeyword);
                }
            }
        }
        Ok(selected.into_iter().collect())
    }

    pub fn contains(&self, keyword: &Keyword) -> bool {
        self.keywords.contains(keyword)
    }

    pub fn values(&self) -> impl Iterator<Item = &str> {
        self.keywords.iter().map(Keyword::as_str)
    }
}

fn valid_version(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
        })
}

fn valid_code(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_CODE_LENGTH
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && value.as_bytes()[0].is_ascii_lowercase()
        && !value.ends_with('-')
}

fn contains_sensitive_word(keyword: &str) -> bool {
    keyword.split_whitespace().any(|word| {
        matches!(
            word,
            "password" | "secret" | "token" | "credential" | "authorization" | "cookie"
        )
    }) || keyword.starts_with("sk-")
        || keyword.starts_with("ghp-")
        || keyword.starts_with("xox-")
}
