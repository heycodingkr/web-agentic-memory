#![forbid(unsafe_code)]

pub mod application;
pub mod contribution_store;
pub mod diagnostics;
pub mod durable;
pub mod http;
pub mod identity;
pub mod mcp_transport;
pub mod memory_repository;
mod owner_usage;
pub mod ports;
mod profile_wire;
pub mod rate_limit;
pub mod score_store;
pub mod usage_store;

pub use application::WamService;
pub use contribution_store::{FileContributionStore, InMemoryContributionStore};
pub use durable::ApplicationRepository;
pub use http::{
    DemoReviewerTokenAuthenticator, DemoTokenAuthenticator, IdentityAuthenticator,
    ReviewerAuthenticator, router, router_with_identity, router_with_identity_and_rate_limit_store,
    router_with_reviewer,
};
pub use identity::{FileIdentityStore, IdentityService};
pub use mcp_transport::McpOriginAllowlist;
pub use memory_repository::{InMemoryRepository, LocalRepository};
pub use rate_limit::{InMemoryRateLimitStore, IpNetwork, RateLimitStore, RedisRateLimitStore};
pub use score_store::{FileScoreStore, InMemoryScoreStore};
pub use usage_store::{FileUsageStore, InMemoryUsageStore};
