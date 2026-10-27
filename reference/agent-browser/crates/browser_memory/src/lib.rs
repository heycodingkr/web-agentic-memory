// SPDX-License-Identifier: Apache-2.0
//! agent-browser memory — layered (L0–L3) SQLite FTS5/BM25 know-how store with token-budgeted
//! recall, exposed to every agent surface as four `browser_memory_*` tools.
//! Design and tests: docs/specs/browser-memory.md.
//!
//! Patterns taken from the MeshCode reference list (memory / context optimization):
//! one SQLite file with FTS5 + BM25 and measured token cost per query (fts5-agent-memory); a small L0
//! index plus on-demand deeper layers and a consult → act → learn loop (layered-memory-mcp); scoping
//! recall by site like wings/rooms (MemPalace).
#![forbid(unsafe_code)]

pub mod store;
pub mod text;
pub mod tools;
pub mod web;
pub mod workflow;

pub use store::{MemoryError, Store};
