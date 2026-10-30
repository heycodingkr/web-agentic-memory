mod lifecycle_support;
use lifecycle_support::{PassingReplay, now};
use std::time::{Duration, Instant};
use wam_gatekeeper::tools::{Candidate, JobScope, Outcomes};
use wam_server::durable::{CommandReplayVerifier, ReplayRequest, ReplayVerifier};

fn request() -> ReplayRequest {
    let mut value = ReplayRequest {
        schema_version: 1,
        scope: JobScope {
            site_host: "example.org".into(),
            task_category: "compose-draft".into(),
            intent_key: "compose-draft".into(),
        },
        candidate: Candidate {
            url_pattern: "/editor".into(),
            action_sequence: vec!["navigate".into(), "type".into(), "click".into()],
            outcomes: Outcomes {
                success: 3,
                partial: 0,
                failed: 0,
            },
            observed_traits: vec![],
            independent_contributors: 1,
            ui_fingerprint: None,
            observed_at: now(),
        },
        baseline: None,
        proposed_record: None,
        input_digest: String::new(),
    };
    value.input_digest = wam_gatekeeper::record::sha256_hex(&serde_json::to_vec(&value).unwrap());
    value
}
#[test]
fn replay_process_uses_closed_json_protocol_and_enforces_deadline_and_output_bound() {
    let input = request();
    let evidence = PassingReplay.verify(&input).unwrap();
    let script = format!(
        "import sys,json; data=json.load(sys.stdin); result={}; result['input_digest']=data['input_digest']; print(json.dumps(result))",
        serde_json::to_string(&evidence).unwrap()
    );
    let verifier =
        CommandReplayVerifier::new("python3", vec!["-c".into(), script], Duration::from_secs(2))
            .unwrap();
    assert_eq!(
        verifier.verify(&input).unwrap().input_digest,
        input.input_digest
    );
    let oversized = CommandReplayVerifier::new(
        "python3",
        vec!["-c".into(), "print('x'*70000)".into()],
        Duration::from_secs(2),
    )
    .unwrap();
    assert!(oversized.verify(&input).is_err());
    let stalled = CommandReplayVerifier::new(
        "python3",
        vec!["-c".into(), "import time;time.sleep(2)".into()],
        Duration::from_millis(80),
    )
    .unwrap();
    let start = Instant::now();
    assert!(stalled.verify(&input).is_err());
    assert!(start.elapsed() < Duration::from_secs(1));
}
