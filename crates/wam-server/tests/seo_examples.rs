//! Search Console, Bing Webmaster Tools and Cloudflare DNS example workflows (issue #217).
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;
use wam_server::{ApplicationRepository, DemoTokenAuthenticator, WamService, examples, router};

const SEARCH_CONSOLE: &str = "web:example-search-console-domain-verify";
const BING: &str = "web:example-bing-webmaster-add-site";
const CLOUDFLARE: &str = "web:example-cloudflare-dns-record-gotcha";

fn test_app() -> axum::Router {
    let repository = ApplicationRepository::demo();
    repository.ensure_examples().expect("examples load");
    router(
        Arc::new(WamService::durable(repository).expect("service")),
        Arc::new(DemoTokenAuthenticator::with_token("local-test-token")),
    )
}

async fn request(app: &axum::Router, method: &str, path: &str, body: Value) -> Value {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK, "{path}");
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

async fn recall(app: &axum::Router, url: &str, task: &str, keyword: &str) -> Vec<String> {
    request(
        app,
        "POST",
        "/v1/memories/recall",
        json!({"url": url, "task_category": task, "keywords": [keyword]}),
    )
    .await["hits"]
        .as_array()
        .expect("hits")
        .iter()
        .map(|hit| hit["memory_id"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn each_tool_workflow_is_recalled_only_inside_its_own_routes() {
    let app = test_app();
    for (url, keyword, expected, prefix) in [
        (
            "https://search.google.com/search-console/welcome",
            "sitemap",
            SEARCH_CONSOLE,
            "web:example-search-console-",
        ),
        (
            "https://search.google.com/search-console/sitemaps?resource_id=sc-domain%3Aexample.org",
            "dns",
            SEARCH_CONSOLE,
            "web:example-search-console-",
        ),
        (
            "https://www.bing.com/webmasters/home/mysites?siteUrl=https://example.org/",
            "cname record",
            BING,
            "web:example-bing-",
        ),
        (
            "https://dash.cloudflare.com/abc123/example-zone/dns/records",
            "proxy",
            CLOUDFLARE,
            "web:example-cloudflare-",
        ),
    ] {
        let hits = recall(&app, url, "navigate", keyword).await;
        assert!(hits.iter().any(|id| id == expected), "{url}: {hits:?}");
        assert!(
            hits.iter().all(|id| id.starts_with(prefix)),
            "{url}: {hits:?}"
        );
    }
}

#[tokio::test]
async fn ordinary_search_pages_and_look_alike_hosts_do_not_get_the_workflows() {
    let app = test_app();
    for (url, keyword) in [
        ("https://search.google.com/search?q=sitemap", "sitemap"),
        ("https://www.bing.com/search?q=cname", "cname record"),
        (
            "https://search.google.com.evil.example.org/search-console/welcome",
            "sitemap",
        ),
        (
            "https://www.bing.com.evil.example.org/webmasters/home",
            "cname record",
        ),
    ] {
        assert!(
            recall(&app, url, "navigate", keyword).await.is_empty(),
            "{url}"
        );
    }
    assert!(
        recall(
            &app,
            "https://search.google.com/search-console/welcome",
            "research",
            "sitemap"
        )
        .await
        .is_empty(),
        "these are navigation workflows, not research"
    );
}

#[tokio::test]
async fn workflows_expand_to_steps_and_say_they_were_observed_once() {
    let app = test_app();
    let catalog = request(&app, "GET", "/v1/memories", Value::Null).await;
    for id in [SEARCH_CONSOLE, BING, CLOUDFLARE] {
        assert!(
            catalog["memories"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| row["memory_id"] == id),
            "{id} missing from catalog"
        );
        let escaped_id = id.replace(':', "%3A");
        let hint = request(
            &app,
            "GET",
            &format!("/v1/memories/{escaped_id}?level=1"),
            Value::Null,
        )
        .await;
        assert!(hint["content"].as_str().unwrap().starts_with("Example:"));
        let detail = request(
            &app,
            "GET",
            &format!("/v1/memories/{escaped_id}?level=2"),
            Value::Null,
        )
        .await;
        let detail: Value = serde_json::from_str(detail["content"].as_str().unwrap()).unwrap();
        assert!(detail["steps"].as_array().unwrap().len() >= 5, "{id}");
        assert!(
            detail["detail"]
                .as_str()
                .unwrap()
                .contains("Observed once in Arise on 2026-11-01"),
            "{id} must say how much was actually observed"
        );
    }
}

fn position(steps: &[wam_gatekeeper::record::Step], code: &str) -> usize {
    steps
        .iter()
        .position(|step| step.note_code.as_deref() == Some(code))
        .unwrap_or_else(|| panic!("missing step {code}"))
}

#[test]
fn the_dns_record_is_read_added_and_checked_before_verifying() {
    let loaded = examples::load().expect("validated examples");
    let steps = |id: &str| {
        loaded
            .iter()
            .find(|row| row.id == id)
            .unwrap_or_else(|| panic!("{id} is not bundled"))
            .payload
            .l2
            .steps
            .clone()
    };
    let google = steps(SEARCH_CONSOLE);
    assert!(
        position(&google, "read-full-txt-value") < position(&google, "add-txt-at-dns-provider")
    );
    assert!(position(&google, "add-txt-at-dns-provider") < position(&google, "check-txt-resolves"));
    assert!(position(&google, "check-txt-resolves") < position(&google, "verify"));
    assert!(position(&google, "verify") < position(&google, "full-sitemap-address"));
    let bing = steps(BING);
    assert!(position(&bing, "read-cname-name-and-value") < position(&bing, "add-cname-dns-only"));
    assert!(position(&bing, "add-cname-dns-only") < position(&bing, "check-cname-resolves"));
    assert!(position(&bing, "check-cname-resolves") < position(&bing, "verify"));
    assert!(position(&bing, "new-site") < position(&bing, "submit-sitemap"));
}

#[tokio::test]
async fn a_combined_recall_stays_a_short_hint_bundle() {
    let app = test_app();
    let recalled = request(
        &app,
        "POST",
        "/v1/memories/recall",
        json!({
            "url": "https://search.google.com/search-console/welcome",
            "task_category": "navigate",
            "keywords": ["sitemap", "dns", "txt record", "verify"],
        }),
    )
    .await;
    let hits = recalled["hits"].as_array().expect("hits");
    assert!(!hits.is_empty());
    let chars: usize = hits
        .iter()
        .map(|hit| hit["hint"].as_str().expect("rendered L1").chars().count())
        .sum();
    assert!(chars <= 1600, "{chars} characters in the L1 bundle");
}
