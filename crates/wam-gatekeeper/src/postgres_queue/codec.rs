use crate::queue::{AccountClass, Job, JobKind, JobState, Priority, QueueError};
use postgres::Row;
use std::convert::TryFrom;

pub(super) fn sql_u64(value: u64) -> Result<i64, QueueError> {
    i64::try_from(value).map_err(|_| QueueError::StorageUnavailable)
}

pub(super) fn from_sql_u64(value: i64) -> Result<u64, QueueError> {
    u64::try_from(value).map_err(|_| QueueError::StorageUnavailable)
}

pub(super) fn kind_text(kind: JobKind) -> &'static str {
    match kind {
        JobKind::Contribution => "contribution",
        JobKind::Reverify => "reverify",
        JobKind::Withdrawal => "withdrawal",
        JobKind::Rollback => "rollback",
    }
}

fn parse_kind(kind: &str) -> Result<JobKind, QueueError> {
    match kind {
        "contribution" => Ok(JobKind::Contribution),
        "reverify" => Ok(JobKind::Reverify),
        "withdrawal" => Ok(JobKind::Withdrawal),
        "rollback" => Ok(JobKind::Rollback),
        _ => Err(QueueError::StorageUnavailable),
    }
}

pub(super) fn class_text(class: AccountClass) -> &'static str {
    match class {
        AccountClass::Established => "established",
        AccountClass::New => "new",
    }
}

fn parse_class(class: &str) -> Result<AccountClass, QueueError> {
    match class {
        "established" => Ok(AccountClass::Established),
        "new" => Ok(AccountClass::New),
        _ => Err(QueueError::StorageUnavailable),
    }
}

pub(super) fn priority_text(priority: Priority) -> &'static str {
    match priority {
        Priority::P0 => "p0",
        Priority::P1 => "p1",
    }
}

fn parse_priority(priority: &str) -> Result<Priority, QueueError> {
    match priority {
        "p0" => Ok(Priority::P0),
        "p1" => Ok(Priority::P1),
        _ => Err(QueueError::StorageUnavailable),
    }
}

fn parse_state(state: &str) -> Result<JobState, QueueError> {
    match state {
        "queued" => Ok(JobState::Queued),
        "leased" => Ok(JobState::Leased),
        "waiting_scope" => Ok(JobState::WaitingScope),
        "waiting_budget" => Ok(JobState::WaitingBudget),
        "retry" => Ok(JobState::Retry),
        "done" => Ok(JobState::Done),
        "dead" => Ok(JobState::Dead),
        "canceled" => Ok(JobState::Canceled),
        _ => Err(QueueError::StorageUnavailable),
    }
}

pub(super) fn parse_job(row: Row) -> Result<Job, QueueError> {
    let contribution_id: Option<String> = row.get(6);
    let lease_expires_at: Option<i64> = row.get(13);
    let finished_at: Option<i64> = row.get(16);
    Ok(Job {
        id: from_sql_u64(row.get(0))?,
        kind: parse_kind(&row.get::<_, String>(1))?,
        scope_key: row.get(2),
        contributor: row.get(3),
        device: row.get(4),
        class: parse_class(&row.get::<_, String>(5))?,
        contribution_id,
        priority: parse_priority(&row.get::<_, String>(7))?,
        enqueued_at: from_sql_u64(row.get(8))?,
        state: parse_state(&row.get::<_, String>(9))?,
        attempts: u32::try_from(row.get::<_, i32>(10))
            .map_err(|_| QueueError::StorageUnavailable)?,
        not_before: from_sql_u64(row.get(11))?,
        lease_generation: from_sql_u64(row.get(12))?,
        lease_expires_at: lease_expires_at.map(from_sql_u64).transpose()?,
        lease_holder: row.get(14),
        attribution_withdrawn: row.get(15),
        finished_at: finished_at.map(from_sql_u64).transpose()?,
        last_error: row.get(17),
    })
}

pub(super) fn priority_rank(priority: Priority) -> u8 {
    match priority {
        Priority::P0 => 0,
        Priority::P1 => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_enum_codecs_round_trip_and_reject_unknown_values() {
        for (text, value) in [
            ("contribution", JobKind::Contribution),
            ("reverify", JobKind::Reverify),
            ("withdrawal", JobKind::Withdrawal),
            ("rollback", JobKind::Rollback),
        ] {
            assert_eq!(kind_text(value), text);
            assert_eq!(parse_kind(text), Ok(value));
        }
        assert!(matches!(
            parse_kind("unknown"),
            Err(QueueError::StorageUnavailable)
        ));

        for (text, value) in [
            ("established", AccountClass::Established),
            ("new", AccountClass::New),
        ] {
            assert_eq!(class_text(value), text);
            assert_eq!(parse_class(text), Ok(value));
        }
        assert!(matches!(
            parse_class("unknown"),
            Err(QueueError::StorageUnavailable)
        ));

        for (text, value) in [("p0", Priority::P0), ("p1", Priority::P1)] {
            assert_eq!(priority_text(value), text);
            assert_eq!(parse_priority(text), Ok(value));
        }
        assert!(matches!(
            parse_priority("unknown"),
            Err(QueueError::StorageUnavailable)
        ));

        for (text, value) in [
            ("queued", JobState::Queued),
            ("leased", JobState::Leased),
            ("waiting_scope", JobState::WaitingScope),
            ("waiting_budget", JobState::WaitingBudget),
            ("retry", JobState::Retry),
            ("done", JobState::Done),
            ("dead", JobState::Dead),
            ("canceled", JobState::Canceled),
        ] {
            assert_eq!(parse_state(text), Ok(value));
        }
        assert!(matches!(
            parse_state("unknown"),
            Err(QueueError::StorageUnavailable)
        ));
        assert_eq!(priority_rank(Priority::P0), 0);
        assert_eq!(priority_rank(Priority::P1), 1);
    }

    #[test]
    fn postgres_unsigned_integer_codecs_reject_out_of_range_values() {
        assert_eq!(sql_u64(0), Ok(0));
        assert_eq!(sql_u64(i64::MAX as u64), Ok(i64::MAX));
        assert!(matches!(
            sql_u64(i64::MAX as u64 + 1),
            Err(QueueError::StorageUnavailable)
        ));
        assert_eq!(from_sql_u64(0), Ok(0));
        assert_eq!(from_sql_u64(i64::MAX), Ok(i64::MAX as u64));
        assert!(matches!(
            from_sql_u64(-1),
            Err(QueueError::StorageUnavailable)
        ));
    }
}
