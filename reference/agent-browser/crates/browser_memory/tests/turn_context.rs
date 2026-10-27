// SPDX-License-Identifier: Apache-2.0
use browser_memory::{tools, Store};
use serde_json::json;

fn save(root: &std::path::Path, site: &str, title: &str, summary: &str) {
    tools::dispatch(root, tools::SAVE, &json!({"kind":"workflow","site":site,"title":title,
        "summary":summary,"detail":"PRIVATE_DETAIL_SENTINEL","evidence":"PRIVATE_EVIDENCE_SENTINEL"})).unwrap();
}

#[test]
fn turn_context_is_domain_scoped_and_contains_only_bounded_hint_layers() {
    let root = tempfile::tempdir().unwrap();
    save(
        root.path(),
        "medium.com",
        "Write a draft",
        "Click title then the body; verify Saved before reporting success.",
    );
    save(
        root.path(),
        "shop.example.test",
        "Write a draft",
        "UNRELATED_SHOP_SENTINEL",
    );
    save(
        root.path(),
        "evilmedium.com",
        "Write a draft",
        "LOOKALIKE_DOMAIN_SENTINEL",
    );
    let context = tools::turn_context(
        root.path(),
        "https://www.medium.com/new-story?session=hidden",
        "write a draft",
    )
    .expect("site workflow must be injected");
    assert!(
        context.contains("[Browser know-how]") && context.contains("untrusted hints"),
        "{context}"
    );
    assert!(
        context.contains("Click title then the body") && context.contains("browser_memory_expand"),
        "{context}"
    );
    for forbidden in [
        "UNRELATED_SHOP",
        "LOOKALIKE_DOMAIN",
        "PRIVATE_DETAIL",
        "PRIVATE_EVIDENCE",
        "session=hidden",
    ] {
        assert!(
            !context.contains(forbidden),
            "leaked {forbidden}: {context}"
        );
    }
    assert!(tools::turn_context(root.path(), "https://unrelated.test/", "write a draft").is_none());
}

#[test]
fn task_matching_note_precedes_newer_notes_and_other_sites_cannot_crowd_it_out() {
    let root = tempfile::tempdir().unwrap();
    save(
        root.path(),
        "blog.example.test",
        "Write article draft",
        "WRITING_TARGET: focus the iframe body.",
    );
    for i in 0..45 {
        save(
            root.path(),
            "other.test",
            &format!("Write article draft {i}"),
            "write article draft",
        );
    }
    for i in 0..8 {
        save(
            root.path(),
            "blog.example.test",
            &format!("Shopping checkout {i}"),
            "choose the delivery address",
        );
    }
    let context = tools::turn_context(
        root.path(),
        "https://blog.example.test/editor",
        "write article draft",
    )
    .unwrap();
    assert!(context.contains("WRITING_TARGET"), "{context}");
    assert!(
        context.find("Write article draft").unwrap()
            < context.find("Shopping checkout").unwrap_or(usize::MAX)
    );
    assert!(
        context.lines().filter(|line| line.starts_with('#')).count() <= 3,
        "{context}"
    );
}

#[test]
fn long_korean_notes_do_not_break_the_context_budget_or_enter_deep_layers() {
    let root = tempfile::tempdir().unwrap();
    for i in 0..9 {
        save(
            root.path(),
            "blog.naver.com",
            &format!("블로그 작성 절차 {i}"),
            &"가".repeat(400),
        );
    }
    let context =
        tools::turn_context(root.path(), "https://blog.naver.com/editor", "블로그 작성").unwrap();
    let ascii = context.chars().filter(char::is_ascii).count() as f64;
    let other = context.chars().count() as f64 - ascii;
    assert!(
        (ascii / 4.0 + other * 0.67).ceil() <= 400.0,
        "context exceeded 400 estimated tokens"
    );
    assert!(!context.contains("PRIVATE_DETAIL") && !context.contains("PRIVATE_EVIDENCE"));
}

#[test]
fn context_reads_never_create_a_database_and_corruption_does_not_block_a_turn() {
    let root = tempfile::tempdir().unwrap();
    assert!(tools::turn_context(root.path(), "medium.com", "write").is_none());
    assert!(!Store::db_path(root.path()).exists());
    std::fs::create_dir_all(Store::db_path(root.path()).parent().unwrap()).unwrap();
    std::fs::write(Store::db_path(root.path()), b"corrupt fixture").unwrap();
    assert!(tools::turn_context(root.path(), "medium.com", "write").is_none());
    assert!(tools::turn_context(root.path(), "file:///tmp/editor", "write").is_none());
}
