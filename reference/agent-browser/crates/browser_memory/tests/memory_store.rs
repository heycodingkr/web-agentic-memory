// SPDX-License-Identifier: Apache-2.0
//! docs/specs/browser-memory.md — store behavior through the tool contract.
use browser_memory::{Store, tools};
use serde_json::{Value, json};
use std::path::Path;

fn call(dir: &Path, name: &str, args: Value) -> Result<Value, String> {
    tools::dispatch(dir, name, &args)
}
fn save(dir: &Path, args: Value) -> Result<Value, String> {
    call(dir, "browser_memory_save", args)
}
fn recall(dir: &Path, args: Value) -> Result<Value, String> {
    call(dir, "browser_memory_recall", args)
}
fn entry(title: &str, summary: &str) -> Value {
    json!({"kind":"workflow","title":title,"summary":summary})
}
fn counts(dir: &Path) -> (i64, i64) {
    Store::open(dir).unwrap().counts().unwrap()
}
fn text(v: &Value) -> String {
    v["text"].as_str().unwrap_or_default().to_string()
}
fn raw_db_bytes(dir: &Path) -> Vec<u8> {
    let db = Store::db_path(dir);
    let mut all = std::fs::read(&db).unwrap_or_default();
    for suffix in ["-wal", "-shm"] {
        let mut p = db.clone().into_os_string();
        p.push(suffix);
        all.extend(std::fs::read(p).unwrap_or_default());
    }
    all
}
fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle.as_bytes())
}

#[test]
fn save_normalizes_site_and_strips_query_and_fragment_everywhere() {
    let dir = tempfile::tempdir().unwrap();
    let r = save(dir.path(), json!({
        "kind":"workflow","title":"Naver login is two-step",
        "summary":"ID first then password; see https://nid.naver.com/a?token_x=zzz1#frag1",
        "detail":"open https://nid.naver.com/b?detailq=qq2#frag2",
        "evidence":"seen at https://nid.naver.com/c?evq=ee3#frag3",
        "site":"https://WWW.nid.naver.com:443/nidlogin.login?mode=form&x=1#top",
        "tags":["Login","naver"]
    })).unwrap();
    assert_eq!(r["ok"], true);
    assert!(r["id"].as_i64().unwrap() >= 1);
    let bytes = raw_db_bytes(dir.path());
    for leaked in ["mode=form", "#top", "token_x", "frag1", "detailq", "frag2", "evq", "frag3"] {
        assert!(!contains(&bytes, leaked), "{leaked} leaked into the database file");
    }
    let c = rusqlite::Connection::open(Store::db_path(dir.path())).unwrap();
    let site: String = c.query_row("select site from memory", [], |r| r.get(0)).unwrap();
    assert_eq!(site, "nid.naver.com");
}

#[test]
fn recall_returns_only_l0_and_l1() {
    let dir = tempfile::tempdir().unwrap();
    save(dir.path(), json!({"kind":"workflow","title":"Naver login is two-step","summary":"Fill ID, continue, then password.",
        "detail":"DETAIL_SENTINEL_9f3","evidence":"EVIDENCE_SENTINEL_77a","site":"nid.naver.com"})).unwrap();
    let r = recall(dir.path(), json!({"query":"naver login"})).unwrap();
    assert_eq!(r["n"], 1);
    let t = text(&r);
    assert!(t.starts_with('#'), "{t}");
    assert!(t.contains("Naver login is two-step") && t.contains("Fill ID, continue"), "{t}");
    assert!(!t.contains("SENTINEL"), "{t}");
    assert!(r["tokens"].as_u64().unwrap() > 0);
}

#[test]
fn recall_defaults_to_five_clamps_to_ten_and_respects_token_budget() {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..30 {
        save(dir.path(), entry(&format!("naver login variant {i}"), &format!("variant {i} of the login flow {}", "x".repeat(300)))).unwrap();
    }
    assert_eq!(recall(dir.path(), json!({"query":"naver login"})).unwrap()["n"], 5);
    let big = recall(dir.path(), json!({"query":"naver login","k":99})).unwrap();
    assert!(big["n"].as_u64().unwrap() <= 10);
    assert!(big["tokens"].as_u64().unwrap() <= 600, "budget exceeded: {}", big["tokens"]);
    assert!(text(&big).contains("more"), "truncation must be visible");
}

#[test]
fn same_site_entry_outranks_equally_relevant_other_site_entry() {
    let dir = tempfile::tempdir().unwrap();
    let a = save(dir.path(), json!({"kind":"site_note","title":"checkout flow","summary":"needs address first","site":"a.example.test"})).unwrap()["id"].as_i64().unwrap();
    let b = save(dir.path(), json!({"kind":"site_note","title":"checkout flow","summary":"needs address first","site":"b.example.test"})).unwrap()["id"].as_i64().unwrap();
    for (site, want) in [("https://b.example.test/cart?x=1", b), ("a.example.test", a)] {
        let r = recall(dir.path(), json!({"query":"checkout flow","site":site})).unwrap();
        assert!(text(&r).starts_with(&format!("#{want} ")), "site {site}: {}", text(&r));
    }
}

#[test]
fn site_without_query_lists_the_site_index_only() {
    let dir = tempfile::tempdir().unwrap();
    for t in ["login two-step", "captcha after search"] {
        save(dir.path(), json!({"kind":"gotcha","title":t,"summary":"s","site":"nid.naver.com"})).unwrap();
    }
    save(dir.path(), json!({"kind":"site_note","title":"naver-wide note","summary":"s","site":"naver.com"})).unwrap();
    save(dir.path(), json!({"kind":"site_note","title":"coupang note","summary":"s","site":"coupang.com"})).unwrap();
    save(dir.path(), entry("global note without site", "s")).unwrap();
    let r = recall(dir.path(), json!({"site":"https://nid.naver.com/login?x=1"})).unwrap();
    let t = text(&r);
    assert_eq!(r["n"], 3, "{t}");
    assert!(t.contains("login two-step") && t.contains("captcha after search") && t.contains("naver-wide note"), "{t}");
    assert!(!t.contains("coupang") && !t.contains("global note"), "{t}");
    assert!(recall(dir.path(), json!({})).is_err(), "query or site is required");
}

#[test]
fn korean_and_english_stemming_queries_find_entries() {
    let dir = tempfile::tempdir().unwrap();
    save(dir.path(), entry("쿠팡 로그인은 2단계", "아이디 다음에 비밀번호를 입력한다")).unwrap();
    save(dir.path(), entry("Scrolling needed before results render", "lazy list")).unwrap();
    assert_eq!(recall(dir.path(), json!({"query":"쿠팡 로그인"})).unwrap()["n"], 1);
    assert_eq!(recall(dir.path(), json!({"query":"로그인을 하려면"})).unwrap()["n"], 1, "particles on the query side must not break recall");
    assert_eq!(recall(dir.path(), json!({"query":"결제"})).unwrap()["n"], 0);
    assert_eq!(recall(dir.path(), json!({"query":"scroll results"})).unwrap()["n"], 1);
    assert_eq!(recall(dir.path(), json!({"query":"\"; DROP TABLE memory; -- NEAR( ) *"})).unwrap()["n"], 0, "FTS syntax in queries must be neutralized");
}

#[test]
fn identical_save_is_deduped_and_index_stays_in_sync() {
    let dir = tempfile::tempdir().unwrap();
    let first = save(dir.path(), entry("Same title", "Same summary")).unwrap();
    let second = save(dir.path(), entry("  same TITLE ", "Same   summary")).unwrap();
    assert_eq!(second["deduped"], true);
    assert_eq!(first["id"], second["id"]);
    assert_eq!(counts(dir.path()), (1, 1));
}

#[test]
fn forget_removes_row_and_index_and_reports_missing() {
    let dir = tempfile::tempdir().unwrap();
    let ids: Vec<i64> = (0..3).map(|i| save(dir.path(), entry(&format!("alpha entry {i}"), "alpha")).unwrap()["id"].as_i64().unwrap()).collect();
    let r = call(dir.path(), "browser_memory_forget", json!({"ids":[ids[0], 999]})).unwrap();
    assert_eq!(r["forgotten"], json!([ids[0]]));
    assert_eq!(r["missing"], json!([999]));
    let t = text(&recall(dir.path(), json!({"query":"alpha entry","k":10})).unwrap());
    assert!(!t.contains(&format!("#{} ", ids[0])), "{t}");
    assert_eq!(counts(dir.path()), (2, 2));
}

#[test]
fn replaces_swaps_a_stale_entry_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let old = save(dir.path(), entry("Old login flow", "one step")).unwrap()["id"].as_i64().unwrap();
    let new = save(dir.path(), json!({"kind":"workflow","title":"New login flow","summary":"two steps","replaces":old})).unwrap();
    assert_eq!(new["replaced"], old);
    assert_eq!(counts(dir.path()), (1, 1));
    let t = text(&recall(dir.path(), json!({"query":"login flow"})).unwrap());
    assert!(t.contains("New login flow") && !t.contains("Old login flow"), "{t}");
    let err = save(dir.path(), json!({"kind":"workflow","title":"Another","summary":"x","replaces":424242})).unwrap_err();
    assert!(err.contains("not found"), "{err}");
    assert_eq!(counts(dir.path()), (1, 1), "failed replace must not write");
}

#[test]
fn expand_returns_requested_layer_only() {
    let dir = tempfile::tempdir().unwrap();
    let id = save(dir.path(), json!({"kind":"workflow","title":"Deep entry","summary":"short","detail":"L2_TEXT_here","evidence":"L3_TEXT_here"})).unwrap()["id"].as_i64().unwrap();
    let l2 = text(&call(dir.path(), "browser_memory_expand", json!({"id":id,"level":2})).unwrap());
    assert!(l2.contains("Deep entry") && l2.contains("L2_TEXT_here") && !l2.contains("L3_TEXT_here"), "{l2}");
    let l3 = text(&call(dir.path(), "browser_memory_expand", json!({"id":id,"level":3})).unwrap());
    assert!(l3.contains("L3_TEXT_here") && !l3.contains("L2_TEXT_here"), "{l3}");
    for bad in [json!({"id":id,"level":1}), json!({"id":id,"level":4}), json!({"id":id}), json!({"level":2})] {
        assert!(call(dir.path(), "browser_memory_expand", bad).is_err());
    }
    assert!(call(dir.path(), "browser_memory_expand", json!({"id":777,"level":2})).unwrap_err().contains("not found"));
}

#[test]
fn invalid_input_is_rejected_without_writing_or_echoing() {
    let dir = tempfile::tempdir().unwrap();
    let canary = "CANARY_ECHO_CHECK";
    let long = |n: usize| format!("{canary}{}", "a".repeat(n));
    let cases = vec![
        json!({"kind":"workflow","title":"","summary":canary}),
        json!({"kind":"workflow","title":long(120),"summary":"s"}),
        json!({"kind":"workflow","title":"t","summary":long(400)}),
        json!({"kind":"workflow","title":"t","summary":"s","detail":long(8000)}),
        json!({"kind":"workflow","title":"t","summary":"s","evidence":long(16000)}),
        json!({"kind":canary,"title":"t","summary":"s"}),
        json!({"kind":"workflow","title":"t","summary":"s",canary:1}),
        json!({"kind":"workflow","title":"t","summary":"s","tags":canary}),
        json!({"kind":"workflow","title":"t","summary":"s","tags":["1","2","3","4","5","6","7","8","9"]}),
        json!({"kind":"workflow","title":1,"summary":canary}),
        json!({"kind":"workflow","title":"t","summary":"s","site":"ftp://x.example.test"}),
        json!({"kind":"workflow","title":"t","summary":"s","replaces":"1"}),
    ];
    for case in cases {
        let err = save(dir.path(), case.clone()).unwrap_err();
        assert!(!err.contains(canary), "error echoed input for {case}: {err}");
    }
    assert!(call(dir.path(), "browser_memory_forget", json!({"ids":[]})).is_err());
    assert!(call(dir.path(), "browser_memory_forget", json!({"ids":(1..=21).collect::<Vec<i64>>()})).is_err());
    assert!(recall(dir.path(), json!({"query":"x","kind":"nope"})).is_err());
    assert!(call(dir.path(), "browser_memory_nope", json!({})).is_err());
    assert_eq!(counts(dir.path()), (0, 0));
}

#[test]
fn secrets_are_rejected_in_every_text_field_without_echo() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = [
        "password=hunter2",
        "Password: correct-horse-battery",
        "Authorization: Bearer abcdefghijklmnopqrstuvwxyz0123456789",
        "token eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dBjftJeZ4CVPmB92K27uhbUJU1p1r_wW1gFWFOEjXk",
        "key sk-abcdefghijklmnopqrstuvwxyz123456",
        "card 4111 1111 1111 1111",
        "card 4111-1111-1111-1111",
        "-----BEGIN PRIVATE KEY-----",
        "blob AbCdEf0123456789AbCdEf0123456789AbCdEf0123456789",
    ];
    for secret in secrets {
        for field in ["title", "summary", "detail", "evidence"] {
            let mut args = entry("t", "s");
            args[field] = json!(secret);
            let err = save(dir.path(), args).unwrap_err();
            assert!(err.to_lowercase().contains("secret"), "{field}/{secret}: {err}");
            assert!(!err.contains(secret) && !err.contains("hunter2"), "{err}");
        }
        let mut args = entry("t", "s");
        args["tags"] = json!([secret]);
        assert!(save(dir.path(), args).is_err(), "tag {secret}");
    }
    assert_eq!(counts(dir.path()), (0, 0));
    for fine in ["login page has a password step", "order number 1234 5678 9012 3450", "see https://example.test/login", "use selector #login-form input[name=id]"] {
        save(dir.path(), entry(fine, "ok")).unwrap_or_else(|e| panic!("{fine}: {e}"));
    }
}

#[test]
fn capacity_refuses_new_entries_but_allows_replacement() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open_with_capacity(dir.path(), 2).unwrap();
    let a = store.save(&entry("one", "s")).unwrap();
    store.save(&entry("two", "s")).unwrap();
    let err = store.save(&entry("three", "s")).unwrap_err().to_string();
    assert!(err.contains("full"), "{err}");
    let mut replace = entry("three", "s");
    replace["replaces"] = a["id"].clone();
    store.save(&replace).unwrap();
    assert_eq!(store.counts().unwrap(), (2, 2));
}

#[test]
fn two_connections_write_concurrently() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_path_buf();
    let handles: Vec<_> = (0..2).map(|w| {
        let p = path.clone();
        std::thread::spawn(move || {
            for i in 0..25 {
                save(&p, entry(&format!("writer{w} note{i}"), "concurrent")).unwrap();
            }
        })
    }).collect();
    for h in handles { h.join().unwrap(); }
    assert_eq!(counts(&path), (50, 50));
    assert_eq!(recall(&path, json!({"query":"concurrent writer0","k":10})).unwrap()["n"], 10);
}

#[test]
fn corrupt_file_is_an_error_not_a_panic_and_missing_file_is_created() {
    let dir = tempfile::tempdir().unwrap();
    let db = Store::db_path(dir.path());
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    std::fs::write(&db, b"this is not a sqlite database at all, just text".repeat(50)).unwrap();
    assert!(save(dir.path(), entry("t", "s")).is_err());
    let fresh = tempfile::tempdir().unwrap();
    assert!(!Store::db_path(fresh.path()).exists());
    save(fresh.path(), entry("t", "s")).unwrap();
    assert!(Store::db_path(fresh.path()).exists());
}

#[test]
fn recall_on_an_empty_store_is_cheap_and_does_not_create_noise() {
    let dir = tempfile::tempdir().unwrap();
    let r = recall(dir.path(), json!({"query":"anything"})).unwrap();
    assert_eq!(r["n"], 0);
    assert!(r["tokens"].as_u64().unwrap() <= 12, "{r}");
}

#[test]
fn four_writers_racing_on_a_fresh_store_never_see_a_lock_error() {
    // Cold start: every writer opens the store for the first time at once (journal-mode switch and
    // schema creation race). SQLite answers some of those with an immediate BUSY — the store retries.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_path_buf();
    let start = std::sync::Arc::new(std::sync::Barrier::new(4));
    let handles: Vec<_> = (0..4).map(|w| {
        let (p, start) = (path.clone(), start.clone());
        std::thread::spawn(move || {
            start.wait();
            for i in 0..15 {
                save(&p, entry(&format!("racer{w} note{i}"), "cold start")).unwrap();
                recall(&p, json!({"query":"cold start"})).unwrap();
            }
        })
    }).collect();
    for h in handles { h.join().unwrap(); }
    assert_eq!(counts(&path), (60, 60));
}

#[test]
fn saving_the_same_note_with_richer_layers_enriches_it_instead_of_dropping_them() {
    let dir = tempfile::tempdir().unwrap();
    let first = save(dir.path(), entry("Login quirk", "second click needed")).unwrap();
    let id = first["id"].as_i64().unwrap();
    let richer = save(dir.path(), json!({"kind":"workflow","title":"Login quirk","summary":"second click needed","detail":"1) click  2) click again","evidence":"seen on 2026-10-16"})).unwrap();
    assert_eq!((richer["deduped"].clone(), richer["updated"].clone(), richer["id"].as_i64()), (json!(true), json!(true), Some(id)));
    let l2 = text(&call(dir.path(), "browser_memory_expand", json!({"id":id,"level":2})).unwrap());
    assert!(l2.contains("1) click  2) click again"), "{l2}");
    let l3 = text(&call(dir.path(), "browser_memory_expand", json!({"id":id,"level":3})).unwrap());
    assert!(l3.contains("seen on 2026-10-16"), "{l3}");
    // Identical again: nothing to update. A save without layers never erases stored ones.
    let again = save(dir.path(), json!({"kind":"workflow","title":"Login quirk","summary":"second click needed","detail":"1) click  2) click again"})).unwrap();
    assert!(again.get("updated").is_none(), "{again}");
    let bare = save(dir.path(), entry("Login quirk", "second click needed")).unwrap();
    assert!(bare.get("updated").is_none(), "{bare}");
    assert!(text(&call(dir.path(), "browser_memory_expand", json!({"id":id,"level":2})).unwrap()).contains("click again"));
    assert_eq!(counts(dir.path()), (1, 1));
}
