# wam-core

A small Rust library for deterministic domain rules shared by the future HTTP, MCP, CLI, and web adapters.

## Modules

| Module | Responsibility | Test seam |
|---|---|---|
| url_scope | HTTPS/loopback URL projection, sensitive path rejection, explicit exact/subtree/host scope matching | Inputs and policy modes only; no DNS lookup or network access |
| keyword | Versioned closed task and keyword vocabularies; NFKC keyword normalization and allowlist filtering | Supply a small in-memory vocabulary |
| memory | Validated immutable projection of a versioned memory index | Construct from typed values; no database required |
| recall | Scope filtering and deterministic route/task/keyword/freshness ranking | Pass a slice of memory indexes and a query |
| profile | Canonical public handles, bounded typed rich-text bio, HTTPS profile/service links | Build profile values through validating constructors; no renderer or database |
| leaderboard | Private overall rank and public opt-in leaderboard projection with deterministic tie ranking | Pass a synthetic account-score slice; no score storage or clock |

The crate has no database, HTTP, filesystem, clock, model, or host dependencies. HTTP and MCP adapters must resolve untrusted strings through the versioned vocabularies and construct typed queries before invoking recall. Adapters map returned hits to transport DTOs; they do not implement a second ranking policy.

Opaque account path segments require an explicit trusted host route rule through parse_with_dynamic_segments. Query, fragment, userinfo, recognized account identifiers, and numeric/UUID path identifiers do not survive canonicalization. Sensitive-looking or ambiguous paths fail closed.

## Run

Run cargo test -p wam-core from the repository root. Integration tests live in tests/ and exercise only the public crate API.