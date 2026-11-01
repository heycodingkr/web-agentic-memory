//! Additional global productivity workflows (issue #103).
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;
use wam_server::{ApplicationRepository, DemoTokenAuthenticator, WamService, examples, router};

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
async fn canva_templates_editing_and_export_are_recalled_in_their_routes() {
    let app = test_app();
    for (url, keyword, expected) in [
        (
            "https://www.canva.com/templates/",
            "template",
            "web:example-canva-template",
        ),
        (
            "https://www.canva.com/design/abc123/edit",
            "replace",
            "web:example-canva-replace-content",
        ),
        (
            "https://www.canva.com/design/abc123/edit?page=2",
            "export",
            "web:example-canva-export-pages",
        ),
    ] {
        let hits = recall(&app, url, "compose-draft", keyword).await;
        assert!(hits.iter().any(|id| id == expected), "{url}: {hits:?}");
        assert!(
            hits.iter().all(|id| id.starts_with("web:example-canva-")),
            "{hits:?}"
        );
    }
    for url in [
        "https://www.canva.com/help/download-or-purchase/",
        "https://www.canva.com/pricing/",
        "https://www.canva.com.evil.example.org/design/abc123/edit",
    ] {
        let app = test_app();
        assert!(
            recall(&app, url, "compose-draft", "export")
                .await
                .is_empty(),
            "{url}"
        );
    }
    let app = test_app();
    assert!(
        recall(
            &app,
            "https://www.canva.com/design/abc123/edit",
            "research",
            "export"
        )
        .await
        .is_empty()
    );
}

#[tokio::test]
async fn calendar_recurring_timezone_and_reschedule_workflows_are_distinct() {
    let app = test_app();
    for (keyword, expected) in [
        ("recurring", "web:example-calendar-recurring"),
        ("timezone", "web:example-calendar-timezone"),
        ("reschedule", "web:example-calendar-reschedule"),
    ] {
        let hits = recall(
            &app,
            "https://calendar.google.com/calendar/u/0/r/eventedit?dates=20261030",
            "compose-draft",
            keyword,
        )
        .await;
        assert!(hits.iter().any(|id| id == expected), "{keyword}: {hits:?}");
        assert!(
            hits.iter()
                .all(|id| id.starts_with("web:example-calendar-")),
            "{hits:?}"
        );
    }
    assert!(
        recall(
            &app,
            "https://calendar.google.com/about/",
            "compose-draft",
            "timezone",
        )
        .await
        .is_empty()
    );
}

#[tokio::test]
async fn notion_database_creation_entry_and_views_are_recalled_for_the_app() {
    for host in ["app.notion.com", "www.notion.so"] {
        let app = test_app();
        for (keyword, base_id) in [
            ("tracker", "web:example-notion-task-database"),
            ("entry", "web:example-notion-database-entry"),
            ("filter", "web:example-notion-database-views"),
        ] {
            let expected = if host == "app.notion.com" {
                base_id.to_owned()
            } else {
                format!("{base_id}-legacy")
            };
            let hits = recall(
                &app,
                &format!("https://{host}/Project-board-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa?v=view"),
                "compose-draft",
                keyword,
            )
            .await;
            assert!(hits.iter().any(|id| id == &expected), "{keyword}: {hits:?}");
            assert!(
                hits.iter().all(|id| id.starts_with("web:example-notion-")),
                "{hits:?}"
            );
        }
    }
    let app = test_app();
    assert!(
        recall(
            &app,
            "https://www.notion.com/help/create-a-database",
            "compose-draft",
            "tracker",
        )
        .await
        .is_empty()
    );
}

#[tokio::test]
async fn productivity_workflows_are_visible_in_catalog_and_expand_to_actionable_steps() {
    let app = test_app();
    let catalog = request(&app, "GET", "/v1/memories", Value::Null).await;
    for id in [
        "web:example-canva-export-pages",
        "web:example-calendar-timezone",
        "web:example-notion-task-database",
    ] {
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
                .contains("Documentation-based example"),
            "{id} must distinguish documented guidance from a live account run"
        );
    }
}

#[tokio::test]
async fn calendar_recall_delivers_a_short_usable_hint_bundle() {
    for (url, keywords) in [(
        "https://calendar.google.com/calendar/u/0/r",
        vec!["recurring", "timezone", "reschedule"],
    )] {
        let app = test_app();
        let recalled = request(
            &app,
            "POST",
            "/v1/memories/recall",
            json!({"url": url, "task_category": "compose-draft", "keywords": keywords}),
        )
        .await;
        let hits = recalled["hits"].as_array().expect("hits");
        assert_eq!(hits.len(), 3, "{url}: keep all requested workflow hints");
        let chars: usize = hits
            .iter()
            .map(|hit| hit["hint"].as_str().expect("rendered L1").chars().count())
            .sum();
        assert!(chars <= 1600, "{url}: {chars} characters in the L1 bundle");
    }
}

#[test]
fn notion_row_only_tasks_do_not_require_writing_page_bodies() {
    for entry in examples::load().expect("validated examples") {
        if !entry.id.starts_with("web:example-notion-database-entry") {
            continue;
        }
        for step in &entry.payload.l2.steps {
            if step.note_code.as_deref() == Some("item-body") {
                assert_eq!(
                    step.when.as_deref(),
                    Some("body-requested"),
                    "{}: writing a page body is optional when only row properties are requested",
                    entry.id
                );
            }
        }
    }
}

#[test]
fn calendar_creation_saves_before_handling_the_invitation_dialog() {
    let loaded = examples::load().expect("validated examples");
    let entry = loaded
        .iter()
        .find(|row| row.id == "web:example-calendar-event")
        .unwrap();
    let steps = &entry.payload.l2.steps;
    let save = steps
        .iter()
        .position(|step| step.note_code.as_deref() == Some("save-button"))
        .unwrap();
    let invitation = steps
        .iter()
        .position(|step| step.note_code.as_deref() == Some("invitation-prompt"))
        .unwrap();
    assert!(
        save < invitation,
        "the invitation prompt appears after Save"
    );
}
