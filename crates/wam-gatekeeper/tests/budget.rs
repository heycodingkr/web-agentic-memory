//! The model budget: request and token buckets plus a daily credit ceiling (docs/test QUE-08).

use wam_gatekeeper::budget::{BudgetConfig, BudgetWait, ModelBudget};

fn budget(requests: u32, tokens: u64, daily: Option<f64>) -> ModelBudget {
    ModelBudget::new(
        BudgetConfig {
            requests_per_minute: requests,
            tokens_per_minute: tokens,
            daily_credits: daily,
        },
        0,
    )
}

#[test]
fn the_request_bucket_allows_a_burst_then_refills_steadily() {
    let mut b = budget(6, 1_000_000, None);
    for _ in 0..6 {
        b.try_acquire(0, 100).unwrap();
    }
    assert_eq!(
        b.try_acquire(0, 100),
        Err(BudgetWait {
            retry_after_secs: 10,
            daily: false
        }),
        "6 per minute: one per 10 s"
    );
    assert_eq!(
        b.try_acquire(9, 100),
        Err(BudgetWait {
            retry_after_secs: 1,
            daily: false
        })
    );
    b.try_acquire(10, 100).unwrap();
    assert!(b.try_acquire(10, 100).is_err());
}

#[test]
fn the_token_bucket_limits_big_passes_even_when_requests_are_free() {
    let mut b = budget(600, 6_000, None);
    b.try_acquire(0, 5_000).unwrap();
    let wait = b.try_acquire(0, 5_000).unwrap_err();
    assert_eq!(
        wait,
        BudgetWait {
            retry_after_secs: 40,
            daily: false
        },
        "4000 missing tokens at 100 per second"
    );
    b.try_acquire(40, 5_000).unwrap();
}

#[test]
fn a_pass_bigger_than_the_whole_bucket_can_still_run_when_the_bucket_is_full() {
    let mut b = budget(60, 1_000, None);
    b.try_acquire(0, 50_000).unwrap();
    assert!(
        b.try_acquire(0, 1).is_err(),
        "it was charged the whole bucket"
    );
}

#[test]
fn buckets_never_hold_more_than_their_capacity() {
    let mut b = budget(2, 1_000, None);
    b.try_acquire(10_000, 100).unwrap();
    b.try_acquire(10_000, 100).unwrap();
    assert!(
        b.try_acquire(10_000, 100).is_err(),
        "idle time does not bank extra capacity"
    );
}

#[test]
fn the_daily_ceiling_blocks_until_the_next_utc_day_and_then_resets() {
    let mut b = budget(600, 10_000_000, Some(1.0));
    b.try_acquire(1_000, 100).unwrap();
    assert!(b.has_room(1_000));
    b.record_cost(1_000, 0.6);
    b.try_acquire(1_001, 100).unwrap();
    b.record_cost(1_001, 0.6);
    assert!(!b.has_room(2_000));
    assert_eq!(
        b.try_acquire(2_000, 100),
        Err(BudgetWait {
            retry_after_secs: 86_400 - 2_000,
            daily: true
        })
    );
    assert!(
        (b.spent_today() - 1.2).abs() < 1e-9,
        "the last pass may overshoot, and it is counted"
    );
    b.try_acquire(86_400, 100).unwrap();
    assert_eq!(b.spent_today(), 0.0);
}

#[test]
fn no_ceiling_means_no_daily_limit() {
    let mut b = budget(600, 10_000_000, None);
    b.record_cost(0, 1e9);
    assert!(b.has_room(1));
    b.try_acquire(1, 100).unwrap();
}

#[test]
fn a_clock_that_runs_backwards_does_not_refill_or_panic() {
    let mut b = budget(1, 1_000, None);
    b.try_acquire(100, 10).unwrap();
    assert!(b.try_acquire(50, 10).is_err());
}

#[test]
fn a_pause_from_the_provider_blocks_every_call_until_it_ends() {
    let mut b = budget(600, 10_000_000, None);
    b.pause_until(500);
    assert!(!b.has_room(100));
    assert_eq!(
        b.try_acquire(100, 10),
        Err(BudgetWait {
            retry_after_secs: 400,
            daily: false
        }),
        "full buckets do not matter while the provider says no"
    );
    assert!(b.has_room(500));
    b.try_acquire(500, 10).unwrap();
}

#[test]
fn a_pause_never_shortens_a_longer_one() {
    let mut b = budget(600, 10_000_000, None);
    b.pause_until(1_000);
    b.pause_until(300);
    assert!(!b.has_room(999));
    assert!(b.has_room(1_000));
}

#[test]
fn when_the_daily_ceiling_and_a_pause_both_apply_the_longer_wait_is_reported() {
    let mut b = budget(600, 10_000_000, Some(1.0));
    b.record_cost(1_000, 2.0);
    b.pause_until(1_500);
    assert_eq!(
        b.try_acquire(1_000, 10),
        Err(BudgetWait {
            retry_after_secs: 86_400 - 1_000,
            daily: true
        })
    );
    let mut b = budget(600, 10_000_000, Some(1.0));
    b.record_cost(86_000, 2.0);
    b.pause_until(86_400 + 3_000);
    assert_eq!(
        b.try_acquire(86_000, 10),
        Err(BudgetWait {
            retry_after_secs: 3_400,
            daily: false
        }),
        "the pause outlasts the day boundary here"
    );
}
