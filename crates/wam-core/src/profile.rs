//! Validated public profile content. Rich text is a typed document; raw HTML is not part of the model.

use std::fmt;
use url::{Host, Url};

const MIN_HANDLE_CHARS: usize = 3;
const MAX_HANDLE_CHARS: usize = 24;
const MAX_BIO_BLOCKS: usize = 20;
const MAX_BIO_CHARS: usize = 1_200;
const MAX_BIO_INLINE_ITEMS: usize = 32;
const MAX_BIO_LIST_ITEMS: usize = 20;
const MAX_BIO_LINKS: usize = 8;
const MAX_SERVICE_LINKS: usize = 5;
const MAX_SERVICE_LINK_LABEL_CHARS: usize = 48;
const MAX_URL_BYTES: usize = 2_048;

const RESERVED_HANDLES: &[&str] = &[
    "admin",
    "api",
    "help",
    "login",
    "leaderboard",
    "memories",
    "profile",
    "settings",
    "support",
    "u",
    "wam",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProfileError {
    InvalidHandle,
    ReservedHandle,
    InvalidExternalUrl,
    InvalidBio,
    BioTooLarge,
    TooManyServiceLinks,
    InvalidServiceLinkLabel,
    DuplicateServiceLink,
}

impl ProfileError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidHandle => "invalid_handle",
            Self::ReservedHandle => "reserved_handle",
            Self::InvalidExternalUrl => "invalid_external_url",
            Self::InvalidBio => "invalid_bio",
            Self::BioTooLarge => "bio_too_large",
            Self::TooManyServiceLinks => "too_many_service_links",
            Self::InvalidServiceLinkLabel => "invalid_service_link_label",
            Self::DuplicateServiceLink => "duplicate_service_link",
        }
    }
}

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl std::error::Error for ProfileError {}

/// Canonical, case-insensitive public handle. It is not an account identifier.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PublicHandle(String);

impl PublicHandle {
    pub fn parse(input: &str) -> Result<Self, ProfileError> {
        let value = input.trim().to_ascii_lowercase();
        let valid = (MIN_HANDLE_CHARS..=MAX_HANDLE_CHARS).contains(&value.len())
            && value.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-'
            })
            && value
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphanumeric)
            && value
                .as_bytes()
                .last()
                .is_some_and(u8::is_ascii_alphanumeric);
        if !valid {
            return Err(ProfileError::InvalidHandle);
        }
        if RESERVED_HANDLES.contains(&value.as_str()) {
            return Err(ProfileError::ReservedHandle);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// An HTTPS destination with a syntactically public DNS host. No DNS lookup or fetch occurs.
/// Query parameters that commonly carry credentials are rejected to avoid publishing tokens.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ExternalUrl(String);

impl ExternalUrl {
    pub fn parse(input: &str) -> Result<Self, ProfileError> {
        let input = input.trim();
        if input.is_empty() || input.len() > MAX_URL_BYTES {
            return Err(ProfileError::InvalidExternalUrl);
        }
        let url = Url::parse(input).map_err(|_| ProfileError::InvalidExternalUrl)?;
        if url.scheme() != "https"
            || !url.username().is_empty()
            || url.password().is_some()
            || !matches!(url.host(), Some(Host::Domain(domain)) if is_public_domain(domain))
        {
            return Err(ProfileError::InvalidExternalUrl);
        }
        if url
            .query_pairs()
            .any(|(key, _)| is_sensitive_query_key(&key))
        {
            return Err(ProfileError::InvalidExternalUrl);
        }
        Ok(Self(url.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn is_public_domain(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host.len() > 253 || !host.contains('.') {
        return false;
    }
    if [
        ".localhost",
        ".local",
        ".internal",
        ".test",
        ".invalid",
        ".example",
    ]
    .iter()
    .any(|suffix| host == suffix.trim_start_matches('.') || host.ends_with(suffix))
    {
        return false;
    }

    let mut labels = host.split('.');
    let valid_label = |label: &str| {
        !label.is_empty()
            && label.len() <= 63
            && label
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphanumeric)
            && label
                .as_bytes()
                .last()
                .is_some_and(u8::is_ascii_alphanumeric)
            && label
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    };
    let all_labels_valid = labels.clone().all(valid_label);
    let tld = labels.next_back().unwrap_or_default();
    all_labels_valid && tld.len() >= 2 && tld.bytes().any(|byte| byte.is_ascii_alphabetic())
}

fn is_sensitive_query_key(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "access_token"
            | "api_key"
            | "apikey"
            | "auth"
            | "authorization"
            | "code"
            | "key"
            | "password"
            | "secret"
            | "session"
            | "state"
            | "token"
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BioInline {
    Text(String),
    Strong(String),
    Emphasis(String),
    Link { text: String, url: ExternalUrl },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BioBlock {
    Paragraph(Vec<BioInline>),
    Heading { level: u8, content: Vec<BioInline> },
    BulletList(Vec<Vec<BioInline>>),
    OrderedList(Vec<Vec<BioInline>>),
}

/// Version 1 editor document. Adapters must convert untrusted JSON through
/// try_new; rendering must treat text as text and never as HTML.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BioDocument {
    blocks: Vec<BioBlock>,
}

impl BioDocument {
    pub fn try_new(blocks: Vec<BioBlock>) -> Result<Self, ProfileError> {
        if blocks.len() > MAX_BIO_BLOCKS {
            return Err(ProfileError::BioTooLarge);
        }

        let mut metrics = BioMetrics::default();
        for block in &blocks {
            match block {
                BioBlock::Paragraph(content) => validate_inline_group(content, &mut metrics)?,
                BioBlock::Heading { level, content } => {
                    if !matches!(level, 2 | 3) {
                        return Err(ProfileError::InvalidBio);
                    }
                    validate_inline_group(content, &mut metrics)?;
                }
                BioBlock::BulletList(items) | BioBlock::OrderedList(items) => {
                    if items.is_empty() || items.len() > MAX_BIO_LIST_ITEMS {
                        return Err(ProfileError::InvalidBio);
                    }
                    for item in items {
                        validate_inline_group(item, &mut metrics)?;
                    }
                }
            }
        }

        if metrics.characters > MAX_BIO_CHARS || metrics.links > MAX_BIO_LINKS {
            return Err(ProfileError::BioTooLarge);
        }
        Ok(Self { blocks })
    }

    pub fn blocks(&self) -> &[BioBlock] {
        &self.blocks
    }

    pub fn plain_text_characters(&self) -> usize {
        self.blocks
            .iter()
            .map(|block| match block {
                BioBlock::Paragraph(items) | BioBlock::Heading { content: items, .. } => {
                    inline_text_characters(items)
                }
                BioBlock::BulletList(items) | BioBlock::OrderedList(items) => {
                    items.iter().map(|item| inline_text_characters(item)).sum()
                }
            })
            .sum()
    }
}

#[derive(Default)]
struct BioMetrics {
    characters: usize,
    links: usize,
}

fn validate_inline_group(
    items: &[BioInline],
    metrics: &mut BioMetrics,
) -> Result<(), ProfileError> {
    if items.is_empty() || items.len() > MAX_BIO_INLINE_ITEMS {
        return Err(ProfileError::InvalidBio);
    }

    let mut has_visible_text = false;
    for item in items {
        let (text, is_link) = match item {
            BioInline::Text(text) => (text.as_str(), false),
            BioInline::Strong(text) | BioInline::Emphasis(text) => (text.as_str(), false),
            BioInline::Link { text, .. } => (text.as_str(), true),
        };
        if text.chars().any(char::is_control) {
            return Err(ProfileError::InvalidBio);
        }
        has_visible_text |= !text.trim().is_empty();
        metrics.characters = metrics.characters.saturating_add(text.chars().count());
        if is_link {
            metrics.links = metrics.links.saturating_add(1);
        }
    }

    if !has_visible_text {
        return Err(ProfileError::InvalidBio);
    }
    Ok(())
}

fn inline_text_characters(items: &[BioInline]) -> usize {
    items
        .iter()
        .map(|item| match item {
            BioInline::Text(text) | BioInline::Strong(text) | BioInline::Emphasis(text) => {
                text.chars().count()
            }
            BioInline::Link { text, .. } => text.chars().count(),
        })
        .sum()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceLink {
    label: String,
    url: ExternalUrl,
}

impl ServiceLink {
    pub fn new(label: impl Into<String>, url: ExternalUrl) -> Result<Self, ProfileError> {
        let label = label.into();
        let label = label.trim();
        if label.is_empty()
            || label.chars().count() > MAX_SERVICE_LINK_LABEL_CHARS
            || label.chars().any(char::is_control)
        {
            return Err(ProfileError::InvalidServiceLinkLabel);
        }
        Ok(Self {
            label: label.to_owned(),
            url,
        })
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn url(&self) -> &ExternalUrl {
        &self.url
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreatorProfile {
    handle: PublicHandle,
    bio: BioDocument,
    links: Vec<ServiceLink>,
}

impl CreatorProfile {
    pub fn try_new(
        handle: PublicHandle,
        bio: BioDocument,
        links: Vec<ServiceLink>,
    ) -> Result<Self, ProfileError> {
        if links.len() > MAX_SERVICE_LINKS {
            return Err(ProfileError::TooManyServiceLinks);
        }
        for (index, link) in links.iter().enumerate() {
            if links[..index]
                .iter()
                .any(|previous| previous.url == link.url)
            {
                return Err(ProfileError::DuplicateServiceLink);
            }
        }
        Ok(Self { handle, bio, links })
    }

    pub fn handle(&self) -> &PublicHandle {
        &self.handle
    }

    pub fn bio(&self) -> &BioDocument {
        &self.bio
    }

    pub fn links(&self) -> &[ServiceLink] {
        &self.links
    }
}
