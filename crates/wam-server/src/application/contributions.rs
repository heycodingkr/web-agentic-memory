use super::*;

impl<R: Repository> WamService<R> {
    pub fn create_contribution(
        &self,
        account_id: &str,
        mut request: CreateContributionRequest,
    ) -> Result<ContributionRecordResponse, ServiceError> {
        if request.url.len() > 2048 {
            return Err(ServiceError::InvalidInput("invalid_url"));
        }
        if !request.consent || request.consent_version != CONTRIBUTION_CONSENT_VERSION {
            return Err(ServiceError::InvalidInput("consent_required"));
        }
        if request.actions.is_empty() || request.actions.len() > 8 {
            return Err(ServiceError::InvalidInput("invalid_actions"));
        }
        if request.actions.iter().collect::<HashSet<_>>().len() != request.actions.len() {
            return Err(ServiceError::InvalidInput("invalid_actions"));
        }
        if request.nonce.as_deref().is_some_and(|nonce| {
            nonce.is_empty()
                || nonce.len() > 128
                || !nonce
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        }) {
            return Err(ServiceError::InvalidInput("invalid_nonce"));
        }
        let task = self
            .repository
            .task_vocabulary()
            .resolve(&request.task_category)
            .map_err(|_| ServiceError::InvalidInput("invalid_task_category"))?;
        let url = CanonicalUrl::parse(&request.url, UrlMode::Public)
            .map_err(|_| ServiceError::InvalidInput("invalid_url"))?;
        request.task_category = task.as_str().to_owned();
        // Keep local file status and the dispatch queue ordered with review claims.
        let mut queue = self.queue.lock().map_err(|_| ServiceError::Internal)?;
        let record = self
            .contributions
            .create_idempotent(
                account_id,
                &request,
                &format!("https://{}{}", url.host(), url.path()),
                now_unix(),
            )
            .map_err(|error| match error.0.as_str() {
                "consent_withdrawn" => ServiceError::InvalidInput("consent_withdrawn"),
                "nonce_conflict" => ServiceError::Conflict,
                "account_disabled" => ServiceError::Unauthorized,
                "queue_unavailable" => ServiceError::Unavailable,
                _ => ServiceError::Internal,
            })?;
        if self.contributions.transactional_queue() {
            return Ok(record);
        }
        let job = match queue_job(account_id, &record) {
            Ok(job) => job,
            Err(_) => {
                let _ = self
                    .contributions
                    .withdraw_for_account(account_id, &record.id);
                return Err(ServiceError::Internal);
            }
        };
        if queue.enqueue(job, record.submitted_at).is_err() {
            let _ = self
                .contributions
                .withdraw_for_account(account_id, &record.id);
            return Err(ServiceError::Unavailable);
        }
        Ok(record)
    }

    pub fn review_queue(&self) -> Result<ReviewQueueResponse, ServiceError> {
        let queue = self.queue.lock().map_err(|_| ServiceError::Internal)?;
        let stats = queue.stats(now_unix());
        let durable_health = self
            .contributions
            .queue_health(now_unix())
            .map_err(|_| ServiceError::Unavailable)?;
        let records = if let Some(repository) = &self.lifecycle {
            repository.admin_review_records(now_unix())?
        } else {
            self.contributions
                .all_for_review()
                .map_err(|_| ServiceError::Internal)?
        };
        let mut counts = ReviewQueueCounts::default();
        let mut contributions = Vec::new();
        for stored in records {
            match stored.record.status.as_str() {
                "queued" => counts.queued += 1,
                "in_review" => counts.in_review += 1,
                "approved" => counts.approved += 1,
                "rejected" => counts.rejected += 1,
                _ => {}
            }
            contributions.push(review_queue_item(stored));
        }
        let decision_history = self
            .contributions
            .review_audit_history()
            .map_err(|_| ServiceError::Internal)?
            .into_iter()
            .map(|event| ReviewDecisionEvent {
                contribution_id: event.contribution_id,
                decision: event.decision,
                review_generation: event.review_generation,
                decided_at: event.decided_at,
                reviewer: event.reviewer,
            })
            .collect();
        Ok(ReviewQueueResponse {
            schema_version: SCHEMA_VERSION,
            counts,
            queue_health: durable_health.unwrap_or(ReviewQueueHealth {
                active_jobs: stats.active,
                oldest_age_secs: stats.oldest_age_secs,
                alert: stats.alert,
            }),
            contributions,
            decision_history,
        })
    }

    pub fn admin_contribution_detail(
        &self,
        id: &str,
    ) -> Result<crate::durable::admin::AdminContributionDetail, ServiceError> {
        self.lifecycle
            .as_ref()
            .ok_or(ServiceError::Unavailable)?
            .admin_contribution_detail(id, now_unix())
    }

    pub fn claim_contribution_for_review(&self, id: &str) -> Result<ReviewQueueItem, ServiceError> {
        let _queue = self.queue.lock().map_err(|_| ServiceError::Internal)?;
        let claimed = self
            .contributions
            .claim_for_review(id)
            .map_err(|_| ServiceError::Internal)?;
        match claimed {
            Some(record) => Ok(review_queue_item(record)),
            None => {
                let exists = self
                    .contributions
                    .all_for_review()
                    .map_err(|_| ServiceError::Internal)?
                    .iter()
                    .any(|record| record.record.id == id);
                Err(if exists {
                    ServiceError::Conflict
                } else {
                    ServiceError::NotFound
                })
            }
        }
    }

    pub fn decide_contribution_review(
        &self,
        id: &str,
        review_generation: u64,
        decision: ContributionReviewDecision,
        reviewer: &str,
    ) -> Result<ContributionRecordResponse, ServiceError> {
        let approved = decision == ContributionReviewDecision::Approve;
        let mut queue = self.queue.lock().map_err(|_| ServiceError::Internal)?;
        let updated = self
            .contributions
            .decide_for_review(id, review_generation, decision, reviewer, now_unix())
            .map_err(|_| ServiceError::Internal)?;
        let record = match updated {
            Some(record) => record,
            None => {
                let exists = self
                    .contributions
                    .all_for_review()
                    .map_err(|_| ServiceError::Internal)?
                    .iter()
                    .any(|record| record.record.id == id);
                return Err(if exists {
                    ServiceError::Conflict
                } else {
                    ServiceError::NotFound
                });
            }
        };
        queue.resolve_contribution(id, approved, now_unix());
        Ok(record)
    }

    pub fn report_outcome(
        &self,
        account_id: &str,
        receipt_id: &str,
        request: OutcomeReportRequest,
    ) -> Result<OutcomeReportResponse, ServiceError> {
        if let Some(result) =
            self.usage
                .report_and_score(account_id, receipt_id, &request, now_unix())
        {
            return result;
        }
        let result = self
            .usage
            .report_outcome(account_id, receipt_id, &request, now_unix())
            .map_err(|_| ServiceError::Internal)?;
        let (recorded, receipt) = match result {
            OutcomeReportResult::Recorded(receipt) => (true, receipt),
            OutcomeReportResult::Duplicate(receipt) => (false, receipt),
            OutcomeReportResult::Conflict => return Err(ServiceError::Conflict),
            OutcomeReportResult::NotFound => return Err(ServiceError::NotFound),
            OutcomeReportResult::InvalidVersions => {
                return Err(ServiceError::InvalidInput("invalid_used_version_ids"));
            }
        };
        if request.outcome == wam_contracts::MemoryOutcome::Success {
            for memory in receipt.used_memories {
                let Some(author_account_id) = memory.author_account_id else {
                    continue;
                };
                self.scores
                    .award_successful_use(SuccessfulUseAward {
                        beneficiary_account_id: author_account_id,
                        consumer_account_id: account_id.to_owned(),
                        memory_id: memory.memory_id,
                        memory_version_id: memory.version_id,
                        receipt_id: receipt_id.to_owned(),
                        created_at: receipt.reported_at,
                    })
                    .map_err(|_| ServiceError::Internal)?;
            }
        }
        Ok(OutcomeReportResponse {
            receipt_id: receipt_id.to_owned(),
            outcome: request.outcome,
            recorded,
        })
    }

    pub fn public_usage_metrics(&self) -> Result<PublicUsageMetrics, ServiceError> {
        self.usage
            .public_metrics()
            .map_err(|_| ServiceError::Internal)
    }

    pub fn pending_gatekeeper_jobs(&self) -> Result<usize, ServiceError> {
        if let Some(health) = self
            .contributions
            .queue_health(now_unix())
            .map_err(|_| ServiceError::Unavailable)?
        {
            return Ok(health.active_jobs);
        }
        self.queue
            .lock()
            .map_err(|_| ServiceError::Internal)
            .map(|queue| queue.jobs().filter(|job| job.state.active()).count())
    }

    pub fn my_contributions(
        &self,
        account_id: &str,
    ) -> Result<ContributionListResponse, ServiceError> {
        if let Some(lifecycle) = &self.lifecycle {
            return lifecycle.owner_contributions(account_id, now_unix());
        }
        if !self
            .repository
            .try_ranking_accounts()
            .map_err(|_| ServiceError::Unavailable)?
            .iter()
            .any(|account| account.key() == account_id)
        {
            return Err(ServiceError::NotFound);
        }
        let contributions = self
            .contributions
            .list_for_account(account_id)
            .map_err(|_| ServiceError::Internal)?;
        Ok(ContributionListResponse {
            schema_version: SCHEMA_VERSION,
            contributions,
        })
    }

    pub fn contribution(
        &self,
        account_id: &str,
        id: &str,
    ) -> Result<ContributionRecordResponse, ServiceError> {
        if let Some(lifecycle) = &self.lifecycle {
            return lifecycle
                .owner_contribution(account_id, id, now_unix())?
                .ok_or(ServiceError::NotFound);
        }
        self.contributions
            .get_for_account(account_id, id)
            .map_err(|_| ServiceError::Internal)?
            .ok_or(ServiceError::NotFound)
    }

    pub fn withdraw_contribution(&self, account_id: &str, id: &str) -> Result<(), ServiceError> {
        if self
            .contributions
            .get_for_account(account_id, id)
            .map_err(|_| ServiceError::Internal)?
            .is_none()
        {
            return Err(ServiceError::NotFound);
        }
        if self
            .contributions
            .withdraw_for_account(account_id, id)
            .map_err(|_| ServiceError::Internal)?
        {
            self.queue
                .lock()
                .map_err(|_| ServiceError::Internal)?
                .cancel_contribution(id, now_unix());
            Ok(())
        } else {
            Err(ServiceError::NotFound)
        }
    }
}
