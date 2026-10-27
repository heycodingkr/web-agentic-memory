//! The layered record (L0 locator, L1 summary, L2 detail) and its validation (docs/spec/memory.md).

use crate::error::{ErrorCode, ToolError};
use crate::text::{check_text, est_tokens};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use wam_core::{
    CanonicalUrl, Keyword, KeywordVocabulary, MemoryKind, RouteCoverage, TaskVocabulary, UrlMode,
    UrlPattern,
};

pub const ACTIONS: [&str; 11] = [
    "navigate", "wait", "click", "type", "key", "snapshot", "scroll", "select", "upload", "dialog",
    "other",
];
const MAX_L0_BYTES: usize = 1024;
const MAX_L1_TOKENS: usize = 400;
const MAX_L2_CHARS: usize = 8000;

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct L0 {
    pub kind: String,
    pub site_host: String,
    pub url_patterns: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_key: Option<String>,
    pub keywords: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct L1 {
    pub title: String,
    pub summary: String,
    #[serde(default)]
    pub key_points: Vec<String>,
    #[serde(default)]
    pub pitfalls: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note_code: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    pub pattern: String,
    pub page_type: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Structure {
    #[serde(default)]
    pub page_types: Vec<String>,
    #[serde(default)]
    pub routes: Vec<Route>,
    #[serde(default)]
    pub ui_traits: Vec<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct L2 {
    #[serde(default)]
    pub steps: Vec<Step>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structure: Option<Structure>,
    #[serde(default)]
    pub preconditions: Vec<String>,
    #[serde(default)]
    pub recovery: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordPayload {
    pub l0: L0,
    pub l1: L1,
    pub l2: L2,
}

pub fn parse_kind(raw: &str) -> Option<MemoryKind> {
    match raw {
        "workflow" => Some(MemoryKind::Workflow),
        "site_note" => Some(MemoryKind::SiteNote),
        "gotcha" => Some(MemoryKind::Gotcha),
        _ => None,
    }
}

pub fn kind_str(kind: MemoryKind) -> &'static str {
    match kind {
        MemoryKind::Workflow => "workflow",
        MemoryKind::SiteNote => "site_note",
        MemoryKind::Gotcha => "gotcha",
    }
}

/// `/editor/post/:id` is an exact route, `/editor/*` a subtree, `/*` the whole host.
pub fn build_pattern(host: &str, route: &str, mode: UrlMode) -> Result<UrlPattern, String> {
    if route.len() > 200
        || !route.starts_with('/')
        || route
            .chars()
            .any(|c| c.is_whitespace() || c == '?' || c == '#')
    {
        return Err("route must start with / and carry no query, fragment, or spaces".into());
    }
    let (path, coverage) = if route == "/*" {
        ("/", RouteCoverage::Host)
    } else if let Some(base) = route.strip_suffix("/*") {
        (base, RouteCoverage::Subtree)
    } else {
        (route, RouteCoverage::Exact)
    };
    let scheme = if mode == UrlMode::Public {
        "https"
    } else {
        "http"
    };
    let url = CanonicalUrl::parse(&format!("{scheme}://{host}{path}"), mode)
        .map_err(|error| error.code().to_string())?;
    Ok(match coverage {
        RouteCoverage::Exact => url.exact_pattern(),
        RouteCoverage::Subtree => url.subtree_pattern(),
        RouteCoverage::Host => url.host_pattern(),
    })
}

pub fn pattern_key(pattern: &UrlPattern) -> String {
    match pattern.coverage() {
        RouteCoverage::Exact => format!("{}{}", pattern.host(), pattern.path()),
        RouteCoverage::Subtree => format!(
            "{}{}/*",
            pattern.host(),
            pattern.path().trim_end_matches('/')
        ),
        RouteCoverage::Host => format!("{}/*", pattern.host()),
    }
}

/// True when `ancestor` is `path` itself or a segment-wise prefix of it.
pub fn path_covers(ancestor: &str, path: &str) -> bool {
    let ancestor = ancestor.trim_end_matches('/');
    path.trim_end_matches('/') == ancestor || path.starts_with(&format!("{ancestor}/"))
}

/// Observation steps (`snapshot`, `wait`) and consecutive repeats do not change what a procedure does.
pub fn normalize_actions(actions: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for action in actions {
        if action == "snapshot" || action == "wait" {
            continue;
        }
        if out.last() != Some(action) {
            out.push(action.clone());
        }
    }
    out
}

pub fn candidate_fingerprint(actions: &[String]) -> String {
    sha256_hex(normalize_actions(actions).join(">").as_bytes())
}

pub fn step_actions(steps: &[Step]) -> Vec<String> {
    steps
        .iter()
        .map(|step| match &step.when {
            Some(when) => format!("{}:{when}", step.action),
            None => step.action.clone(),
        })
        .collect()
}

pub fn fingerprint_of(payload: &RecordPayload) -> String {
    match parse_kind(&payload.l0.kind) {
        Some(MemoryKind::SiteNote) => sha256_hex(
            serde_json::to_string(&payload.l2.structure)
                .unwrap_or_default()
                .as_bytes(),
        ),
        Some(MemoryKind::Gotcha) => sha256_hex(payload.l1.pitfalls.join(",").as_bytes()),
        _ => candidate_fingerprint(&step_actions(&payload.l2.steps)),
    }
}

/// The text an agent sees for a hint (layer 1).
pub fn render_l1(payload: &RecordPayload) -> String {
    let mut out = format!("{}\n{}", payload.l1.title, payload.l1.summary);
    for point in &payload.l1.key_points {
        out.push_str(&format!("\n- {point}"));
    }
    if !payload.l1.pitfalls.is_empty() {
        out.push_str(&format!("\npitfalls: {}", payload.l1.pitfalls.join(", ")));
    }
    out
}

fn code_ok(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 40
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

pub struct ValidationCtx<'a> {
    pub host: &'a str,
    pub task: &'a str,
    pub intent_key: &'a str,
    pub tasks: &'a TaskVocabulary,
    pub mode: UrlMode,
}

pub struct Validated {
    pub payload: RecordPayload,
    pub kind: MemoryKind,
    pub patterns: Vec<UrlPattern>,
    pub keywords: Vec<Keyword>,
    pub fingerprint: String,
}

fn schema(message: impl Into<String>) -> ToolError {
    ToolError::new(ErrorCode::SchemaInvalid, message)
}

fn scope(message: impl Into<String>) -> ToolError {
    ToolError::new(ErrorCode::ScopeViolation, message)
}

fn screen(field: &str, text: &str, host: &str, multiline: bool) -> Result<(), ToolError> {
    check_text(text, host, multiline)
        .map_err(|reason| ToolError::new(ErrorCode::TextRejected, format!("{field}: {reason}")))
}

fn check_codes(field: &str, values: &[String], max: usize) -> Result<(), ToolError> {
    if values.len() > max {
        return Err(schema(format!("{field} has more than {max} items")));
    }
    match values.iter().find(|value| !code_ok(value)) {
        Some(_) => Err(schema(format!(
            "{field} items must be closed codes such as resume-dialog"
        ))),
        None => Ok(()),
    }
}

/// Validates and normalizes a model-written record against the job's scope.
pub fn validate(
    mut payload: RecordPayload,
    ctx: &ValidationCtx<'_>,
) -> Result<Validated, ToolError> {
    let host_bare = ctx.host.split(':').next().unwrap_or(ctx.host);
    let kind = parse_kind(&payload.l0.kind)
        .ok_or_else(|| schema("l0.kind must be workflow, site_note, or gotcha"))?;
    if payload.l0.site_host != ctx.host {
        return Err(scope("l0.site_host must equal the job's site host"));
    }
    let given_task = payload.l0.task_category.clone();
    if given_task.as_deref().is_some_and(|task| task != ctx.task) {
        return Err(scope("l0.task_category must equal the job's task category"));
    }
    payload.l0.task_category = Some(ctx.task.to_owned());
    let given_intent = payload.l0.intent_key.clone();
    if given_intent
        .as_deref()
        .is_some_and(|intent| intent != ctx.intent_key)
    {
        return Err(scope("l0.intent_key must equal the job's intent key"));
    }
    payload.l0.intent_key = Some(ctx.intent_key.to_owned());
    if !ctx.tasks.contains(ctx.task) {
        return Err(schema(
            "the job's task category is not in the task vocabulary",
        ));
    }

    if payload.l0.url_patterns.is_empty() || payload.l0.url_patterns.len() > 4 {
        return Err(schema("l0.url_patterns needs 1 to 4 route patterns"));
    }
    let mut patterns: Vec<UrlPattern> = Vec::new();
    for route in &payload.l0.url_patterns {
        let pattern = build_pattern(ctx.host, route, ctx.mode)
            .map_err(|why| schema(format!("invalid route pattern {route:?}: {why}")))?;
        if pattern.host() != ctx.host {
            return Err(scope("route patterns must stay inside the job's site host"));
        }
        if patterns.contains(&pattern) {
            return Err(schema("duplicate route pattern"));
        }
        patterns.push(pattern);
    }

    if payload.l0.keywords.len() > 8 {
        return Err(schema("l0.keywords has more than 8 items"));
    }
    let keywords: Vec<Keyword> = if payload.l0.keywords.is_empty() {
        Vec::new()
    } else {
        let vocabulary = KeywordVocabulary::new("author", &payload.l0.keywords)
            .map_err(|error| schema(format!("invalid or duplicate keywords: {error:?}")))?;
        payload
            .l0
            .keywords
            .iter()
            .filter_map(|keyword| vocabulary.resolve(keyword))
            .collect()
    };
    payload.l0.keywords = keywords
        .iter()
        .map(|keyword| keyword.as_str().to_owned())
        .collect();
    if serde_json::to_vec(&payload.l0).map_or(0, |bytes| bytes.len()) > MAX_L0_BYTES {
        return Err(schema("l0 exceeds 1 KiB"));
    }

    match kind {
        MemoryKind::Workflow if payload.l2.steps.is_empty() => {
            return Err(schema("a workflow needs l2.steps"));
        }
        MemoryKind::SiteNote
            if payload.l2.structure.as_ref().is_none_or(|s| {
                s.page_types.is_empty() && s.routes.is_empty() && s.ui_traits.is_empty()
            }) =>
        {
            return Err(schema(
                "a site_note needs l2.structure with page types, routes, or UI traits",
            ));
        }
        MemoryKind::Gotcha if payload.l1.pitfalls.is_empty() => {
            return Err(schema("a gotcha needs l1.pitfalls"));
        }
        _ => {}
    }

    let title_len = payload.l1.title.chars().count();
    let summary_len = payload.l1.summary.chars().count();
    if !(1..=120).contains(&title_len) || !(1..=400).contains(&summary_len) {
        return Err(schema(
            "l1.title must be 1-120 characters and l1.summary 1-400",
        ));
    }
    if payload.l1.key_points.len() > 5
        || payload
            .l1
            .key_points
            .iter()
            .any(|p| p.chars().count() > 120)
    {
        return Err(schema(
            "l1.key_points allows 5 items of at most 120 characters",
        ));
    }
    check_codes("l1.pitfalls", &payload.l1.pitfalls, 5)?;
    screen("l1.title", &payload.l1.title, host_bare, false)?;
    screen("l1.summary", &payload.l1.summary, host_bare, false)?;
    for point in &payload.l1.key_points {
        screen("l1.key_points", point, host_bare, false)?;
    }
    if est_tokens(&render_l1(&payload)) > MAX_L1_TOKENS {
        return Err(schema("layer 1 exceeds 400 estimated tokens"));
    }

    if payload.l2.steps.len() > 40 {
        return Err(schema("l2.steps allows at most 40 steps"));
    }
    for step in &payload.l2.steps {
        if !ACTIONS.contains(&step.action.as_str()) {
            return Err(schema(format!("unknown step action {:?}", step.action)));
        }
        for code in [&step.when, &step.note_code].into_iter().flatten() {
            if !code_ok(code) {
                return Err(schema(
                    "step codes must be closed codes such as resume-dialog",
                ));
            }
        }
    }
    if let Some(structure) = &payload.l2.structure {
        check_codes("l2.structure.page_types", &structure.page_types, 12)?;
        check_codes("l2.structure.ui_traits", &structure.ui_traits, 12)?;
        if structure.routes.len() > 20 {
            return Err(schema("l2.structure.routes allows at most 20 routes"));
        }
        for route in &structure.routes {
            if !code_ok(&route.page_type) {
                return Err(schema("route page types must be closed codes"));
            }
            let pattern = build_pattern(ctx.host, &route.pattern, ctx.mode).map_err(|why| {
                schema(format!(
                    "invalid structure route {:?}: {why}",
                    route.pattern
                ))
            })?;
            if pattern.host() != ctx.host {
                return Err(scope(
                    "structure routes must stay inside the job's site host",
                ));
            }
        }
    }
    check_codes("l2.preconditions", &payload.l2.preconditions, 8)?;
    if payload.l2.recovery.len() > 8 || payload.l2.recovery.iter().any(|r| r.chars().count() > 160)
    {
        return Err(schema(
            "l2.recovery allows 8 items of at most 160 characters",
        ));
    }
    for item in &payload.l2.recovery {
        screen("l2.recovery", item, host_bare, false)?;
    }
    if let Some(detail) = &payload.l2.detail {
        if detail.chars().count() > 1500 {
            return Err(schema("l2.detail allows at most 1500 characters"));
        }
        screen("l2.detail", detail, host_bare, true)?;
    }
    if serde_json::to_string(&payload.l2).map_or(0, |text| text.chars().count()) > MAX_L2_CHARS {
        return Err(schema("layer 2 exceeds 8000 characters"));
    }

    let fingerprint = fingerprint_of(&payload);
    Ok(Validated {
        payload,
        kind,
        patterns,
        keywords,
        fingerprint,
    })
}
