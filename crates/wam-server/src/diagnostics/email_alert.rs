use super::types::now_micros;
use super::{Diagnostics, ErrorCode, StoredDiagnostic};
use crate::identity::email::{ResendEmailSender, normalize_email};
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashSet, VecDeque},
    env,
    io::{self, ErrorKind},
    sync::Arc,
    time::Duration,
};

const POLL_INTERVAL: Duration = Duration::from_secs(60);
const STARTUP_LOOKBACK: u64 = 10 * 60;
const MAX_RECENT: usize = 1000;
const MAX_PENDING: usize = 5000;
const BATCH_SIZE: usize = 50;

#[async_trait]
trait AlertMailer: Send + Sync + 'static {
    async fn send(
        &self,
        recipient: &str,
        batch_id: &str,
        subject: &str,
        text: &str,
    ) -> Result<(), AlertDeliveryError>;
}

#[derive(Debug)]
struct AlertDeliveryError;

struct ResendAlertMailer(ResendEmailSender);

#[async_trait]
impl AlertMailer for ResendAlertMailer {
    async fn send(
        &self,
        recipient: &str,
        batch_id: &str,
        subject: &str,
        text: &str,
    ) -> Result<(), AlertDeliveryError> {
        self.0
            .send_diagnostic_alert(recipient, batch_id, subject, text)
            .await
            .map_err(|_| AlertDeliveryError)
    }
}

/// Optional email delivery for closed operational diagnostics.
pub struct DiagnosticsEmailAlerts {
    recipient: String,
    mailer: Arc<dyn AlertMailer>,
}

impl DiagnosticsEmailAlerts {
    /// Email alerts are off unless a recipient is configured. When enabled, the
    /// existing Resend key and sender address are required.
    pub fn from_env() -> Result<Option<Self>, io::Error> {
        let recipient = match env::var("WAM_DIAGNOSTICS_EMAIL_TO") {
            Ok(value) => value,
            Err(env::VarError::NotPresent) => return Ok(None),
            Err(env::VarError::NotUnicode(_)) => {
                return Err(config_error("WAM_DIAGNOSTICS_EMAIL_TO is invalid"));
            }
        };
        let recipient = normalize_email(&recipient)
            .map_err(|_| config_error("WAM_DIAGNOSTICS_EMAIL_TO is invalid"))?;
        let api_key = required_env("RESEND_API_KEY")?;
        let from = required_env("WAM_EMAIL_FROM")?;
        let sender = ResendEmailSender::new(api_key, from)
            .map_err(|_| config_error("diagnostic email sender configuration is invalid"))?;
        Ok(Some(Self {
            recipient,
            mailer: Arc::new(ResendAlertMailer(sender)),
        }))
    }

    pub fn start(self, diagnostics: Diagnostics) -> tokio::task::JoinHandle<()> {
        tokio::spawn(run(diagnostics, self.mailer, self.recipient, POLL_INTERVAL))
    }
}

fn required_env(name: &'static str) -> Result<String, io::Error> {
    env::var(name).map_err(|_| {
        config_error(match name {
            "RESEND_API_KEY" => "RESEND_API_KEY is required for diagnostic email",
            _ => "WAM_EMAIL_FROM is required for diagnostic email",
        })
    })
}

fn config_error(message: &'static str) -> io::Error {
    io::Error::new(ErrorKind::InvalidInput, message)
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct AlertCursor {
    received_at_micros: u64,
    event_id: String,
}

impl AlertCursor {
    fn after(event: &StoredDiagnostic) -> Self {
        Self {
            received_at_micros: event.ordering_timestamp_micros(),
            event_id: event.event_id.clone(),
        }
    }

    fn from_checkpoint(event: &StoredDiagnostic) -> Self {
        Self {
            received_at_micros: event.ordering_timestamp_micros(),
            event_id: event.trace_id.clone(),
        }
    }

    fn precedes(&self, event: &StoredDiagnostic) -> bool {
        (event.ordering_timestamp_micros(), event.event_id.as_str())
            > (self.received_at_micros, self.event_id.as_str())
    }
}

async fn run(
    diagnostics: Diagnostics,
    mailer: Arc<dyn AlertMailer>,
    recipient: String,
    poll_interval: Duration,
) {
    let mut interval = tokio::time::interval(poll_interval);
    let mut cursor: Option<AlertCursor> = None;
    let mut initialized = false;
    let mut pending = VecDeque::<StoredDiagnostic>::new();
    let mut pending_ids = HashSet::<String>::new();

    loop {
        interval.tick().await;
        let store = diagnostics.clone();
        let recent = tokio::task::spawn_blocking(move || store.recent(MAX_RECENT)).await;
        let Ok(Ok(mut events)) = recent else {
            continue;
        };

        if !initialized {
            cursor = latest_checkpoint(&events);
            if cursor.is_none() {
                let baseline_micros =
                    now_micros().saturating_sub(STARTUP_LOOKBACK.saturating_mul(1_000_000));
                let baseline = AlertCursor {
                    received_at_micros: baseline_micros,
                    event_id: "0".repeat(32),
                };
                diagnostics.record(StoredDiagnostic::email_checkpoint_at(
                    baseline.received_at_micros,
                    &baseline.event_id,
                ));
                cursor = Some(baseline);
            }
            initialized = true;
        }
        if let Some(checkpoint) = latest_checkpoint(&events)
            && cursor.as_ref().is_none_or(|current| checkpoint > *current)
        {
            cursor = Some(checkpoint);
            pending.retain(|event| {
                cursor
                    .as_ref()
                    .is_some_and(|current| current.precedes(event))
            });
            pending_ids = pending.iter().map(|event| event.event_id.clone()).collect();
        }

        events.sort_by(|left, right| {
            (left.ordering_timestamp_micros(), &left.event_id)
                .cmp(&(right.ordering_timestamp_micros(), &right.event_id))
        });
        for event in events {
            if !is_alertable(&event)
                || cursor
                    .as_ref()
                    .is_some_and(|current| !current.precedes(&event))
                || pending_ids.contains(&event.event_id)
                || pending.len() >= MAX_PENDING
            {
                continue;
            }
            pending_ids.insert(event.event_id.clone());
            pending.push_back(event);
        }

        if pending.is_empty() {
            continue;
        }
        let batch = pending.iter().take(BATCH_SIZE).cloned().collect::<Vec<_>>();
        let (batch_id, subject, text) = format_batch(&batch);
        if mailer
            .send(&recipient, &batch_id, &subject, &text)
            .await
            .is_err()
        {
            // Alert delivery failures are retried with the same batch key and do not recurse.
            eprintln!("diagnostic email delivery deferred");
            continue;
        }

        let Some(last) = batch.last() else {
            continue;
        };
        cursor = Some(AlertCursor::after(last));
        diagnostics.record(StoredDiagnostic::email_checkpoint(last));
        for _ in 0..batch.len() {
            if let Some(event) = pending.pop_front() {
                pending_ids.remove(&event.event_id);
            }
        }
    }
}

fn latest_checkpoint(events: &[StoredDiagnostic]) -> Option<AlertCursor> {
    events
        .iter()
        .filter(|event| event.kind == "email_checkpoint")
        .map(AlertCursor::from_checkpoint)
        .max()
}

fn is_alertable(event: &StoredDiagnostic) -> bool {
    let Some(error) = &event.error_code else {
        return false;
    };
    !(error == &ErrorCode::HttpError
        && event
            .status_code
            .is_some_and(|status| (400..500).contains(&status)))
}

fn format_batch(events: &[StoredDiagnostic]) -> (String, String, String) {
    let mut hash = Sha256::new();
    hash.update(b"wam-diagnostics-email-v1:");
    for event in events {
        hash.update(event.event_id.as_bytes());
        hash.update(b"\n");
    }
    let batch_id = format!("{:x}", hash.finalize());
    let subject = format!("WAM error alert: {} event(s)", events.len());
    let mut text = format!(
        "Web Agentic Memory recorded {} actionable error event(s).\n\nHTTP status describes transport delivery; MCP tools can fail with HTTP 200.\n\n",
        events.len()
    );
    for event in events {
        text.push_str(&format!(
            "source={} operation={} error={} status={} duration_ms={} trace_id={} received_at={} error_reason={}\n",
            source_label(&event.source),
            operation_label(&event.operation),
            event
                .error_code
                .as_ref()
                .map(error_label)
                .unwrap_or("unknown"),
            event
                .status_code
                .map(|status| status.to_string())
                .unwrap_or_else(|| "none".into()),
            event.duration_ms,
            event.trace_id,
            event.received_at,
            event.error_reason.map(|reason| reason.as_str()).unwrap_or("unknown"),
        ));
    }
    (batch_id, subject, text)
}

fn source_label(source: &super::Source) -> &'static str {
    match source {
        super::Source::Web => "web",
        super::Source::Cli => "cli",
        super::Source::Mcp => "mcp",
        super::Source::Server => "server",
    }
}

fn operation_label(operation: &super::Operation) -> &'static str {
    match operation {
        super::Operation::Recall => "recall",
        super::Operation::Expand => "expand",
        super::Operation::Vocab => "vocab",
        super::Operation::Catalog => "catalog",
        super::Operation::Profile => "profile",
        super::Operation::Dashboard => "dashboard",
        super::Operation::Contribution => "contribution",
        super::Operation::Outcome => "outcome",
        super::Operation::Auth => "auth",
        super::Operation::Device => "device",
        super::Operation::Review => "review",
        super::Operation::Usage => "usage",
        super::Operation::Telemetry => "telemetry",
        super::Operation::Unknown => "unknown",
    }
}

fn error_label(error: &ErrorCode) -> &'static str {
    match error {
        ErrorCode::NetworkError => "network_error",
        ErrorCode::HttpError => "http_error",
        ErrorCode::InvalidResponse => "invalid_response",
        ErrorCode::InvalidInput => "invalid_input",
        ErrorCode::UnhandledError => "unhandled_error",
        ErrorCode::UnhandledRejection => "unhandled_rejection",
        ErrorCode::Panic => "panic",
        ErrorCode::InternalError => "internal_error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::mpsc;

    struct FakeMailer {
        failures_left: AtomicUsize,
        attempts: std::sync::Mutex<Vec<String>>,
        sent: mpsc::UnboundedSender<(String, String, String)>,
    }

    #[async_trait]
    impl AlertMailer for FakeMailer {
        async fn send(
            &self,
            _recipient: &str,
            batch_id: &str,
            subject: &str,
            text: &str,
        ) -> Result<(), AlertDeliveryError> {
            self.attempts.lock().unwrap().push(batch_id.into());
            if self
                .failures_left
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                    left.checked_sub(1)
                })
                .is_ok()
            {
                return Err(AlertDeliveryError);
            }
            let _ = self
                .sent
                .send((batch_id.into(), subject.into(), text.into()));
            Ok(())
        }
    }

    fn event(
        source: super::super::Source,
        status_code: u16,
        error_code: ErrorCode,
    ) -> StoredDiagnostic {
        let received_at_micros = super::super::types::now_micros();
        StoredDiagnostic {
            event_id: super::super::random_id(),
            trace_id: super::super::random_id(),
            kind: "request".into(),
            source,
            operation: super::super::Operation::Recall,
            error_code: Some(error_code),
            error_reason: None,
            admin_auth: None,
            method: "POST".into(),
            status_code: Some(status_code),
            duration_ms: 12,
            received_at: received_at_micros / 1_000_000,
            received_at_micros,
        }
    }

    #[test]
    fn recall_alert_explains_http_200_and_includes_the_closed_reason() {
        let mut document = serde_json::to_value(event(
            super::super::Source::Mcp,
            200,
            ErrorCode::InvalidInput,
        ))
        .unwrap();
        document["error_reason"] = serde_json::json!("invalid_url");
        let rejected: StoredDiagnostic = serde_json::from_value(document).unwrap();
        assert!(is_alertable(&rejected));
        let (_, _, body) = format_batch(&[rejected]);
        assert!(body.contains("error_reason=invalid_url"));
        assert!(body.contains("HTTP status describes transport delivery"));
    }

    #[test]
    fn cursor_orders_events_in_the_same_second_by_arrival_time() {
        let mut delivered = event(super::super::Source::Cli, 500, ErrorCode::InternalError);
        delivered.event_id = "f".repeat(32);
        delivered.received_at = 1_800_000_000;
        delivered.received_at_micros = 1_800_000_000_000_001;

        let mut arrived_later = event(super::super::Source::Mcp, 500, ErrorCode::InternalError);
        arrived_later.event_id = "0".repeat(32);
        arrived_later.received_at = delivered.received_at;
        arrived_later.received_at_micros = delivered.received_at_micros + 1;

        assert!(AlertCursor::after(&delivered).precedes(&arrived_later));
    }

    fn fake_mailer(
        failures: usize,
    ) -> (
        Arc<FakeMailer>,
        mpsc::UnboundedReceiver<(String, String, String)>,
    ) {
        let (sent, receiver) = mpsc::unbounded_channel();
        (
            Arc::new(FakeMailer {
                failures_left: AtomicUsize::new(failures),
                attempts: std::sync::Mutex::new(Vec::new()),
                sent,
            }),
            receiver,
        )
    }

    #[tokio::test]
    async fn batches_closed_metadata_skips_routine_http_rejections_and_persists_checkpoint() {
        let diagnostics = Diagnostics::in_memory();
        let (mailer, mut sent) = fake_mailer(0);
        let worker = tokio::spawn(run(
            diagnostics.clone(),
            mailer,
            "alerts@example.test".into(),
            Duration::from_millis(5),
        ));
        diagnostics.record(event(super::super::Source::Web, 404, ErrorCode::HttpError));
        let error = event(super::super::Source::Mcp, 503, ErrorCode::InternalError);
        let expected_trace = error.trace_id.clone();
        diagnostics.record(error);

        let (_, subject, body) = tokio::time::timeout(Duration::from_secs(1), sent.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(subject, "WAM error alert: 1 event(s)");
        assert!(body.contains("source=mcp operation=recall error=internal_error status=503"));
        assert!(body.contains(&format!("trace_id={expected_trace}")));
        diagnostics.flush().unwrap();
        assert!(
            diagnostics
                .recent(100)
                .unwrap()
                .iter()
                .any(|event| event.kind == "email_checkpoint")
        );
        worker.abort();
    }

    #[tokio::test]
    async fn failed_delivery_retries_the_same_batch_key_and_checkpoint_prevents_resend() {
        let diagnostics = Diagnostics::in_memory();
        diagnostics.record(event(super::super::Source::Cli, 500, ErrorCode::HttpError));
        diagnostics.flush().unwrap();
        let (mailer, mut sent) = fake_mailer(1);
        let worker = tokio::spawn(run(
            diagnostics.clone(),
            mailer.clone(),
            "alerts@example.test".into(),
            Duration::from_millis(5),
        ));
        let (batch_id, _, _) = tokio::time::timeout(Duration::from_secs(1), sent.recv())
            .await
            .unwrap()
            .unwrap();
        let attempts = mailer.attempts.lock().unwrap().clone();
        assert_eq!(attempts, [batch_id.clone(), batch_id]);
        diagnostics.flush().unwrap();
        worker.abort();

        let (second_mailer, mut second_sent) = fake_mailer(0);
        let second_worker = tokio::spawn(run(
            diagnostics,
            second_mailer,
            "alerts@example.test".into(),
            Duration::from_millis(5),
        ));
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(second_sent.try_recv().is_err());
        second_worker.abort();
    }
}
