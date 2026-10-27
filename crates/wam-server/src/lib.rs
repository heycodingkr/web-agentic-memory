#![forbid(unsafe_code)]

pub mod application;
pub mod durable;
pub mod contribution_store;
pub mod http;
pub mod identity;
pub mod mcp_transport;
pub mod memory_repository;
pub mod ports;
mod profile_wire;
pub mod score_store;
pub mod usage_store;

pub use application::WamService;
pub use contribution_store::{FileContributionStore, InMemoryContributionStore};
pub use http::{
    DemoReviewerTokenAuthenticator, DemoTokenAuthenticator, IdentityAuthenticator,
    ReviewerAuthenticator, router, router_with_identity, router_with_reviewer,
};
pub use identity::{FileIdentityStore, IdentityService};
pub use mcp_transport::McpOriginAllowlist;
pub use memory_repository::{InMemoryRepository, LocalRepository};
pub use score_store::{FileScoreStore, InMemoryScoreStore};
pub use usage_store::{FileUsageStore, InMemoryUsageStore};
