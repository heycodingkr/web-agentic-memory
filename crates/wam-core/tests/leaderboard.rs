use wam_core::leaderboard::{
    RankingAccount, personal_public_rank, personal_rank, public_leaderboard,
};
use wam_core::profile::PublicHandle;

fn account(key: &str, score: i64, handle: Option<&str>, listed: bool) -> RankingAccount {
    RankingAccount::new(
        key,
        score,
        handle.map(|value| PublicHandle::parse(value).unwrap()),
        listed,
    )
}

#[test]
fn public_board_includes_only_opted_in_profiles_with_positive_scores() {
    let accounts = vec![
        account("private", 100, Some("private_user"), false),
        account("listed", 80, Some("listed_user"), true),
        account("missing-handle", 70, None, true),
        account("no-score", 0, Some("zero_user"), true),
        account("negative", -2, Some("reversed_user"), true),
    ];

    let entries = public_leaderboard(&accounts, 20);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].rank(), 1);
    assert_eq!(entries[0].handle().as_str(), "listed_user");
    assert_eq!(entries[0].total_points(), 80);
}

#[test]
fn ties_share_competition_rank_and_order_is_deterministic() {
    let accounts = vec![
        account("c", 25, Some("charlie"), true),
        account("b", 25, Some("bravo"), true),
        account("a", 40, Some("alpha"), true),
        account("d", 10, Some("delta"), true),
    ];

    let entries = public_leaderboard(&accounts, 100);
    let values: Vec<_> = entries
        .iter()
        .map(|entry| (entry.rank(), entry.handle().as_str(), entry.total_points()))
        .collect();
    assert_eq!(
        values,
        vec![
            (1, "alpha", 40),
            (2, "bravo", 25),
            (2, "charlie", 25),
            (4, "delta", 10),
        ]
    );
    assert_eq!(public_leaderboard(&accounts, 1).len(), 1);
}

#[test]
fn dashboard_rank_is_private_and_includes_opted_out_accounts() {
    let accounts = vec![
        account("a", 100, Some("a_user"), false),
        account("b", 80, Some("b_user"), true),
        account("c", 80, None, false),
        account("zero", 0, Some("zero_user"), true),
    ];

    let private = personal_rank("b", &accounts).unwrap();
    assert_eq!(private.rank(), 2);
    assert_eq!(private.total_points(), 80);
    assert_eq!(private.ranked_accounts(), 3);

    let public = personal_public_rank("b", &accounts).unwrap();
    assert_eq!(public.rank(), 1);
    assert_eq!(public.ranked_accounts(), 1);

    assert!(personal_rank("zero", &accounts).is_none());
    assert!(personal_rank("missing", &accounts).is_none());
    assert!(personal_public_rank("a", &accounts).is_none());
}
