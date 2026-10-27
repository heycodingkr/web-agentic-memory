use super::state::{
    ContributionState, all_audit_records, all_queue_records, all_review_records,
    claim_review_record, create_record, decide_review_record, get_record, list_records,
    withdraw_record,
};
use crate::ports::{
    ContributionStore, ContributionStoreError, OwnedContribution, ReviewAuditRecord,
    ReviewContribution,
};
use std::sync::Mutex;
use wam_contracts::{ContributionAction, ContributionRecordResponse, ContributionReviewDecision};

#[derive(Default)]
pub struct InMemoryContributionStore {
    state: Mutex<ContributionState>,
}

impl ContributionStore for InMemoryContributionStore {
    fn create(
        &self,
        account_id: &str,
        url_pattern: String,
        task_category: String,
        actions: Vec<ContributionAction>,
        submitted_at: u64,
    ) -> Result<ContributionRecordResponse, ContributionStoreError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ContributionStoreError("contribution store lock poisoned".into()))?;
        create_record(
            &mut state,
            account_id,
            url_pattern,
            task_category,
            actions,
            submitted_at,
        )
    }

    fn list_for_account(
        &self,
        account_id: &str,
    ) -> Result<Vec<ContributionRecordResponse>, ContributionStoreError> {
        let state = self
            .state
            .lock()
            .map_err(|_| ContributionStoreError("contribution store lock poisoned".into()))?;
        Ok(list_records(&state, account_id))
    }

    fn all_for_queue(&self) -> Result<Vec<OwnedContribution>, ContributionStoreError> {
        let state = self
            .state
            .lock()
            .map_err(|_| ContributionStoreError("contribution store lock poisoned".into()))?;
        Ok(all_queue_records(&state))
    }

    fn all_for_review(&self) -> Result<Vec<ReviewContribution>, ContributionStoreError> {
        let state = self
            .state
            .lock()
            .map_err(|_| ContributionStoreError("contribution store lock poisoned".into()))?;
        Ok(all_review_records(&state))
    }

    fn claim_for_review(
        &self,
        id: &str,
    ) -> Result<Option<ReviewContribution>, ContributionStoreError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ContributionStoreError("contribution store lock poisoned".into()))?;
        claim_review_record(&mut state, id)
    }

    fn decide_for_review(
        &self,
        id: &str,
        review_generation: u64,
        decision: ContributionReviewDecision,
        reviewer: &str,
        decided_at: u64,
    ) -> Result<Option<ContributionRecordResponse>, ContributionStoreError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ContributionStoreError("contribution store lock poisoned".into()))?;
        Ok(decide_review_record(
            &mut state,
            id,
            review_generation,
            decision,
            reviewer,
            decided_at,
        ))
    }

    fn review_audit_history(&self) -> Result<Vec<ReviewAuditRecord>, ContributionStoreError> {
        let state = self
            .state
            .lock()
            .map_err(|_| ContributionStoreError("contribution store lock poisoned".into()))?;
        Ok(all_audit_records(&state))
    }

    fn get_for_account(
        &self,
        account_id: &str,
        id: &str,
    ) -> Result<Option<ContributionRecordResponse>, ContributionStoreError> {
        let state = self
            .state
            .lock()
            .map_err(|_| ContributionStoreError("contribution store lock poisoned".into()))?;
        Ok(get_record(&state, account_id, id))
    }

    fn withdraw_for_account(
        &self,
        account_id: &str,
        id: &str,
    ) -> Result<bool, ContributionStoreError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ContributionStoreError("contribution store lock poisoned".into()))?;
        Ok(withdraw_record(&mut state, account_id, id))
    }
}
