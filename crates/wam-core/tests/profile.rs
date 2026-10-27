use wam_core::profile::{
    BioBlock, BioDocument, BioInline, CreatorProfile, ExternalUrl, ProfileError, PublicHandle,
    ServiceLink,
};

#[test]
fn handles_are_case_insensitive_and_reserve_routes() {
    let handle = PublicHandle::parse("  Agent_42 ").unwrap();
    assert_eq!(handle.as_str(), "agent_42");
    assert_eq!(
        PublicHandle::parse("admin").unwrap_err(),
        ProfileError::ReservedHandle
    );
    assert_eq!(
        PublicHandle::parse("ab").unwrap_err(),
        ProfileError::InvalidHandle
    );
    assert_eq!(
        PublicHandle::parse("real.name@example.com")
            .unwrap_err()
            .code(),
        "invalid_handle"
    );
}

#[test]
fn external_urls_allow_public_https_only_and_reject_credentials_and_tokens() {
    let url = ExternalUrl::parse("https://example.com/service?view=public#about").unwrap();
    assert_eq!(
        url.as_str(),
        "https://example.com/service?view=public#about"
    );

    for unsafe_url in [
        "javascript:alert(1)",
        "http://example.com",
        "https://user:pass@example.com",
        "https://localhost/about",
        "https://127.0.0.1/admin",
        "https://example.com/profile?access_token=sentinel",
        "https://service.internal/about",
    ] {
        assert_eq!(
            ExternalUrl::parse(unsafe_url).unwrap_err(),
            ProfileError::InvalidExternalUrl,
            "{unsafe_url}"
        );
    }
}

#[test]
fn rich_bio_is_typed_bounded_and_accepts_only_safe_link_targets() {
    let destination = ExternalUrl::parse("https://example.com/work").unwrap();
    let bio = BioDocument::try_new(vec![
        BioBlock::Heading {
            level: 2,
            content: vec![BioInline::Strong("Design engineer".to_owned())],
        },
        BioBlock::Paragraph(vec![
            BioInline::Text("I build ".to_owned()),
            BioInline::Emphasis("useful tools".to_owned()),
            BioInline::Text(".".to_owned()),
            BioInline::Link {
                text: "My service".to_owned(),
                url: destination,
            },
        ]),
        BioBlock::BulletList(vec![vec![BioInline::Text("Korean and English".to_owned())]]),
    ])
    .unwrap();

    assert_eq!(bio.blocks().len(), 3);
    assert_eq!(bio.plain_text_characters(), 64);
    assert_eq!(
        BioDocument::try_new(vec![BioBlock::Heading {
            level: 1,
            content: vec![BioInline::Text("Page title".to_owned())],
        }])
        .unwrap_err(),
        ProfileError::InvalidBio
    );
    assert_eq!(
        BioDocument::try_new(vec![BioBlock::Paragraph(vec![BioInline::Text(
            " ".to_owned()
        ),])])
        .unwrap_err(),
        ProfileError::InvalidBio
    );
}

#[test]
fn rich_bio_enforces_character_block_and_link_limits() {
    let text = "x".repeat(1_201);
    assert_eq!(
        BioDocument::try_new(vec![BioBlock::Paragraph(vec![BioInline::Text(text)])]).unwrap_err(),
        ProfileError::BioTooLarge
    );

    let too_many_links = (0..9)
        .map(|_| BioInline::Link {
            text: "service".to_owned(),
            url: ExternalUrl::parse("https://example.com").unwrap(),
        })
        .collect();
    assert_eq!(
        BioDocument::try_new(vec![BioBlock::Paragraph(too_many_links)]).unwrap_err(),
        ProfileError::BioTooLarge
    );

    let too_many_blocks = vec![BioBlock::Paragraph(vec![BioInline::Text("x".to_owned())]); 21];
    assert_eq!(
        BioDocument::try_new(too_many_blocks).unwrap_err(),
        ProfileError::BioTooLarge
    );
}

#[test]
fn profile_limits_service_links_and_detects_duplicates() {
    let handle = PublicHandle::parse("creator_1").unwrap();
    let bio = BioDocument::default();
    let link =
        |label: &str, url: &str| ServiceLink::new(label, ExternalUrl::parse(url).unwrap()).unwrap();

    assert_eq!(
        CreatorProfile::try_new(
            handle.clone(),
            bio.clone(),
            vec![
                link("Site", "https://example.com"),
                link("Duplicate", "https://example.com"),
            ],
        )
        .unwrap_err(),
        ProfileError::DuplicateServiceLink
    );

    let too_many = (0..6)
        .map(|index| {
            link(
                &format!("Site {index}"),
                &format!("https://site{index}.example.com"),
            )
        })
        .collect();
    assert_eq!(
        CreatorProfile::try_new(handle, bio, too_many).unwrap_err(),
        ProfileError::TooManyServiceLinks
    );
}

#[test]
fn profile_errors_do_not_echo_untrusted_values() {
    let error = ExternalUrl::parse("javascript:do_not_echo_secret").unwrap_err();
    assert_eq!(error.to_string(), "invalid_external_url");
}
