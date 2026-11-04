//! Trip.com flight search example workflow (issue #270). Its only measured evidence is a preliminary
//! individual-run observation (docs/test/benchmarks/trip-flights.md), so these tests pin scope and shape, not speed.
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;
use wam_server::{ApplicationRepository, DemoTokenAuthenticator, WamService, examples, router};

const TRIP_FLIGHTS: &str = "web:example-trip-flight-search";

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

async fn recall_hits(app: &axum::Router, url: &str, task: &str, keyword: &str) -> Vec<Value> {
    request(
        app,
        "POST",
        "/v1/memories/recall",
        json!({"url": url, "task_category": task, "keywords": [keyword]}),
    )
    .await["hits"]
        .as_array()
        .expect("hits")
        .clone()
}

async fn recall(app: &axum::Router, url: &str, task: &str, keyword: &str) -> Vec<String> {
    recall_hits(app, url, task, keyword)
        .await
        .iter()
        .map(|hit| hit["memory_id"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn the_flight_search_workflow_is_recalled_for_the_flight_routes_of_trip_com() {
    let app = test_app();
    for (url, keyword) in [
        ("https://www.trip.com/flights/", "flights"),
        (
            "https://www.trip.com/flights/showfarefirst?dcity=sel&acity=tyo&ddate=2026-12-15&triptype=ow&class=y&quantity=1&nonstoponly=on",
            "nonstop",
        ),
    ] {
        let hits = recall(&app, url, "research", keyword).await;
        assert!(hits.iter().any(|id| id == TRIP_FLIGHTS), "{url}: {hits:?}");
    }
}

#[tokio::test]
async fn other_trip_com_sections_other_tasks_and_look_alike_hosts_do_not_get_it() {
    let app = test_app();
    for url in [
        "https://www.trip.com/hotels/",
        "https://www.trip.com/",
        "https://www.trip.com.evil.example.org/flights/",
        "https://www.nottrip.com/flights/",
    ] {
        assert!(
            recall(&app, url, "research", "flights").await.is_empty(),
            "{url}"
        );
    }
    assert!(
        recall(&app, "https://www.trip.com/flights/", "navigate", "flights")
            .await
            .is_empty(),
        "a flight search is a research task, not navigation"
    );
}

#[tokio::test]
async fn the_hint_is_short_and_says_what_it_is_and_what_not_to_press() {
    let app = test_app();
    let hits = recall_hits(&app, "https://www.trip.com/flights/", "research", "flights").await;
    let chars: usize = hits
        .iter()
        .map(|hit| hit["hint"].as_str().expect("rendered L1").chars().count())
        .sum();
    assert!(chars <= 1600, "{chars} characters in the L1 bundle");

    let hint = request(
        &app,
        "GET",
        "/v1/memories/web%3Aexample-trip-flight-search?level=1",
        Value::Null,
    )
    .await;
    let hint = hint["content"].as_str().unwrap();
    assert!(hint.starts_with("Example:"), "{hint}");
    assert!(hint.contains("showfarefirst"), "{hint}");
    assert!(hint.contains("Never press Select"), "{hint}");

    let detail = request(
        &app,
        "GET",
        "/v1/memories/web%3Aexample-trip-flight-search?level=2",
        Value::Null,
    )
    .await;
    let detail: Value = serde_json::from_str(detail["content"].as_str().unwrap()).unwrap();
    assert!(detail["steps"].as_array().unwrap().len() >= 5);
    let text = detail["detail"].as_str().unwrap();
    assert!(text.contains("Checked on 2026-11-03"), "{text}");
    assert!(text.contains("do not press it"), "{text}");
}

fn position(steps: &[wam_gatekeeper::record::Step], code: &str) -> usize {
    steps
        .iter()
        .position(|step| step.note_code.as_deref() == Some(code))
        .unwrap_or_else(|| panic!("missing step {code}"))
}

#[test]
fn the_results_link_is_opened_before_the_sort_tab_is_used() {
    let loaded = examples::load().expect("validated examples");
    let steps = loaded
        .iter()
        .find(|row| row.id == TRIP_FLIGHTS)
        .expect("bundled")
        .payload
        .l2
        .steps
        .clone();
    assert!(position(&steps, "results-link") < position(&steps, "sort-cheapest"));
    assert!(position(&steps, "sort-cheapest") < position(&steps, "read-first-card"));
}
