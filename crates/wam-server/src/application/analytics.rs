use super::{SCHEMA_VERSION, ServiceError, WamService, now_unix};
use crate::{ports::Repository, rate_limit::utc_dates};
use std::collections::BTreeMap;
use wam_contracts::{AuthoredMemoryAnalytics, MemoryAnalyticsPoint, MemoryAnalyticsResponse};

const MAX_RANGE_DAYS: i64 = 90;

#[derive(Clone, Copy)]
enum Granularity {
    Day,
    Week,
}

impl Granularity {
    fn parse(value: Option<&str>) -> Result<Self, ServiceError> {
        match value.unwrap_or("day") {
            "day" => Ok(Self::Day),
            "week" => Ok(Self::Week),
            _ => Err(ServiceError::InvalidInput("invalid_bucket")),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Week => "week",
        }
    }

    fn period_day(self, day: i64) -> i64 {
        match self {
            Self::Day => day,
            Self::Week => day - (day + 3).rem_euclid(7),
        }
    }
}

impl<R: Repository> WamService<R> {
    pub fn memory_analytics(
        &self,
        account_id: &str,
        from: Option<&str>,
        to: Option<&str>,
        bucket: Option<&str>,
    ) -> Result<MemoryAnalyticsResponse, ServiceError> {
        let today = utc_dates(now_unix(), 1)
            .into_iter()
            .next()
            .ok_or(ServiceError::Internal)?;
        let today_day = parse_date(&today).ok_or(ServiceError::Internal)?;
        let from_day = from
            .map(|value| parse_date(value).ok_or(ServiceError::InvalidInput("invalid_date")))
            .transpose()?;
        let to_day = to
            .map(|value| parse_date(value).ok_or(ServiceError::InvalidInput("invalid_date")))
            .transpose()?;
        let to_day = to_day.unwrap_or(today_day);
        let from_day = from_day.unwrap_or(to_day - 29);
        if from_day > to_day || to_day > today_day || to_day - from_day + 1 > MAX_RANGE_DAYS {
            return Err(ServiceError::InvalidInput("invalid_date_range"));
        }
        let granularity = Granularity::parse(bucket)?;
        let from_date = format_date(from_day);
        let to_date = format_date(to_day);
        let authored = self
            .repository
            .try_authored_memory_versions(account_id)
            .map_err(|error| match error {
                crate::ports::RepositoryError::AccountDisabled => ServiceError::Unauthorized,
                _ => ServiceError::Unavailable,
            })?;
        let version_ids = authored
            .iter()
            .map(|memory| memory.version_id.clone())
            .collect::<Vec<_>>();
        let usage = self
            .usage
            .memory_usage(&version_ids, &from_date, &to_date)
            .map_err(|_| ServiceError::Internal)?;

        let period_days = (from_day..=to_day)
            .map(|day| granularity.period_day(day))
            .collect::<std::collections::BTreeSet<_>>();
        let mut results = Vec::with_capacity(authored.len());
        for memory in authored {
            let mut points = BTreeMap::new();
            for day in &period_days {
                points.insert(
                    *day,
                    MemoryAnalyticsPoint {
                        period_start: format_date(*day),
                        anonymous_recall_hits: 0,
                        other_account_recall_hits: 0,
                        anonymous_detail_views: 0,
                        other_account_detail_views: 0,
                        success_outcomes: 0,
                        partial_outcomes: 0,
                        failed_outcomes: 0,
                    },
                );
            }
            for aggregate in usage.iter().filter(|aggregate| {
                aggregate.version_id == memory.version_id && aggregate.memory_id == memory.memory_id
            }) {
                let Some(day) = parse_date(&aggregate.utc_date) else {
                    continue;
                };
                let period = granularity.period_day(day);
                let Some(point) = points.get_mut(&period) else {
                    continue;
                };
                point.anonymous_recall_hits = point
                    .anonymous_recall_hits
                    .saturating_add(aggregate.anonymous_recall_hits);
                point.other_account_recall_hits = point
                    .other_account_recall_hits
                    .saturating_add(aggregate.other_account_recall_hits);
                point.anonymous_detail_views = point
                    .anonymous_detail_views
                    .saturating_add(aggregate.anonymous_detail_views);
                point.other_account_detail_views = point
                    .other_account_detail_views
                    .saturating_add(aggregate.other_account_detail_views);
                point.success_outcomes = point
                    .success_outcomes
                    .saturating_add(aggregate.success_outcomes);
                point.partial_outcomes = point
                    .partial_outcomes
                    .saturating_add(aggregate.partial_outcomes);
                point.failed_outcomes = point
                    .failed_outcomes
                    .saturating_add(aggregate.failed_outcomes);
            }
            results.push(AuthoredMemoryAnalytics {
                memory_id: memory.memory_id,
                version_id: memory.version_id,
                title: memory.title,
                site: memory.site,
                points: points.into_values().collect(),
            });
        }
        Ok(MemoryAnalyticsResponse {
            schema_version: SCHEMA_VERSION,
            from_date,
            to_date,
            bucket: granularity.name().into(),
            memories: results,
        })
    }
}

fn parse_date(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let year = value.get(0..4)?.parse::<i64>().ok()?;
    let month = value.get(5..7)?.parse::<i64>().ok()?;
    let date = value.get(8..10)?.parse::<i64>().ok()?;
    if !(1970..=9999).contains(&year) || !(1..=12).contains(&month) {
        return None;
    }
    let day = days_from_civil(year, month, date);
    (format_date(day) == value).then_some(day)
}

fn days_from_civil(mut year: i64, month: i64, date: i64) -> i64 {
    year -= i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let adjusted_month = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * adjusted_month + 2) / 5 + date - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn format_date(day: i64) -> String {
    let shifted = day + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let date = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    format!("{year:04}-{month:02}-{date:02}")
}

#[cfg(test)]
mod tests {
    use super::{days_from_civil, format_date, parse_date};

    #[test]
    fn date_parser_rejects_invalid_calendar_dates_and_round_trips_leap_days() {
        assert!(parse_date("2025-02-29").is_none());
        assert!(parse_date("2024-02-30").is_none());
        let day = parse_date("2024-02-29").expect("valid leap day");
        assert_eq!(format_date(day), "2024-02-29");
        assert_eq!(days_from_civil(1970, 1, 1), 0);
    }
}
