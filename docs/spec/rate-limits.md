# Rate limits and abuse controls

Status: implemented in the working tree for Axum HTTP admission, Redis-backed shared buckets, quota headers, 429 feedback, and owner usage. Production Redis, ingress proxy configuration, and load behavior have not been live-verified in this environment. Values below are the current compiled defaults, not measured capacity claims.

## Scope and threat model

Protect the public memory catalog, recall and detail endpoints, authenticated accounts, and the service from scraping, credential guessing, resource exhaustion, and accidental retry storms. A client can be anonymous, authenticated with a web session, or authenticated with a linked device. IP limits remain in force after login so creating accounts does not bypass a network-level cap.

This policy limits request frequency and expensive operation cost. It does not replace request-size limits, schema validation, authorization, queue admission, database timeouts, or abuse review. Those controls remain separate.

## Requirements

| ID | Requirement | Acceptance | Tests |
|---|---|---|---|
| F-RL-01 | Apply a shared anonymous IP quota before expensive handler work | Every server replica sees the same quota state; a denied request does not reach recall, detail, or database work | RL-01, RL-03 |
| F-RL-02 | Give authenticated accounts a higher account quota while retaining an IP cap | Both the account bucket and IP bucket must allow the request; auth never bypasses the shared-IP ceiling | RL-02, RL-03, SEC-J02 |
| F-RL-03 | Weight costly operations and separately bound writes/auth challenges | Recall, expand, list, contribution, OAuth/device polling, and outcome reports use configurable policy classes; idempotent retries do not multiply writes | RL-10, RL-13~14, API-44, FAIL-33 |
| F-RL-04 | Resolve client IP only through a trusted proxy chain | Ignore forwarded-IP headers from untrusted peers; direct connections use the socket peer address | RL-04, SEC-J01 |
| F-RL-05 | Explain a limit to clients in a stable response contract | `429`, `Retry-After`, account quota capacity/refill state when safe, and a fixed `RATE_LIMITED` error code; no shared-IP remaining count or raw key/IP in body | RL-06, RL-07, WEB-27 |
| F-RL-06 | Show account usage and remaining quota to the account owner | Authenticated usage endpoint and dashboard show the current window and historical aggregates; anonymous callers cannot query another identity | USAGE-01~05, API-45~46, WEB-26~28 |
| F-RL-07 | Fail safely if the shared limiter is unavailable | Do not silently turn a multi-replica limit into per-process limits; costly/write routes fail with a retryable 503, while explicitly cacheable public content may use its bounded cache | RL-09, FAIL-33~34 |
| F-RL-08 | Minimize limiter-key and audit data | Store no raw IP or per-request URL/body; IP keys are keyed digests with TTL, account keys use internal IDs and short windows | RL-05, SEC-J03 |
| F-RL-09 | Shed load at service saturation independently of a caller quota | When the per-process in-flight API/MCP request limit is full, work is shed with 503; health checks and normal per-IP/account quotas remain distinguishable | RL-12, SEC-J03, FAIL-36 |

## Policy and initial pilot values

All numbers are configuration, versioned and observable by operators. Initial values are deliberately modest and must be load-tested before public launch. The server returns the effective policy window, not the internal Redis key or account identifier.

| Class | Anonymous | Authenticated | Notes |
|---|---:|---:|---|
| Recall | Anonymous IP 60 cost units/minute, burst 20; also shared IP 600/120 | Account 300/60 and shared IP 600/120 | Recall costs 3 units; result limit remains capped at 3 |
| Detail/expand | Anonymous IP 60/20; also shared IP 600/120 | Account 300/60 and shared IP 600/120 | Detail costs 2 units |
| Public reads and other reads | Anonymous IP 60/20; also shared IP 600/120 | Account 300/60 and shared IP 600/120 | Base read cost is 1 unit |
| Account status/usage | Status IP 30/10 | Status IP 30/10 and account 30/10 | Separate low-cost bucket remains available when a normal account bucket is depleted |
| Writes and OAuth/device POST | Anonymous IP 60/20; also shared IP 600/120 | Account 300/60 and shared IP 600/120 | Write cost is 5 units; auth and consent checks still apply |
| MCP | Same buckets and operation costs as HTTP above | Same buckets and operation costs as HTTP above | Valid discovery/ping/notifications and ordinary reads cost 1; recall 3; expand 2; status/usage use the separate status bucket. Writes, unknown methods/tools and malformed envelopes cost 5 |

IPv4 uses the exact address. IPv6 is grouped by its /64 prefix so privacy-address rotation within one subnet does not reset the anonymous bucket. The server trusts `X-Forwarded-For` only when the immediate peer is on the explicitly configured proxy allowlist; otherwise it uses the socket peer. Proxy configuration and public ingress are deployment inputs, not guessed from request headers.

Use a weighted token bucket with continuous refill and explicit burst capacity. The implementation boundary is a pure policy/key module plus a `RateLimitStore` port; production uses one shared Redis-compatible primary and an atomic script for account+IP admission. Deployment must support atomic multi-key operations for the selected topology; do not assume Redis Cluster can atomically update unrelated keys across slots. Unit and router tests use a deterministic in-memory adapter. Do not use per-process counters in a multi-replica deployment. [`tower-governor`](https://github.com/benwis/tower-governor) is a useful Axum/IP/header integration reference, while its in-memory quota alone is not a production multi-replica store. Redis documents the need for a centralized store and atomic counter/expiry operations ([Redis rate limiter](https://redis.io/docs/latest/develop/use-cases/rate-limiter/)).

## HTTP and web contract

- Over limit returns HTTP 429 and `Retry-After`. Current headers are `RateLimit-Limit`, `RateLimit-Remaining`, and `RateLimit-Reset`; they are implementation fields, not a claim of conformance to a finalized IETF rate-limit header RFC.
- The current error envelope uses `error: "rate_limited"` and integer `retry_after_seconds`. For multiple exhausted buckets, it advertises the longest wait needed for this operation. It never echoes a route key, IP address, token, URL, or quota-store key.
- CORS exposes the current rate-limit headers, `Retry-After`, and the local-cache marker. The web API client parses the server-provided wait time; it does not guess a reset timestamp.
- Show a calm warning when the account approaches its configured allowance and a clear retry time after 429. Do not automatically retry a 429. Offer retry only after the advertised time, with jitter for automated clients.
- Run admission once per inbound request; internal DB/cache retries remain under that admitted operation and do not spend a second request token. A client retry after `Retry-After` is a new request.
- Denied requests do not consume admitted-operation units or increment admitted-call counts. Authenticated denials appear as a separate owner-only daily `denied_requests` aggregate; anonymous and invalid-credential denials are not attributed to an account.

`WAM_MAX_INFLIGHT_REQUESTS` bounds concurrent API and MCP work per server process (default 256). When no slot is available, the service returns `503` with `error: "service_busy"` and `Retry-After: 1`; `/healthz` remains available. This local guard bounds work on each process and does not replace shared account/IP buckets. Tune it to the DB and Redis pool sizes and measure under load before increasing it.

HTTP 429 is defined by [RFC 6585](https://www.rfc-editor.org/rfc/rfc6585.html). `RateLimit` and `RateLimit-Policy` are currently an HTTPAPI draft, so pin its revision in contract tests if adopted ([draft-ietf-httpapi-ratelimit-headers](https://datatracker.ietf.org/doc/html/draft-ietf-httpapi-ratelimit-headers)). OWASP recommends per-client limits as one control against unrestricted resource consumption ([API4:2023](https://api-security.owasp.org/editions/2023/en/0xa4-unrestricted-resource-consumption/)).

## Privacy and failure behavior

IP bucket keys are `HMAC(rotating_secret, normalized_ip_or_prefix || policy_version)` and expire with the longest active window plus a small cleanup margin. Never put the IP or digest into application logs. Account quota keys use an internal stable account ID, not handle, email, OAuth subject, or device token. Rate-limit aggregates are not leaderboard inputs.

If the shared store is unreachable, do not silently fail open. Return a fixed retryable 503 with `Retry-After: 1` and `retry_after_seconds: 1` for writes, login/device challenges, and uncached or expensive recall. A response may be served from a bounded public cache only if its cache policy permits it and it is clearly marked as cached. Do not use local per-process fallback as though it were the shared quota.

An independent per-process load-shed guard protects DB/Redis pools and tail latency during distributed scraping that evades per-IP limits. It is a backpressure response (`503 service_busy` with `Retry-After`), not a per-user 429. It does not reject the lightweight health check or serve uncached public data from an unsafe stale cache. Multi-process and deployment capacity still require load testing.

## Current gap

The working tree applies weighted anonymous-IP, authenticated-account, and shared-IP buckets before HTTP handlers. The Redis adapter uses an atomic multi-key script; production startup requires Redis and a 32-byte key secret. Disposable Redis integration tests cover two independent store connections, concurrent quota boundaries, denied usage counts, multi-bucket atomicity, and account deletion fences. `GET /v1/me/usage`, 30-day admitted activity, authenticated denied-attempt counts, per-process in-flight shedding, `Retry-After`, exposed rate headers, and the dashboard quota panel are implemented. An injected limiter-storage failure returns a fixed 503 with `Retry-After: 1`, while `/healthz` remains available. Live Redis failover, deployed trusted-proxy configuration, and production capacity tuning remain unverified. Invalid credentials use a bounded low-cost anonymous path; valid account requests still consume both account and network buckets. Anonymous and invalid-credential denials are not attributable to an owner dashboard.
