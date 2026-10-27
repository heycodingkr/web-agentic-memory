use super::*;

impl<R: Repository> WamService<R> {
    pub fn leaderboard(&self) -> Result<LeaderboardResponse, ServiceError> {
        let accounts = self.ranking_accounts()?;
        let entries = public_leaderboard(&accounts, 100)
            .into_iter()
            .map(|entry| LeaderboardEntry {
                rank: entry.rank(),
                handle: entry.handle().as_str().to_owned(),
                total_points: entry.total_points(),
            })
            .collect();
        Ok(LeaderboardResponse {
            schema_version: SCHEMA_VERSION,
            entries,
        })
    }

    fn ranking_accounts(&self) -> Result<Vec<wam_core::leaderboard::RankingAccount>, ServiceError> {
        self.repository
            .try_ranking_accounts().map_err(|_| ServiceError::Unavailable)?
            .iter()
            .map(|account| {
                let ledger_points = self
                    .scores
                    .total_points_for(account.key())
                    .map_err(|_| ServiceError::Internal)?;
                Ok(account.with_total_points(account.total_points().saturating_add(ledger_points)))
            })
            .collect()
    }

    pub fn public_profile(&self, raw_handle: &str) -> Result<ProfileResponse, ServiceError> {
        let handle = PublicHandle::parse(raw_handle).map_err(|_| ServiceError::NotFound)?;
        let snapshot = self
            .repository
            .try_profile_by_handle(&handle).map_err(|_| ServiceError::Unavailable)?
            .filter(|p| p.public_opt_in)
            .ok_or(ServiceError::NotFound)?;
        Ok(profile_response(&snapshot.profile, true))
    }

    pub fn my_profile(&self, account_id: &str) -> Result<ProfileResponse, ServiceError> {
        let snapshot = self
            .repository
            .try_profile_for_account(account_id).map_err(|_| ServiceError::Unavailable)?
            .ok_or(ServiceError::NotFound)?;
        Ok(profile_response(&snapshot.profile, snapshot.public_opt_in))
    }

    pub fn save_my_profile(
        &self,
        account_id: &str,
        input: ProfilePutRequest,
    ) -> Result<ProfileResponse, ServiceError> {
        let (profile, public_opt_in) =
            validate_profile(input).map_err(|error| ServiceError::InvalidInput(error.code()))?;
        self.repository
            .save_profile(account_id, profile.clone(), public_opt_in)
            .map_err(|error| match error {
                RepositoryError::HandleTaken => ServiceError::Conflict,
                RepositoryError::AccountMissing => ServiceError::NotFound,
                RepositoryError::Unavailable => ServiceError::Unavailable,
                RepositoryError::AccountDisabled => ServiceError::Unauthorized,
            })?;
        Ok(profile_response(&profile, public_opt_in))
    }

    pub fn ensure_account(&self, account_id: &str) -> Result<(), ServiceError> {
        self.repository
            .ensure_account(account_id)
            .map_err(|error| match error {
                RepositoryError::AccountMissing => ServiceError::NotFound,
                RepositoryError::Unavailable => ServiceError::Unavailable,
                RepositoryError::AccountDisabled => ServiceError::Unauthorized,
                RepositoryError::HandleTaken => ServiceError::Conflict,
            })
    }

    pub fn dashboard(&self, account_id: &str) -> Result<DashboardResponse, ServiceError> {
        let accounts = self.ranking_accounts()?;
        let account = accounts
            .iter()
            .find(|account| account.key() == account_id)
            .ok_or(ServiceError::NotFound)?;
        Ok(DashboardResponse {
            schema_version: SCHEMA_VERSION,
            total_points: account.total_points(),
            overall_rank: personal_rank(account_id, &accounts).map(|rank| rank.rank()),
            public_rank: personal_public_rank(account_id, &accounts).map(|rank| rank.rank()),
        })
    }

    pub fn score_events(
        &self,
        account_id: &str,
        limit: usize,
        before: Option<&str>,
    ) -> Result<ScoreEventListResponse, ServiceError> {
        if !(1..=100).contains(&limit) {
            return Err(ServiceError::InvalidInput("invalid_limit"));
        }
        if let Some(cursor) = before
            && (cursor.len() > 64
                || !cursor.strip_prefix("score-").is_some_and(|id| {
                    !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit())
                }))
        {
            return Err(ServiceError::InvalidInput("invalid_cursor"));
        }
        self.scores
            .events_for_account(account_id, before, limit)
            .map_err(|_| ServiceError::Internal)?
            .ok_or(ServiceError::NotFound)
    }
}
