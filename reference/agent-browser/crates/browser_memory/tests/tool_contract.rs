// SPDX-License-Identifier: Apache-2.0
//! docs/specs/browser-memory.md — shared tool definitions (API agent, stdio MCP, CLI allowlists).
use browser_memory::tools;
use serde_json::json;

#[test]
fn four_tools_with_strict_schemas() {
    assert_eq!(tools::NAMES, ["browser_memory_recall", "browser_memory_save", "browser_memory_expand", "browser_memory_forget"]);
    for name in tools::NAMES {
        let schema = tools::schema(name);
        assert_eq!(schema["type"], "object", "{name}");
        assert_eq!(schema["additionalProperties"], false, "{name}");
        assert!(!tools::description(name).is_empty(), "{name}");
    }
    assert_eq!(tools::schema(tools::SAVE)["required"], json!(["kind", "title", "summary"]));
    assert_eq!(tools::schema(tools::EXPAND)["required"], json!(["id", "level"]));
    assert_eq!(tools::schema(tools::FORGET)["required"], json!(["ids"]));
    assert_eq!(tools::schema(tools::SAVE)["properties"]["kind"]["enum"], json!(["workflow", "site_note", "gotcha", "preference"]));
    assert_eq!(tools::schema(tools::EXPAND)["properties"]["level"]["enum"], json!([2, 3]));
}

#[test]
fn definitions_stay_small_because_they_are_paid_for_every_turn() {
    let total: usize = tools::NAMES.iter().map(|n| tools::description(n).len() + tools::schema(n).to_string().len()).sum();
    assert!(total <= 2000, "tool definitions cost too much context: {total} bytes");
}

#[test]
fn save_description_routes_credentials_to_the_vault_and_marks_recall_untrusted() {
    assert!(tools::description(tools::SAVE).contains("browser_save_account"));
    assert!(tools::description(tools::SAVE).to_lowercase().contains("never"));
    let recall = tools::description(tools::RECALL).to_lowercase();
    assert!(recall.contains("verify") || recall.contains("untrusted"));
}

#[test]
fn default_dir_follows_the_app_data_convention() {
    let dir = tools::default_dir_with(&|k| match k {
        "XDG_CONFIG_HOME" => Some("/x/cfg".into()),
        _ => None,
    });
    assert_eq!(dir, std::path::PathBuf::from("/x/cfg/agent-browser"));
    let home = tools::default_dir_with(&|k| (k == "HOME").then(|| "/h".into()));
    #[cfg(target_os = "macos")]
    assert_eq!(home, std::path::PathBuf::from("/h/Library/Application Support/agent-browser"));
    #[cfg(not(target_os = "macos"))]
    assert_eq!(home, std::path::PathBuf::from("/h/.config/agent-browser"));
}

#[test]
fn dispatch_ignores_only_the_transport_session_id() {
    let dir = tempfile::tempdir().unwrap();
    let with_session = json!({"query":"x","session_id":"abc"});
    assert!(tools::dispatch(dir.path(), tools::RECALL, &with_session).is_ok());
    assert!(tools::dispatch(dir.path(), tools::RECALL, &json!({"query":"x","other":1})).is_err());
}

#[test]
fn model_facing_text_is_compact_lines_not_a_json_envelope() {
    let dir = tempfile::tempdir().unwrap();
    let call = |name: &str, args: serde_json::Value| tools::call_text(dir.path(), name, &args);
    let a = call(tools::SAVE, json!({"kind":"gotcha","title":"Search needs scroll","summary":"results lazy-load","site":"https://shop.example.test/s?q=1"})).unwrap();
    assert_eq!(a, "saved #1");
    assert_eq!(call(tools::SAVE, json!({"kind":"gotcha","title":"Search needs scroll","summary":"results lazy-load","site":"shop.example.test"})).unwrap(), "already saved #1");
    assert_eq!(call(tools::SAVE, json!({"kind":"gotcha","title":"Search needs scroll","summary":"results lazy-load","site":"shop.example.test","detail":"scroll twice"})).unwrap(), "already saved #1 (layers updated)");
    assert_eq!(call(tools::SAVE, json!({"kind":"gotcha","title":"Search needs two scrolls","summary":"results lazy-load twice","replaces":1})).unwrap(), "saved #2 (replaced #1)");
    let recalled = call(tools::RECALL, json!({"query":"scroll"})).unwrap();
    assert!(recalled.starts_with("#2 gotcha - | Search needs two scrolls"), "{recalled}");
    assert!(!recalled.contains('{') && !recalled.contains("\"ok\""), "{recalled}");
    assert_eq!(call(tools::FORGET, json!({"ids":[2]})).unwrap(), "forgotten #2");
    assert_eq!(call(tools::FORGET, json!({"ids":[2]})).unwrap(), "not found #2");
    assert_eq!(call(tools::FORGET, json!({"ids":[2, 3]})).unwrap(), "not found #2 #3");
    assert_eq!(call(tools::RECALL, json!({"query":"scroll"})).unwrap(), "no matching memory");
}

#[test]
fn guidance_is_one_short_shared_text_in_both_languages() {
    // Sent every turn (CLI argv + API system prompt): the tool descriptions carry the details.
    for locale in [i18n::Locale::En, i18n::Locale::Ko] {
        let g = i18n::with_locale(locale, tools::guidance);
        for name in [tools::RECALL, tools::SAVE, "browser_save_account"] {
            assert!(g.contains(name), "{locale:?} guidance misses {name}");
        }
        assert!(g.chars().count() <= 260, "guidance is paid for every turn: {} chars", g.chars().count());
    }
    assert!(i18n::with_locale(i18n::Locale::En, tools::guidance).contains("untrusted"));
    assert!(i18n::with_locale(i18n::Locale::Ko, tools::guidance).contains("신뢰할 수 없다"));
}
