//! Authenticated request preparation, transport errors, and response decoding.

use crate::endpoint::route_url;
use crate::telemetry::{classify_route, random_id};
use crate::{ApiError, DiagnosticError, DiagnosticOperation, HttpClient};
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use std::time::Instant;

impl HttpClient {
    pub(crate) async fn get<T: DeserializeOwned>(
        &self,
        segments: &[&str],
        query: Option<(&str, &str)>,
    ) -> Result<T, ApiError> {
        let mut url = route_url(&self.endpoint, segments)?;
        if let Some((key, value)) = query {
            url.query_pairs_mut().append_pair(key, value);
        }
        let request = self.client.get(url);
        self.send(request).await
    }

    pub(crate) async fn post_json<T: DeserializeOwned, B: serde::Serialize>(
        &self,
        segments: &[&str],
        body: &B,
    ) -> Result<T, ApiError> {
        let url = route_url(&self.endpoint, segments)?;
        self.send(self.client.post(url).json(body)).await
    }

    pub(crate) async fn send_empty(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<(), ApiError> {
        let (request, operation, trace_id) = self.prepare_request(request)?;
        let started = Instant::now();
        let response = match self.client.execute(request).await {
            Ok(response) => response,
            Err(_) => {
                self.report_error(
                    operation,
                    DiagnosticError::NetworkError,
                    trace_id,
                    None,
                    started,
                )
                .await;
                return Err(ApiError::Transport);
            }
        };
        let trace_id = response_trace_id(&response, trace_id);
        if response.status().is_success() {
            self.drain_telemetry().await;
            Ok(())
        } else {
            self.report_error(
                operation,
                DiagnosticError::HttpError,
                trace_id,
                Some(response.status().as_u16()),
                started,
            )
            .await;
            Err(error_for_response(&response))
        }
    }

    pub(crate) async fn send<T: DeserializeOwned>(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<T, ApiError> {
        let (request, operation, trace_id) = self.prepare_request(request)?;
        let started = Instant::now();
        let response = match self.client.execute(request).await {
            Ok(response) => response,
            Err(_) => {
                self.report_error(
                    operation,
                    DiagnosticError::NetworkError,
                    trace_id,
                    None,
                    started,
                )
                .await;
                return Err(ApiError::Transport);
            }
        };
        let trace_id = response_trace_id(&response, trace_id);
        let status = response.status();
        let result = decode(response).await;
        if let Err(error) = &result {
            let kind = if *error == ApiError::InvalidResponse {
                DiagnosticError::InvalidResponse
            } else {
                DiagnosticError::HttpError
            };
            self.report_error(operation, kind, trace_id, Some(status.as_u16()), started)
                .await;
        }
        if result.is_ok() {
            self.drain_telemetry().await;
        }
        result
    }

    pub(crate) fn prepare_request(
        &self,
        mut request: reqwest::RequestBuilder,
    ) -> Result<(reqwest::Request, DiagnosticOperation, String), ApiError> {
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let mut request = request.build().map_err(|_| ApiError::Transport)?;
        let operation = classify_route(request.url().path());
        let trace_id = random_id().ok_or(ApiError::Transport)?;
        request.headers_mut().insert(
            "x-wam-client",
            reqwest::header::HeaderValue::from_static(self.diagnostic_source.as_str()),
        );
        request.headers_mut().insert(
            "x-wam-trace-id",
            reqwest::header::HeaderValue::from_str(&trace_id).map_err(|_| ApiError::Transport)?,
        );
        Ok((request, operation, trace_id))
    }
}

pub(crate) fn response_trace_id(response: &reqwest::Response, request_trace: String) -> String {
    response
        .headers()
        .get("x-wam-trace-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| {
            value.len() == 32
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
        .map(str::to_owned)
        .unwrap_or(request_trace)
}

fn error_for_status(status: StatusCode, retry_after: Option<&str>) -> ApiError {
    match status {
        StatusCode::TOO_MANY_REQUESTS => ApiError::RateLimited {
            retry_after_seconds: retry_after
                .and_then(|value| value.trim().parse::<u64>().ok())
                .map(|seconds| seconds.max(1)),
        },
        StatusCode::UNAUTHORIZED => ApiError::Unauthorized,
        StatusCode::FORBIDDEN => ApiError::Forbidden,
        StatusCode::NOT_FOUND => ApiError::NotFound,
        StatusCode::CONFLICT => ApiError::Conflict,
        StatusCode::BAD_REQUEST => ApiError::InvalidInput,
        _ => ApiError::Server,
    }
}

pub(crate) fn error_for_response(response: &reqwest::Response) -> ApiError {
    error_for_status(
        response.status(),
        response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok()),
    )
}

async fn decode<T: DeserializeOwned>(response: reqwest::Response) -> Result<T, ApiError> {
    if !response.status().is_success() {
        return Err(error_for_response(&response));
    }
    response.json().await.map_err(|_| ApiError::InvalidResponse)
}
