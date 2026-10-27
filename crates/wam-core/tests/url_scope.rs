use wam_core::{CanonicalUrl, RouteCoverage, RouteMatchLevel, UrlMode, UrlPolicyError};

#[test]
fn strips_userinfo_query_fragment_and_default_port() {
    let normalized = CanonicalUrl::parse(
        "HTTPS://alice:secret@Medium.COM:443/editor/post/948203?access_token=fake#private",
        UrlMode::Public,
    )
    .unwrap();

    assert_eq!(normalized.host(), "medium.com");
    assert_eq!(normalized.path(), "/editor/post/:id");
    let safe_projection = format!("{}{}", normalized.host(), normalized.path());
    assert!(!safe_projection.contains("alice"));
    assert!(!safe_projection.contains("secret"));
    assert!(!safe_projection.contains("token"));
    assert!(!safe_projection.contains("private"));
}

#[test]
fn canonicalizes_encoded_unreserved_path_and_dynamic_ids() {
    let first = CanonicalUrl::parse(
        "https://medium.com/editor/%70ost/%31%32%33/",
        UrlMode::Public,
    )
    .unwrap();
    let second = CanonicalUrl::parse(
        "https://medium.com/editor/post/550e8400-e29b-41d4-a716-446655440000",
        UrlMode::Public,
    )
    .unwrap();

    assert_eq!(first.path(), "/editor/post/:id");
    assert_eq!(second.path(), "/editor/post/:id");
}

#[test]
fn account_email_path_is_replaced_without_retaining_identity() {
    let normalized = CanonicalUrl::parse(
        "https://medium.com/users/reader%40example.com/posts",
        UrlMode::Public,
    )
    .unwrap();

    assert_eq!(normalized.path(), "/users/:account/posts");
}

#[test]
fn host_adapter_rules_redact_opaque_account_names() {
    use wam_core::DynamicSegmentKind;

    let normalized = CanonicalUrl::parse_with_dynamic_segments(
        "https://medium.com/users/reader-42/posts",
        UrlMode::Public,
        &[(1, DynamicSegmentKind::Account)],
    )
    .unwrap();

    assert_eq!(normalized.path(), "/users/:account/posts");
    assert_eq!(
        CanonicalUrl::parse_with_dynamic_segments(
            "https://medium.com/users/reader-42/posts",
            UrlMode::Public,
            &[(9, DynamicSegmentKind::Account)],
        ),
        Err(UrlPolicyError::AmbiguousPath)
    );
}

#[test]
fn test_mode_accepts_only_loopback_http() {
    let fixture = CanonicalUrl::parse(
        "http://127.0.0.1:5173/editor/post/123",
        UrlMode::LoopbackTest,
    )
    .unwrap();

    assert_eq!(fixture.host(), "127.0.0.1:5173");
    assert_eq!(fixture.path(), "/editor/post/:id");

    assert_eq!(
        CanonicalUrl::parse("http://medium.com/editor", UrlMode::LoopbackTest),
        Err(UrlPolicyError::HostNotPublic)
    );
    assert_eq!(
        CanonicalUrl::parse("https://127.0.0.1/editor", UrlMode::Public),
        Err(UrlPolicyError::HostNotPublic)
    );
}

#[test]
fn unsafe_scheme_private_domain_and_ambiguous_segments_fail_closed() {
    assert_eq!(
        CanonicalUrl::parse("http://medium.com/editor", UrlMode::Public),
        Err(UrlPolicyError::UnsupportedScheme)
    );
    assert_eq!(
        CanonicalUrl::parse("https://service.test/editor", UrlMode::Public),
        Err(UrlPolicyError::HostNotPublic)
    );
    assert_eq!(
        CanonicalUrl::parse("https://medium.com/editor/a%2Fadmin", UrlMode::Public),
        Err(UrlPolicyError::AmbiguousPath)
    );
    assert_eq!(
        CanonicalUrl::parse(
            "https://medium.com/reset/AbCdEf12345678901234567890123456",
            UrlMode::Public
        ),
        Err(UrlPolicyError::SensitivePath)
    );
}

#[test]
fn subtree_matching_is_explicit_and_segment_bounded() {
    let base = CanonicalUrl::parse("https://medium.com/editor/post/123", UrlMode::Public).unwrap();
    let child = CanonicalUrl::parse(
        "https://medium.com/editor/post/123/comments",
        UrlMode::Public,
    )
    .unwrap();
    let sibling_prefix =
        CanonicalUrl::parse("https://medium.com/editor/posting/123", UrlMode::Public).unwrap();

    let exact = base.exact_pattern();
    let subtree = base.subtree_pattern();
    let host = base.host_pattern();

    assert_eq!(exact.coverage(), RouteCoverage::Exact);
    assert_eq!(exact.match_level(&child), None);
    assert_eq!(subtree.match_level(&child), Some(RouteMatchLevel::Parent));
    assert_eq!(subtree.match_level(&sibling_prefix), None);
    assert_eq!(host.match_level(&child), Some(RouteMatchLevel::Host));
}
