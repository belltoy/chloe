//! Domain types, errors, config and service traits. No SQL, no agent-specific code.

pub mod config;
pub mod error;
pub mod event;
pub mod model;
pub mod service;

pub use config::Config;
pub use error::{Error, Result};
pub use event::{
    AgentEvent, EVENT_SCHEMA_VERSION, EventKind, EventStore, IngestReport, Rejection, SessionInfo,
    SessionState,
};
pub use model::{Kind, Memory, MemoryId, ProjectId, Source, Status};
