//! Endpoint validation, public URL normalization, and encoded route construction.

use crate::ApiError;
use wam_core::url_scope::{CanonicalUrl, UrlMode};

pub fn normalize_endpoint(input: &str) -> Result<String, ApiError> {
    let url = reqwest::Url::parse(input).map_err(|_| ApiError::InvalidEndpoint)?;
    let loopback = url.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    });
    if url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !(url.scheme() == "https" || (url.scheme() == "http" && loopback))
    {
        return Err(ApiError::InvalidEndpoint);
    }
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

pub(crate) fn canonical_public_url(input: &str) -> Result<String, ApiError> {
    let url = CanonicalUrl::parse(input, UrlMode::Public).map_err(|_| ApiError::InvalidInput)?;
    Ok(format!("https://{}{}", url.host(), url.path()))
}

pub(crate) fn route_url(endpoint: &str, segments: &[&str]) -> Result<reqwest::Url, ApiError> {
    let mut url = reqwest::Url::parse(endpoint).map_err(|_| ApiError::InvalidEndpoint)?;
    {
        let mut path = url
            .path_segments_mut()
            .map_err(|_| ApiError::InvalidEndpoint)?;
        path.pop_if_empty().extend(segments.iter().copied());
    }
    Ok(url)
}
