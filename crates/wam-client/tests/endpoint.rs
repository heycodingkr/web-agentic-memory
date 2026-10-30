use wam_client::{ApiError, HttpClient};

#[test]
fn endpoints_reject_secrets_ambiguous_suffixes_and_plaintext_remote_transport() {
    for endpoint in [
        "https://user:sentinel@example.com",
        "https://example.com?token=sentinel",
        "https://example.com/#sentinel",
        "http://example.com",
    ] {
        assert!(
            matches!(
                HttpClient::new(endpoint, None),
                Err(ApiError::InvalidEndpoint)
            ),
            "unsafe endpoint was accepted"
        );
    }
    for endpoint in [
        "http://127.0.0.1:3000",
        "http://[::1]:3000",
        "https://example.com/api/",
    ] {
        assert!(HttpClient::new(endpoint, None).is_ok());
    }
}
