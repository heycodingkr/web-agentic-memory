use super::QueueConfig;

/// Retry timing shared by the in-memory and PostgreSQL queue backends.
pub(crate) fn retry_delay(config: &QueueConfig, job_id: u64, attempts: u32) -> u64 {
    let exponent = attempts.saturating_sub(1).min(20);
    let capped = config
        .backoff_base_secs
        .saturating_mul(1u64 << exponent)
        .min(config.backoff_cap_secs);
    // Derive jitter from the job so retries spread out but remain deterministic in tests.
    let jitter = job_id.wrapping_mul(7919) ^ u64::from(attempts).wrapping_mul(104_729);
    capped.saturating_add(jitter % (capped / 2 + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_delay_is_deterministic_and_caps_the_exponential_base_before_jitter() {
        let config = QueueConfig {
            backoff_base_secs: 4,
            backoff_cap_secs: 10,
            ..QueueConfig::default()
        };

        let first_attempt = retry_delay(&config, 19, 1);
        assert_eq!(first_attempt, retry_delay(&config, 19, 1));
        assert!((4..=6).contains(&first_attempt));
        assert!((8..=12).contains(&retry_delay(&config, 19, 2)));
        assert!((10..=15).contains(&retry_delay(&config, 19, 3)));
    }

    #[test]
    fn retry_delay_saturates_for_extreme_configuration_and_attempt_counts() {
        let config = QueueConfig {
            backoff_base_secs: u64::MAX,
            backoff_cap_secs: u64::MAX,
            ..QueueConfig::default()
        };

        assert_eq!(retry_delay(&config, u64::MAX, u32::MAX), u64::MAX);
    }
}
