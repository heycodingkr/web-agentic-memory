//! Read-only administrator projections. Never serialize worker credentials or entire application state.
use super::ApplicationRepository;
use crate::{application::ServiceError, ports::ReviewContribution};
use serde::Serialize;
use wam_contracts::ContributionRecordResponse;
use wam_gatekeeper::record::RecordPayload;

#[derive(Serialize)]
pub struct AdminContributionDetail {
    pub record: ContributionRecordResponse,
    pub review_generation: u64,
    pub submitter_account_id: String,
    pub proposed_memory: Option<RecordPayload>,
    pub replay: Option<ReplaySummary>,
}
#[derive(Serialize)]
pub struct ReplaySummary {
    pub samples: u32,
    pub baseline_successes: u32,
    pub candidate_successes: u32,
    pub baseline_tool_calls: u32,
    pub candidate_tool_calls: u32,
}
impl ApplicationRepository {
    pub(crate) fn admin_review_records(
        &self,
        now: u64,
    ) -> Result<Vec<ReviewContribution>, ServiceError> {
        self.read(|state| {
            let mut records =
                crate::contribution_store::state::all_review_records(&state.contributions);
            for item in &mut records {
                if let Some(entry) = state
                    .contributions
                    .contributions
                    .iter()
                    .find(|entry| entry.record.id == item.record.id)
                {
                    item.record = super::lifecycle::owner_record(
                        state,
                        &entry.account_id,
                        item.record.clone(),
                        now,
                    );
                }
            }
            Ok(records)
        })
    }
    pub(crate) fn admin_contribution_detail(
        &self,
        id: &str,
        now: u64,
    ) -> Result<AdminContributionDetail, ServiceError> {
        self.read(|state| {
            let entry = state
                .contributions
                .contributions
                .iter()
                .find(|entry| entry.record.id == id)
                .ok_or(ServiceError::NotFound)?;
            let generation =
                crate::contribution_store::state::all_review_records(&state.contributions)
                    .into_iter()
                    .find(|item| item.record.id == id)
                    .ok_or(ServiceError::NotFound)?
                    .review_generation;
            let proposal = state.proposals.get(id);
            Ok(AdminContributionDetail {
                record: super::lifecycle::owner_record(
                    state,
                    &entry.account_id,
                    entry.record.clone(),
                    now,
                ),
                review_generation: generation,
                submitter_account_id: entry.account_id.clone(),
                proposed_memory: proposal
                    .and_then(|proposal| proposal.replay_request.proposed_record.clone()),
                replay: proposal.map(|proposal| ReplaySummary {
                    samples: proposal.replay.samples,
                    baseline_successes: proposal.replay.baseline_successes,
                    candidate_successes: proposal.replay.candidate_successes,
                    baseline_tool_calls: proposal.replay.baseline_tool_calls,
                    candidate_tool_calls: proposal.replay.candidate_tool_calls,
                }),
            })
        })
    }
}
