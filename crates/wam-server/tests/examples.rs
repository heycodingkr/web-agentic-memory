//! Built-in example memories: valid, injected once, and recalled by URL scope (issue #102).
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use serde_json::Value;
use std::{collections::BTreeSet, sync::Arc};
use tower::ServiceExt;
use wam_server::{ApplicationRepository, DemoTokenAuthenticator, WamService, examples, router};

fn app_with_examples() -> (axum::Router, ApplicationRepository, usize) {
    let repository = ApplicationRepository::demo();
    let added = repository.ensure_examples().expect("examples load");
    let service = Arc::new(WamService::durable(repository.clone()).expect("durable service"));
    let app = router(
        service,
        Arc::new(DemoTokenAuthenticator::with_token("local-test-token")),
    );
    (app, repository, added)
}

async fn get(app: &axum::Router, path: &str) -> Value {
    let response = app
        .clone()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK, "{path}");
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

async fn recall(app: &axum::Router, body: &str) -> Vec<String> {
    let request = Request::post("/v1/memories/recall")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "{body}");
    let value: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    value["hits"]
        .as_array()
        .expect("hits")
        .iter()
        .map(|hit| hit["memory_id"].as_str().expect("memory_id").to_owned())
        .collect()
}

#[test]
fn bundled_examples_pass_the_record_validator_and_cover_the_common_sites() {
    let loaded = examples::load().expect("every bundled example validates");
    assert!(loaded.len() >= 12, "only {} examples", loaded.len());
    let hosts: BTreeSet<_> = loaded
        .iter()
        .map(|e| e.payload.l0.site_host.as_str())
        .collect();
    for host in [
        "docs.google.com",
        "mail.google.com",
        "calendar.google.com",
        "drive.google.com",
        "medium.com",
    ] {
        assert!(hosts.contains(host), "no example for {host}");
    }
    let tasks: BTreeSet<_> = loaded
        .iter()
        .filter_map(|e| e.payload.l0.task_category.as_deref())
        .collect();
    assert_eq!(
        tasks.len(),
        3,
        "one example per task category at least: {tasks:?}"
    );
    for example in &loaded {
        assert!(
            example.payload.l1.title.starts_with("Example:"),
            "{}",
            example.id
        );
        assert!(!example.payload.l2.steps.is_empty(), "{}", example.id);
    }
}

#[tokio::test]
async fn examples_are_added_once_and_leave_existing_rows_alone() {
    let (app, repository, added) = app_with_examples();
    assert_eq!(added, examples::load().unwrap().len());
    assert_eq!(
        repository.ensure_examples().unwrap(),
        0,
        "second load adds nothing"
    );
    let catalog = get(&app, "/v1/memories").await;
    let memories = catalog["memories"].as_array().unwrap();
    assert_eq!(memories.len(), added + 1, "the demo row plus the examples");
    assert!(
        memories
            .iter()
            .any(|m| m["memory_id"] == "web:example-draft")
    );
    let example = memories
        .iter()
        .find(|m| m["memory_id"] == "web:example-docs-meeting-notes")
        .expect("docs example listed");
    assert!(example["title"].as_str().unwrap().starts_with("Example:"));
    assert_eq!(example["domain"], "docs.google.com");
    assert_eq!(example["verified_date"], examples::WRITTEN_AT.to_string());
}

#[tokio::test]
async fn recall_is_scoped_by_path_so_docs_and_sheets_do_not_leak_into_each_other() {
    let (app, _, _) = app_with_examples();
    let docs = recall(
        &app,
        r#"{"url":"https://docs.google.com/document/d/abc123/edit?tab=t.0#heading=h.x","task_category":"compose-draft","keywords":["document"]}"#,
    )
    .await;
    assert!(
        docs.contains(&"web:example-docs-meeting-notes".to_owned()),
        "{docs:?}"
    );
    assert!(
        docs.iter().all(|id| id.starts_with("web:example-docs-")),
        "{docs:?}"
    );
    let sheets = recall(
        &app,
        r#"{"url":"https://docs.google.com/spreadsheets/d/abc123/edit?gid=0#gid=0","task_category":"compose-draft","keywords":["spreadsheet"]}"#,
    )
    .await;
    assert!(
        sheets.contains(&"web:example-sheets-work-log".to_owned()),
        "{sheets:?}"
    );
    assert!(
        sheets
            .iter()
            .all(|id| id.starts_with("web:example-sheets-")),
        "{sheets:?}"
    );
}

#[tokio::test]
async fn mail_calendar_and_research_examples_are_recalled_and_the_vocabulary_grows() {
    let (app, _, _) = app_with_examples();
    let mail = recall(
        &app,
        r#"{"url":"https://mail.google.com/mail/u/0/#inbox","task_category":"compose-draft","keywords":["email"]}"#,
    )
    .await;
    assert!(
        mail.contains(&"web:example-gmail-compose".to_owned()),
        "{mail:?}"
    );
    assert!(
        mail.iter().all(|id| id.starts_with("web:example-gmail-")),
        "{mail:?}"
    );
    let calendar = recall(
        &app,
        r#"{"url":"https://calendar.google.com/calendar/u/0/r","task_category":"compose-draft","keywords":["calendar"]}"#,
    )
    .await;
    // More calendar workflows are bundled too (issue #103): the basic one is among them and nothing else is.
    assert!(
        calendar.contains(&"web:example-calendar-event".to_owned()),
        "{calendar:?}"
    );
    assert!(
        calendar
            .iter()
            .all(|id| id.starts_with("web:example-calendar-")),
        "{calendar:?}"
    );
    let wiki = recall(
        &app,
        r#"{"url":"https://en.wikipedia.org/wiki/Spreadsheet","task_category":"research","keywords":["research"]}"#,
    )
    .await;
    assert!(
        wiki.contains(&"web:example-wikipedia-facts".to_owned()),
        "{wiki:?}"
    );
    assert!(
        wiki.iter()
            .all(|id| id.starts_with("web:example-wikipedia-")),
        "{wiki:?}"
    );
    let other_task = recall(
        &app,
        r#"{"url":"https://mail.google.com/mail/u/0/","task_category":"research","keywords":["email"]}"#,
    )
    .await;
    assert!(
        other_task.is_empty(),
        "a research lookup must not return the compose example: {other_task:?}"
    );

    let response = app
        .clone()
        .oneshot(
            Request::get("/v1/memories/vocab")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    let vocabulary: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let keywords: Vec<&str> = vocabulary["keywords"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    for word in ["spreadsheet", "email", "calendar", "document", "template"] {
        assert!(keywords.contains(&word), "{word} missing from {keywords:?}");
    }
}

#[tokio::test]
async fn level_one_and_two_serve_the_example_text() {
    let (app, _, _) = app_with_examples();
    let hint = get(&app, "/v1/memories/web%3Aexample-sheets-work-log?level=1").await;
    let content = hint["content"].as_str().unwrap();
    assert!(
        content.starts_with("Example: Google Sheets daily work log"),
        "{content}"
    );
    assert!(content.contains("equals sign"), "{content}");
    let detail = get(&app, "/v1/memories/web%3Aexample-sheets-work-log?level=2").await;
    let parsed: Value = serde_json::from_str(detail["content"].as_str().unwrap()).unwrap();
    assert!(parsed["steps"].as_array().unwrap().len() >= 5);
    assert!(
        parsed["detail"]
            .as_str()
            .unwrap()
            .contains("Example written for testing recall")
    );
}
