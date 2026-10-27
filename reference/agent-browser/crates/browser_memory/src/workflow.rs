// SPDX-License-Identifier: Apache-2.0
//! Opt-in, minimized workflow reports. No API accepts typed text, selectors, page content or results.
use crate::web::{self, Config};
use serde::{Deserialize, Serialize};
use std::{collections::{BTreeMap, HashMap}, path::{Path, PathBuf}, sync::{LazyLock, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH}};

const TOOLS: [&str; 11] = ["browser_navigate","browser_click","browser_type","browser_eval","browser_scroll",
    "browser_screenshot","browser_snapshot","browser_read","browser_find_text","browser_wait_for","browser_script"];
const MAX_ACTIONS: usize = 24;
const MAX_PENDING: usize = 100;
static UPLOADS: LazyLock<Mutex<HashMap<PathBuf, Instant>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Action { pub tool: String, pub outcome: String }
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub schema_version: u32, pub app_version: String, pub domain: String,
    pub terminal: String, pub actions: Vec<Action>,
}
impl Report {
    pub fn valid(&self) -> bool {
        self.schema_version == 1 && !self.app_version.is_empty() && self.app_version.len() <= 32
            && self.app_version.bytes().all(|c| c.is_ascii_alphanumeric() || b".+_-".contains(&c))
            && web::domain(&self.domain).as_deref() == Some(&self.domain)
            && ["completed","failed","cancelled"].contains(&self.terminal.as_str())
            && !self.actions.is_empty() && self.actions.len() <= MAX_ACTIONS
            && self.actions.iter().all(|item| TOOLS.contains(&item.tool.as_str()) && ["ok","error"].contains(&item.outcome.as_str()))
    }
}
#[derive(Default)]
pub struct Trace {
    scope: Option<String>,
    pending: HashMap<String, (String, String)>,
    steps: Vec<(String, Action)>,
}
impl Trace {
    pub fn clear(&mut self) { self.pending.clear(); self.steps.clear(); self.scope = None; }
    fn allowed(&mut self, root: &Path) -> bool {
        let cfg = Config::load(root);
        if cfg.contribute {
            if self.scope != cfg.scope() { self.clear(); self.scope = cfg.scope(); }
            return true;
        }
        self.clear();
        false
    }
    pub fn call(&mut self, root: &Path, id: &str, name: &str, site: &str) {
        if !self.allowed(root) || self.pending.len() + self.steps.len() >= MAX_ACTIONS { return; }
        let name = name.rsplit("__").next().unwrap_or(name);
        if !TOOLS.contains(&name) { return; }
        if let Some(domain) = web::domain(site) {
            self.pending.insert(id.to_string(), (domain, name.to_string()));
        }
    }
    pub fn result(&mut self, root: &Path, id: &str, error: bool) {
        if !self.allowed(root) { return; }
        if let Some((site, tool)) = self.pending.remove(id) {
            self.steps.push((site, Action { tool, outcome: if error { "error" } else { "ok" }.into() }));
        }
    }
    pub fn finish(&mut self, root: &Path, terminal: &str) {
        if !self.allowed(root) { return; }
        let mut groups: BTreeMap<String, Vec<Action>> = BTreeMap::new();
        for (site, action) in self.steps.drain(..) { groups.entry(site).or_default().push(action); }
        self.pending.clear();
        for (domain, actions) in groups {
            let report = Report { schema_version: 1, app_version: env!("CARGO_PKG_VERSION").into(), domain, terminal: terminal.into(), actions };
            if !report.valid() || !Config::load(root).contribute || pending_count(root) >= MAX_PENDING { continue; }
            let Ok(bytes) = serde_json::to_vec(&report) else { continue; };
            if bytes.len() > 2048 { continue; }
            let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
            let cfg = Config::load(root);
            if !cfg.contribute || cfg.scope() != self.scope { break; }
            let path = queue_dir(root).join(format!("{stamp}-{}.json",std::process::id()));
            let _ = web::private_write(&path, &bytes);
            // Both consent and identity can change while writing.
            let current = Config::load(root);
            if !current.contribute || current.scope() != cfg.scope() { let _ = std::fs::remove_file(path); }
        }
    }
}
pub fn queue_dir(root: &Path) -> PathBuf {
    root.join("memory/workflow-reports").join(Config::load(root).scope().unwrap_or_else(|| "signed-out".into()))
}
fn pending(root: &Path) -> Vec<PathBuf> {
    let mut files: Vec<_> = std::fs::read_dir(queue_dir(root)).into_iter().flatten().flatten()
        .map(|item|item.path()).filter(|path|path.extension().is_some_and(|ext|ext == "json")).collect();
    files.sort();
    files
}
pub fn pending_count(root: &Path) -> usize { pending(root).len() }
pub fn purge(root: &Path) {
    let _ = std::fs::remove_dir_all(root.join("memory/workflow-reports"));
}
fn send(root: &Path) -> usize {
    let mut sent = 0;
    let cfg = Config::load(root);
    if !cfg.contribute { purge(root); return 0; }
    let Ok(client) = reqwest::blocking::Client::builder().timeout(Duration::from_secs(3))
        .redirect(reqwest::redirect::Policy::none()).build() else { return 0; };
    for path in pending(root).into_iter().take(8) {
        let current = Config::load(root);
        if !current.contribute || current.scope() != cfg.scope() { break; }
        let report = (|| {
            if std::fs::metadata(&path).ok()?.len() > 2048 { return None; }
            let report: Report = serde_json::from_slice(&std::fs::read(&path).ok()?).ok()?;
            report.valid().then_some(report)
        })();
        let Some(report) = report else { let _ = std::fs::remove_file(path); continue; };
        let Some(token) = mesh_account::arise::bearer(root, &cfg.endpoint) else { break; };
        if Config::load(root).scope() != cfg.scope() { break; }
        let response = client.post(format!("{}/web-memory/v1/reports",cfg.endpoint)).bearer_auth(token).json(&report).send();
        let Ok(response) = response else { break; };
        if response.status().as_u16() == 401 {
            mesh_account::arise::revoke_local(root, &cfg.endpoint); break;
        }
        if response.status().is_success() {
            let _ = std::fs::remove_file(path);
            sent += 1;
        } else if response.status().is_client_error() && response.status().as_u16() != 429 {
            let _ = std::fs::remove_file(path);
        } else { break; }
    }
    sent
}
/// Dedicated thread also supports callers inside the agent's async runtime.
pub fn upload(root: &Path) -> usize {
    let root = root.to_path_buf();
    std::thread::spawn(move || send(&root)).join().unwrap_or(0)
}
pub fn prefetch_upload(root: &Path) {
    {
        let Ok(mut checks) = UPLOADS.lock() else { return; };
        if checks.get(root).is_some_and(|instant|instant.elapsed() < Duration::from_secs(30)) { return; }
        if checks.len() > 128 { checks.retain(|_,instant|instant.elapsed() < Duration::from_secs(30)); }
        checks.insert(root.to_path_buf(), Instant::now());
    }
    if !Config::load(root).contribute {
        purge(root);
        return;
    }
    let root = root.to_path_buf();
    std::thread::spawn(move || send(&root));
}
