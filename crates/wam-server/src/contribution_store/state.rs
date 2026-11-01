use crate::ports::{
    ContributionStoreError, OwnedContribution, ReviewAuditRecord, ReviewContribution,
};
use serde::{Deserialize, Serialize};
use wam_contracts::{ContributionAction, ContributionRecordResponse, ContributionReviewDecision};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct StoredContribution {
    pub(crate) account_id: String,
    pub(crate) record: ContributionRecordResponse,
    #[serde(default)]
    pub(crate) review_generation: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ContributionState {
    pub(crate) schema_version: u8,
    pub(crate) next_id: u64,
    pub(crate) contributions: Vec<StoredContribution>,
    #[serde(default)]
    pub(crate) review_audit: Vec<ReviewAuditRecord>,
}

impl Default for ContributionState {
    fn default() -> Self {
        Self {
            schema_version: 2,
            next_id: 1,
            contributions: Vec::new(),
            review_audit: Vec::new(),
        }
    }
}

pub(crate) fn create_record(
    state: &mut ContributionState,
    account_id: &str,
    url_pattern: String,
    task_category: String,
    actions: Vec<ContributionAction>,
    submitted_at: u64,
) -> Result<ContributionRecordResponse, ContributionStoreError> {
    let id = format!("local-{:06}", state.next_id);
    state.next_id = state
        .next_id
        .checked_add(1)
        .ok_or_else(|| ContributionStoreError("contribution id space exhausted".into()))?;
    let record = ContributionRecordResponse {
        id,
        url_pattern,
        task_category,
        actions,
        status: "queued".to_owned(),
        submitted_at,
        progress: None,
        reflection: None,
    };
    state.contributions.push(StoredContribution {
        account_id: account_id.to_owned(),
        record: record.clone(),
        review_generation: 0,
    });
    Ok(record)
}

pub(crate) fn list_records(
    state: &ContributionState,
    account_id: &str,
) -> Vec<ContributionRecordResponse> {
    let mut records: Vec<_> = state
        .contributions
        .iter()
        .filter(|stored| stored.account_id == account_id)
        .map(|stored| stored.record.clone())
        .collect();
    records.sort_by_key(|item| std::cmp::Reverse(item.submitted_at));
    records
}

pub(crate) fn get_record(
    state: &ContributionState,
    account_id: &str,
    id: &str,
) -> Option<ContributionRecordResponse> {
    state
        .contributions
        .iter()
        .find(|stored| stored.account_id == account_id && stored.record.id == id)
        .map(|stored| stored.record.clone())
}

pub(crate) fn withdraw_record(state: &mut ContributionState, account_id: &str, id: &str) -> bool {
    let Some(index) = state
        .contributions
        .iter()
        .position(|stored| stored.account_id == account_id && stored.record.id == id)
    else {
        return false;
    };
    state.contributions.remove(index);
    true
}

pub(crate) fn is_active_review_status(status: &str) -> bool {
    matches!(status, "queued" | "in_review")
}

pub(crate) fn all_queue_records(state: &ContributionState) -> Vec<OwnedContribution> {
    state
        .contributions
        .iter()
        .filter(|stored| is_active_review_status(&stored.record.status))
        .map(|stored| OwnedContribution {
            account_id: stored.account_id.clone(),
            record: stored.record.clone(),
        })
        .collect()
}

pub(crate) fn all_review_records(state: &ContributionState) -> Vec<ReviewContribution> {
    let mut records: Vec<_> = state
        .contributions
        .iter()
        .map(|stored| ReviewContribution {
            record: stored.record.clone(),
            review_generation: stored.review_generation,
        })
        .collect();
    records.sort_by_key(|item| std::cmp::Reverse(item.record.submitted_at));
    records
}

pub(crate) fn all_audit_records(state: &ContributionState) -> Vec<ReviewAuditRecord> {
    let mut events = state.review_audit.clone();
    events.sort_by_key(|event| std::cmp::Reverse(event.decided_at));
    events
}

pub(crate) fn claim_review_record(
    state: &mut ContributionState,
    id: &str,
) -> Result<Option<ReviewContribution>, ContributionStoreError> {
    let Some(stored) = state
        .contributions
        .iter_mut()
        .find(|stored| stored.record.id == id)
    else {
        return Ok(None);
    };
    if stored.record.status != "queued" {
        return Ok(None);
    }
    stored.review_generation = stored
        .review_generation
        .checked_add(1)
        .ok_or_else(|| ContributionStoreError("review generation space exhausted".into()))?;
    stored.record.status = "in_review".to_owned();
    Ok(Some(ReviewContribution {
        record: stored.record.clone(),
        review_generation: stored.review_generation,
    }))
}

pub(crate) fn decide_review_record(
    state: &mut ContributionState,
    id: &str,
    review_generation: u64,
    decision: ContributionReviewDecision,
    reviewer: &str,
    decided_at: u64,
) -> Option<ContributionRecordResponse> {
    let index = state
        .contributions
        .iter()
        .position(|stored| stored.record.id == id)?;
    let stored = &mut state.contributions[index];
    if stored.record.status != "in_review" || stored.review_generation != review_generation {
        return None;
    }
    let decision_name = match decision {
        ContributionReviewDecision::Approve => "approved",
        ContributionReviewDecision::Reject => "rejected",
    };
    stored.record.status = decision_name.to_owned();
    state.review_audit.push(ReviewAuditRecord {
        contribution_id: stored.record.id.clone(),
        decision: decision_name.to_owned(),
        review_generation,
        decided_at,
        reviewer: reviewer.to_owned(),
    });
    Some(stored.record.clone())
}
