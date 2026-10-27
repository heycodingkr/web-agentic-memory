//! Synthetic private metadata only: production authentication is tested by the Rust server suite.
use std::path::Path;
use serde_json::json;
pub fn login(root:&Path, endpoint:&str, user:&str) {
    let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
    std::fs::write(mesh_account::arise::state_file(root,endpoint), serde_json::to_vec(&json!({
        "access":"member-fixture-token","refresh":"fixture-refresh",
        "expiresAt":now+3600000,"userId":user
    })).unwrap()).unwrap();
}
