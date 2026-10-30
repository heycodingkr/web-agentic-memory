mod lifecycle_support;
use lifecycle_support::*;
use postgres::{Client, NoTls};
use std::{cell::Cell, sync::Arc};
use wam_contracts::{MemoryOutcome, OutcomeReportRequest, RecallRequest};
use wam_core::{
    recall::{RecallDocument, RecallQuery, recall_documents},
    url_scope::{CanonicalUrl, UrlMode},
};
use wam_server::{
    ApplicationRepository, FileIdentityStore, IdentityService, WamService,
    application::ServiceError,
    diagnostics::random_id,
    durable::{ReplayEvidence, ReplayRequest, ReplayVerifier, WorkResult},
    identity::{DevicePoll, GitHubOAuthProvider, IdentityError},
    ports::{Repository, ScoreStore, UsageStore},
};

struct Schema {
    base: String,
    url: String,
    name: String,
}
impl Schema {
    fn new() -> Self {
        let base =
            std::env::var("WAM_TEST_DATABASE_URL").expect("use a disposable WAM_TEST_DATABASE_URL");
        let name = format!("wam_application_{}_{}", std::process::id(), random_id());
        Client::connect(&base, NoTls)
            .unwrap()
            .batch_execute(&format!("CREATE SCHEMA {name}"))
            .unwrap();
        let scoped = schema_url(&base, &name);
        Self {
            base,
            url: scoped,
            name,
        }
    }
    fn repository(&self) -> ApplicationRepository {
        ApplicationRepository::connect(&self.url).expect("open application schema")
    }
    fn client(&self) -> Client {
        Client::connect(&self.url, NoTls).unwrap()
    }
}
fn schema_url(base: &str, name: &str) -> String {
    let mut scoped = url::Url::parse(base).unwrap();
    scoped
        .query_pairs_mut()
        .append_pair("options", &format!("-c search_path={name}"));
    let query = scoped.query().unwrap_or_default().replace('+', "%20");
    scoped.set_query(Some(&query));
    scoped.to_string()
}

#[test]
fn schema_options_encoding_preserves_url_credentials() {
    let scoped = schema_url(
        "postgres://fixture:fixture+value@localhost/disposable",
        "unit_scope",
    );
    assert_eq!(
        url::Url::parse(&scoped).unwrap().password(),
        Some("fixture+value")
    );
    assert!(scoped.contains("options=-c%20search_path%3Dunit_scope"));
}

impl Drop for Schema {
    fn drop(&mut self) {
        if let Ok(mut client) = Client::connect(&self.base, NoTls) {
            let _ = client.batch_execute(&format!("DROP SCHEMA {} CASCADE", self.name));
        }
    }
}

#[test]
#[ignore = "requires disposable WAM_TEST_DATABASE_URL"]
fn migrations_profiles_intake_queue_and_outbox_survive_reopen() {
    let schema = Schema::new();
    let repository = schema.repository();
    let service = prepare(&repository);
    let id = contribution(&service, AUTHOR);
    let mut client = schema.client();
    assert_eq!(
        client
            .query_one("SELECT count(*) FROM wam_schema_migrations", &[])
            .unwrap()
            .get::<_, i64>(0),
        2
    );
    assert_eq!(
        client
            .query_one(
                "SELECT count(*) FROM wam_work_items WHERE contribution_id=$1",
                &[&id]
            )
            .unwrap()
            .get::<_, i64>(0),
        1
    );
    assert_eq!(
        client
            .query_one(
                "SELECT count(*) FROM wam_outbox WHERE kind='contribution_queued' AND target=$1",
                &[&id]
            )
            .unwrap()
            .get::<_, i64>(0),
        1
    );
    drop(service);
    drop(repository);
    let repository = schema.repository();
    let service = prepare(&repository);
    assert_eq!(
        service
            .my_contributions(AUTHOR)
            .unwrap()
            .contributions
            .len(),
        1
    );
    assert_eq!(service.pending_gatekeeper_jobs().unwrap(), 1);
}

#[test]
#[ignore = "requires disposable WAM_TEST_DATABASE_URL"]
fn postgres_recall_candidates_match_the_core_oracle_and_filter_routes_before_loading() {
    let schema = Schema::new();
    let repository = schema.repository();
    let service = prepare(&repository);
    contribution(&service, AUTHOR);
    repository
        .process_next(&mut provider("high"), &PassingReplay, now())
        .unwrap();

    let tasks = repository.task_vocabulary();
    let keywords = repository
        .try_keyword_vocabulary()
        .unwrap()
        .filter(["draft"])
        .unwrap();
    let exact = RecallQuery::new(
        CanonicalUrl::parse("https://example.org/editor", UrlMode::Public).unwrap(),
        Some(tasks.resolve("compose-draft").unwrap()),
        None,
        keywords.clone(),
    )
    .unwrap();
    let candidates = repository.try_recall_candidates(&exact).unwrap();
    let all_records = repository.try_memories().unwrap();
    let all_documents: Vec<_> = all_records
        .iter()
        .map(|record| {
            RecallDocument::new(
                &record.index,
                &record.title,
                &record.summary,
                &record.site,
                &record.domain,
            )
        })
        .collect();
    let oracle_ids: Vec<_> = recall_documents(&all_documents, &exact)
        .into_iter()
        .map(|hit| hit.memory.id().to_owned())
        .collect();
    let candidate_documents: Vec<_> = candidates
        .iter()
        .map(|record| {
            RecallDocument::new(
                &record.index,
                &record.title,
                &record.summary,
                &record.site,
                &record.domain,
            )
        })
        .collect();
    let candidate_ids: Vec<_> = recall_documents(&candidate_documents, &exact)
        .into_iter()
        .map(|hit| hit.memory.id().to_owned())
        .collect();
    assert_eq!(candidate_ids, oracle_ids);
    assert_eq!(candidate_ids.len(), 1);

    let child_route = RecallQuery::new(
        CanonicalUrl::parse("https://example.org/editor/child", UrlMode::Public).unwrap(),
        Some(tasks.resolve("compose-draft").unwrap()),
        None,
        keywords.clone(),
    )
    .unwrap();
    assert!(
        repository
            .try_recall_candidates(&child_route)
            .unwrap()
            .is_empty()
    );

    let wrong_host = RecallQuery::new(
        CanonicalUrl::parse("https://elsewhere.org/editor", UrlMode::Public).unwrap(),
        None,
        None,
        keywords,
    )
    .unwrap();
    assert!(
        repository
            .try_recall_candidates(&wrong_host)
            .unwrap()
            .is_empty()
    );

    let response = service
        .recall(
            RecallRequest {
                url: "https://example.org/editor".into(),
                task_category: Some("compose-draft".into()),
                keywords: vec!["draft".into()],
                ..RecallRequest::default()
            },
            Some(READER),
        )
        .unwrap();
    assert_eq!(response.hits.len(), 1);
    let receipt = response.receipt_id.expect("authenticated recall receipt");
    let mut client = schema.client();
    assert_eq!(
        client
            .query_one(
                "SELECT count(*) FROM wam_usage_receipts WHERE id=$1",
                &[&receipt]
            )
            .unwrap()
            .get::<_, i64>(0),
        1
    );
    let usage: serde_json::Value = client
        .query_one(
            "SELECT document->'usage' FROM wam_app_meta WHERE singleton",
            &[],
        )
        .unwrap()
        .get(0);
    assert_eq!(usage["authenticated_recall_requests"], 1);
    let memory_id = response.hits[0].memory_id.as_str();
    let detail = service.expand(memory_id, 1, Some(READER)).unwrap();
    assert_eq!(detail.memory_id, memory_id);
    let metrics = repository.public_metrics().unwrap();
    assert_eq!(metrics.authenticated_recall_requests, 1);
    assert_eq!(metrics.detail_requests, 1);
    let analytics = service
        .memory_analytics(AUTHOR, None, None, Some("day"))
        .unwrap();
    let authored = analytics
        .memories
        .iter()
        .find(|memory| memory.version_id == response.hits[0].version_id)
        .expect("author's version appears in analytics");
    assert_eq!(
        authored
            .points
            .iter()
            .map(|point| point.other_account_recall_hits)
            .sum::<u64>(),
        1
    );
    assert_eq!(
        authored
            .points
            .iter()
            .map(|point| point.other_account_detail_views)
            .sum::<u64>(),
        1
    );
    assert_eq!(
        client
            .query_one("SELECT count(*) FROM wam_memory_usage_aggregates", &[])
            .unwrap()
            .get::<_, i64>(0),
        1
    );
}

#[test]
#[ignore = "requires disposable WAM_TEST_DATABASE_URL"]
fn postgres_expand_pins_public_superseded_versions_and_hides_retired_history() {
    let schema = Schema::new();
    let repository = schema.repository();
    let service = prepare(&repository);
    contribution(&service, AUTHOR);
    let WorkResult::Published {
        memory_id,
        version_id: first,
    } = repository
        .process_next(&mut provider("high"), &PassingReplay, now())
        .unwrap()
    else {
        panic!()
    };

    improvement(&service, SUPPORTER);
    let WorkResult::Published {
        version_id: second, ..
    } = repository
        .process_next(&mut replacement(&memory_id, 1), &PassingReplay, now())
        .unwrap()
    else {
        panic!()
    };

    let previous = service
        .expand_version(&memory_id, 2, Some(&first), None)
        .expect("a superseded version is readable while its memory is public");
    assert_eq!(previous.version_id, first);
    assert_eq!(
        service.expand(&memory_id, 2, None).unwrap().version_id,
        second
    );

    repository.retire_version(&memory_id, 2, now()).unwrap();
    assert_eq!(
        service.expand_version(&memory_id, 2, Some(&first), None),
        Err(ServiceError::NotFound)
    );
}

#[test]
#[ignore = "requires disposable WAM_TEST_DATABASE_URL"]
fn postgres_publication_receipt_score_reversal_and_immutable_facts_are_consistent() {
    let schema = Schema::new();
    let repository = schema.repository();
    let service = prepare(&repository);
    contribution(&service, AUTHOR);
    let WorkResult::Published {
        memory_id,
        version_id,
    } = repository
        .process_next(&mut provider("high"), &PassingReplay, now())
        .unwrap()
    else {
        panic!()
    };
    let receipt = service
        .recall(
            RecallRequest {
                url: "https://example.org/editor".into(),
                ..RecallRequest::default()
            },
            Some(READER),
        )
        .unwrap()
        .receipt_id
        .unwrap();
    let request = OutcomeReportRequest {
        outcome: MemoryOutcome::Success,
        used_version_ids: vec![version_id.clone()],
    };
    assert!(
        service
            .report_outcome(READER, &receipt, request.clone())
            .unwrap()
            .recorded
    );
    let analytics = service
        .memory_analytics(AUTHOR, None, None, Some("day"))
        .unwrap();
    let authored = analytics
        .memories
        .iter()
        .find(|memory| memory.version_id == version_id)
        .expect("published version analytics");
    assert_eq!(
        authored
            .points
            .iter()
            .map(|point| point.success_outcomes)
            .sum::<u64>(),
        1
    );
    let reopened = schema.repository();
    let second = WamService::durable(reopened.clone()).unwrap();
    assert!(
        !second
            .report_outcome(READER, &receipt, request)
            .unwrap()
            .recorded
    );
    assert_eq!(reopened.total_points_for(AUTHOR).unwrap(), 22);
    let mut client = schema.client();
    assert_eq!(
        client
            .query_one("SELECT count(*) FROM wam_score_events", &[])
            .unwrap()
            .get::<_, i64>(0),
        2
    );
    assert!(client.execute("UPDATE wam_memory_versions SET document=jsonb_set(document,'{payload,l1,title}','\"forged\"') WHERE id=$1", &[&version_id]).is_err());
    assert!(
        client
            .execute(
                "UPDATE wam_score_events SET points_delta=999 WHERE event_type='approval'",
                &[]
            )
            .is_err()
    );
    assert!(client.execute("DELETE FROM wam_score_events", &[]).is_err());
    reopened.retire_version(&memory_id, 1, now()).unwrap();
    reopened.retire_version(&memory_id, 1, now()).unwrap();
    assert_eq!(reopened.total_points_for(AUTHOR).unwrap(), 0);
    assert_eq!(
        client
            .query_one(
                "SELECT count(*) FROM wam_score_events WHERE event_type='reversal'",
                &[]
            )
            .unwrap()
            .get::<_, i64>(0),
        2
    );
    assert!(second.catalog().unwrap().memories.is_empty());
}

#[test]
#[ignore = "requires disposable WAM_TEST_DATABASE_URL"]
fn score_receipt_and_reversal_links_are_immutable_facts() {
    let schema = Schema::new();
    let repository = schema.repository();
    let service = prepare(&repository);
    contribution(&service, AUTHOR);
    let WorkResult::Published {
        memory_id,
        version_id,
    } = repository
        .process_next(&mut provider("high"), &PassingReplay, now())
        .unwrap()
    else {
        panic!()
    };
    let request = RecallRequest {
        url: "https://example.org/editor".into(),
        ..RecallRequest::default()
    };
    let original_receipt = service
        .recall(request.clone(), Some(READER))
        .unwrap()
        .receipt_id
        .unwrap();
    service
        .report_outcome(
            READER,
            &original_receipt,
            OutcomeReportRequest {
                outcome: MemoryOutcome::Success,
                used_version_ids: vec![version_id],
            },
        )
        .unwrap();
    let another_receipt = service
        .recall(request, Some(READER))
        .unwrap()
        .receipt_id
        .unwrap();
    let mut client = schema.client();
    assert!(
        client
            .execute(
                "UPDATE wam_score_events SET receipt_id=$1 WHERE event_type='successful_use'",
                &[&another_receipt]
            )
            .is_err(),
        "a score must retain the receipt that justified it"
    );
    repository.retire_version(&memory_id, 1, now()).unwrap();
    assert!(client.execute(
        "UPDATE wam_score_events SET reversal_of=(SELECT id FROM wam_score_events WHERE event_type='successful_use') WHERE points_delta=-20",
        &[]
    ).is_err(), "a reversal must retain its original positive event");
}

#[test]
#[ignore = "requires disposable WAM_TEST_DATABASE_URL"]
fn failed_approval_insert_rolls_back_memory_decision_contribution_score_and_outbox() {
    let schema = Schema::new();
    let repository = schema.repository();
    let service = prepare(&repository);
    let id = contribution(&service, AUTHOR);
    let mut client = schema.client();
    client.batch_execute("CREATE FUNCTION reject_approval() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.event_type='approval' THEN RAISE EXCEPTION 'injected score failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER injected_score_failure BEFORE INSERT ON wam_score_events FOR EACH ROW EXECUTE FUNCTION reject_approval()").unwrap();
    let start = now();
    assert_eq!(
        repository.process_next(&mut provider("high"), &PassingReplay, start),
        Err(ServiceError::Unavailable)
    );
    assert_eq!(
        client
            .query_one("SELECT count(*) FROM wam_memory_records", &[])
            .unwrap()
            .get::<_, i64>(0),
        0
    );
    assert_eq!(
        client
            .query_one("SELECT count(*) FROM wam_decisions", &[])
            .unwrap()
            .get::<_, i64>(0),
        0
    );
    assert_eq!(
        client
            .query_one("SELECT count(*) FROM wam_score_events", &[])
            .unwrap()
            .get::<_, i64>(0),
        0
    );
    assert_eq!(service.contribution(AUTHOR, &id).unwrap().status, "queued");
    assert_eq!(
        client
            .query_one(
                "SELECT count(*) FROM wam_outbox WHERE kind='curation_committed'",
                &[]
            )
            .unwrap()
            .get::<_, i64>(0),
        0
    );
    client
        .batch_execute("DROP TRIGGER injected_score_failure ON wam_score_events")
        .unwrap();
    let reopened = schema.repository();
    assert!(matches!(
        reopened
            .process_next(&mut provider("high"), &PassingReplay, start + 301)
            .unwrap(),
        WorkResult::Published { .. }
    ));
    assert_eq!(reopened.total_points_for(AUTHOR).unwrap(), 20);
}

#[test]
#[ignore = "requires disposable WAM_TEST_DATABASE_URL"]
fn separate_database_connection_withdrawal_wins_before_publication_commit() {
    let schema = Schema::new();
    let repository = schema.repository();
    let withdrawal = schema.repository();
    let service = prepare(&repository);
    contribution(&service, AUTHOR);
    struct Withdraw {
        repository: ApplicationRepository,
        calls: Cell<u8>,
    }
    impl ReplayVerifier for Withdraw {
        fn verify(&self, request: &ReplayRequest) -> Result<ReplayEvidence, ServiceError> {
            self.calls.set(self.calls.get() + 1);
            if self.calls.get() == 2 {
                self.repository.set_consent(
                    AUTHOR,
                    false,
                    wam_contracts::CONTRIBUTION_CONSENT_VERSION,
                    now(),
                )?;
            }
            PassingReplay.verify(request)
        }
    }
    let replay = Withdraw {
        repository: withdrawal,
        calls: Cell::new(0),
    };
    assert_eq!(
        repository.process_next(&mut provider("high"), &replay, now()),
        Err(ServiceError::Conflict)
    );
    assert!(service.catalog().unwrap().memories.is_empty());
    assert_eq!(repository.total_points_for(AUTHOR).unwrap(), 0);
    assert_eq!(
        schema
            .client()
            .query_one(
                "SELECT count(*) FROM wam_work_items WHERE state='canceled'",
                &[]
            )
            .unwrap()
            .get::<_, i64>(0),
        1
    );
}

#[test]
#[ignore = "requires disposable WAM_TEST_DATABASE_URL"]
fn concurrent_outcome_retries_credit_once_across_two_connections() {
    let schema = Schema::new();
    let repository = schema.repository();
    let service = prepare(&repository);
    contribution(&service, AUTHOR);
    let WorkResult::Published { version_id, .. } = repository
        .process_next(&mut provider("high"), &PassingReplay, now())
        .unwrap()
    else {
        panic!()
    };
    let receipt = service
        .recall(
            RecallRequest {
                url: "https://example.org/editor".into(),
                ..RecallRequest::default()
            },
            Some(READER),
        )
        .unwrap()
        .receipt_id
        .unwrap();
    let second = WamService::durable(schema.repository()).unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let request = OutcomeReportRequest {
        outcome: MemoryOutcome::Success,
        used_version_ids: vec![version_id],
    };
    let r = receipt.clone();
    let input = request.clone();
    let start = barrier.clone();
    let first = std::thread::spawn(move || {
        start.wait();
        service.report_outcome(READER, &r, input).unwrap().recorded
    });
    let other = std::thread::spawn(move || {
        barrier.wait();
        second
            .report_outcome(READER, &receipt, request)
            .unwrap()
            .recorded
    });
    assert_ne!(first.join().unwrap(), other.join().unwrap());
    assert_eq!(repository.total_points_for(AUTHOR).unwrap(), 22);
    assert_eq!(
        schema
            .client()
            .query_one(
                "SELECT count(*) FROM wam_score_events WHERE event_type='successful_use'",
                &[]
            )
            .unwrap()
            .get::<_, i64>(0),
        1
    );
}

#[test]
#[ignore = "requires disposable WAM_TEST_DATABASE_URL"]
fn nonce_idempotency_and_rollback_keep_relational_constraints_after_reopen() {
    let schema = Schema::new();
    let repository = schema.repository();
    let service = prepare(&repository);
    let input:wam_contracts::CreateContributionRequest=serde_json::from_value(serde_json::json!({
        "url":"https://example.org/editor","task_category":"compose-draft","actions":["navigate","edit","save"],
        "consent":true,"consent_version":wam_contracts::CONTRIBUTION_CONSENT_VERSION,"nonce":"submit-once"
    })).unwrap();
    let first = service.create_contribution(AUTHOR, input.clone()).unwrap();
    let second = WamService::durable(schema.repository()).unwrap();
    assert_eq!(
        second.create_contribution(AUTHOR, input).unwrap().id,
        first.id
    );
    let WorkResult::Published { memory_id, .. } = repository
        .process_next(&mut provider("high"), &PassingReplay, now())
        .unwrap()
    else {
        panic!()
    };
    improvement(&service, SUPPORTER);
    repository
        .process_next(&mut replacement(&memory_id, 1), &PassingReplay, now())
        .unwrap();
    assert_eq!(repository.total_points_for(SUPPORTER).unwrap(), 20);
    let restored = repository
        .rollback_version(&memory_id, 1, 2, &PassingReplay, now())
        .unwrap();
    let reopened = schema.repository();
    assert_eq!(reopened.total_points_for(AUTHOR).unwrap(), 20);
    assert_eq!(reopened.total_points_for(SUPPORTER).unwrap(), 0);
    assert_eq!(reopened.memories()[0].index.version_id(), restored);
    let mut client = schema.client();
    assert_eq!(
        client
            .query_one("SELECT count(*) FROM wam_memory_versions", &[])
            .unwrap()
            .get::<_, i64>(0),
        3
    );
    assert_eq!(
        client
            .query_one(
                "SELECT active_version FROM wam_memory_records WHERE id=$1",
                &[&memory_id]
            )
            .unwrap()
            .get::<_, i32>(0),
        3
    );
    assert_eq!(client.query_one("SELECT COALESCE(sum(points_delta),0)::bigint FROM wam_score_events WHERE beneficiary_id=$1",&[&SUPPORTER]).unwrap().get::<_,i64>(0),0);
}

#[test]
#[ignore = "requires disposable WAM_TEST_DATABASE_URL"]
fn deleting_an_awarded_author_preserves_anonymous_facts_and_other_accounts() {
    let schema = Schema::new();
    let repository = schema.repository();
    let service = prepare(&repository);
    contribution(&service, AUTHOR);
    repository
        .process_next(&mut provider("high"), &PassingReplay, now())
        .unwrap();
    repository.delete_account(AUTHOR, now()).unwrap();
    let reopened = schema.repository();
    assert!(reopened.memories().is_empty());
    assert!(reopened.profile_for_account(AUTHOR).is_none());
    assert!(reopened.profile_for_account(READER).is_some());
    let mut client = schema.client();
    assert_eq!(
        client
            .query_one("SELECT count(*) FROM wam_score_events", &[])
            .unwrap()
            .get::<_, i64>(0),
        2
    );
    assert_eq!(
        client
            .query_one(
                "SELECT COALESCE(sum(points_delta),0)::bigint FROM wam_score_events",
                &[]
            )
            .unwrap()
            .get::<_, i64>(0),
        0
    );
    for table in [
        "wam_users",
        "wam_contributions",
        "wam_consents",
        "wam_memory_authors",
        "wam_score_events",
        "wam_identity_records",
        "wam_work_items",
        "wam_outbox",
        "wam_app_meta",
    ] {
        for row in client
            .query(&format!("SELECT document::text FROM {table}"), &[])
            .unwrap()
        {
            let text: String = row.get(0);
            assert!(!text.contains(AUTHOR), "{table}");
            assert!(!text.contains("private-sentinel"), "{table}");
        }
    }
}

struct OAuth;
#[async_trait::async_trait]
impl GitHubOAuthProvider for OAuth {
    fn authorization_url(&self, state: &str, _: &str) -> Result<String, IdentityError> {
        Ok(format!("https://example.org/authorize?state={state}"))
    }
    async fn exchange_code(&self, code: &str, _: &str) -> Result<String, IdentityError> {
        Ok(format!("test-subject:{code}"))
    }
}

#[test]
#[ignore = "requires disposable WAM_TEST_DATABASE_URL"]
fn identity_deletion_revokes_sessions_devices_and_erases_private_links_across_restart() {
    let schema = Schema::new();
    let repository = schema.repository();
    let identity = IdentityService::new(
        Arc::new(FileIdentityStore::from_repository(repository.clone())),
        Some(Arc::new(OAuth)),
        "http://localhost:5173",
        false,
    );
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let start = now();
    let (state, _) = identity.begin_oauth(start).unwrap();
    let (account, session, _, _) = runtime
        .block_on(identity.finish_oauth("delete-me", &state, &state, start))
        .unwrap();
    let device = identity
        .start_device(
            "test device",
            vec!["read:memories".into(), "write:contributions".into()],
            start,
        )
        .unwrap();
    identity
        .approve_device(&device.user_code, &account, start)
        .unwrap();
    let DevicePoll::Authorized { access_token, .. } = identity
        .poll_device(&device.device_code, start + 3)
        .unwrap()
    else {
        panic!()
    };
    assert!(identity.authenticate_session(&session, start + 4).is_some());
    assert!(
        identity
            .authenticate_device(&access_token, start + 4)
            .is_some()
    );
    repository.delete_account(&account, start + 5).unwrap();
    let reopened = schema.repository();
    let identity = IdentityService::new(
        Arc::new(FileIdentityStore::from_repository(reopened.clone())),
        Some(Arc::new(OAuth)),
        "http://localhost:5173",
        false,
    );
    assert!(identity.authenticate_session(&session, start + 6).is_none());
    assert!(
        identity
            .authenticate_device(&access_token, start + 6)
            .is_none()
    );
    assert!(reopened.profile_for_account(&account).is_none());
    assert_eq!(
        reopened.ensure_account(&account),
        Err(wam_server::ports::RepositoryError::AccountDisabled)
    );
    let rows = schema.client().query("SELECT document::text FROM wam_identity_records UNION ALL SELECT document::text FROM wam_users UNION ALL SELECT document::text FROM wam_app_meta", &[]).unwrap();
    for row in rows {
        let text: String = row.get(0);
        assert!(!text.contains(&session));
        assert!(!text.contains(&access_token));
        assert!(!text.contains(&account));
    }
}
