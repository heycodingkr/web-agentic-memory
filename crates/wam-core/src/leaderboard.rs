//! Pure ranking projections for private dashboards and the public opt-in board.

use crate::profile::PublicHandle;

const MAX_PUBLIC_PAGE_SIZE: usize = 100;

#[derive(Clone, Eq, PartialEq)]
pub struct RankingAccount {
    account_key: String,
    total_points: i64,
    handle: Option<PublicHandle>,
    public_opt_in: bool,
}

impl RankingAccount {
    /// account_key is an internal stable key; it is never returned by ranking APIs.
    /// Callers pass only accounts eligible under the current server ranking policy.
    pub fn new(
        account_key: impl Into<String>,
        total_points: i64,
        handle: Option<PublicHandle>,
        public_opt_in: bool,
    ) -> Self {
        Self {
            account_key: account_key.into(),
            total_points,
            handle,
            public_opt_in,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeaderboardEntry {
    rank: usize,
    handle: PublicHandle,
    total_points: i64,
}

impl LeaderboardEntry {
    pub fn rank(&self) -> usize {
        self.rank
    }

    pub fn handle(&self) -> &PublicHandle {
        &self.handle
    }

    pub fn total_points(&self) -> i64 {
        self.total_points
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PersonalRank {
    rank: usize,
    total_points: i64,
    ranked_accounts: usize,
}

impl PersonalRank {
    pub fn rank(&self) -> usize {
        self.rank
    }

    pub fn total_points(&self) -> i64 {
        self.total_points
    }

    pub fn ranked_accounts(&self) -> usize {
        self.ranked_accounts
    }
}

/// Returns the signed-in account's private rank among all positive-score accounts.
/// Equal totals share a competition rank (1, 1, 3). No other account data is returned.
pub fn personal_rank(account_key: &str, accounts: &[RankingAccount]) -> Option<PersonalRank> {
    rank_for(account_key, accounts, |_| true)
}

/// Returns the signed-in account's rank in the public opt-in cohort, if listed.
pub fn personal_public_rank(
    account_key: &str,
    accounts: &[RankingAccount],
) -> Option<PersonalRank> {
    rank_for(account_key, accounts, |account| {
        account.public_opt_in && account.handle.is_some()
    })
}

fn rank_for(
    account_key: &str,
    accounts: &[RankingAccount],
    eligible: impl Fn(&RankingAccount) -> bool,
) -> Option<PersonalRank> {
    let target = accounts
        .iter()
        .find(|account| account.account_key == account_key)?;
    if target.total_points <= 0 || !eligible(target) {
        return None;
    }

    let ranked_accounts: Vec<_> = accounts
        .iter()
        .filter(|account| account.total_points > 0 && eligible(account))
        .collect();
    let rank = ranked_accounts
        .iter()
        .filter(|account| account.total_points > target.total_points)
        .count()
        + 1;

    Some(PersonalRank {
        rank,
        total_points: target.total_points,
        ranked_accounts: ranked_accounts.len(),
    })
}

/// Public rows are limited to opted-in profiles with a handle and positive points.
/// Ties share rank; handle order makes equal-score rows deterministic.
pub fn public_leaderboard(accounts: &[RankingAccount], limit: usize) -> Vec<LeaderboardEntry> {
    let mut listed: Vec<_> = accounts
        .iter()
        .filter(|account| {
            account.public_opt_in && account.handle.is_some() && account.total_points > 0
        })
        .collect();
    listed.sort_by(|left, right| {
        right
            .total_points
            .cmp(&left.total_points)
            .then_with(|| left.handle.as_ref().cmp(&right.handle.as_ref()))
            .then_with(|| left.account_key.cmp(&right.account_key))
    });

    let mut previous_points = None;
    let mut current_rank = 0;
    listed
        .iter()
        .take(limit.min(MAX_PUBLIC_PAGE_SIZE))
        .enumerate()
        .filter_map(|(index, account)| {
            if previous_points != Some(account.total_points) {
                current_rank = index + 1;
                previous_points = Some(account.total_points);
            }
            account.handle.as_ref().map(|handle| LeaderboardEntry {
                rank: current_rank,
                handle: handle.clone(),
                total_points: account.total_points,
            })
        })
        .collect()
}
