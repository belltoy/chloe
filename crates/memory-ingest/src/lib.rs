//! Agent adapters: turn an agent's native hook payload into a redacted `AgentEvent`.
//! No SQL and no network; persistence is the caller's concern.

pub mod hooks;
pub mod redact;
pub mod spool;
