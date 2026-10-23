use serde::{Deserialize, Serialize};

use crate::{Kind, Memory, MemoryId, ProjectId, Result, Source};

#[derive(Debug, Clone)]
pub struct NewMemory {
    pub project_id: Option<ProjectId>,
    pub kind: Kind,
    pub title: String,
    pub content: String,
    pub source: Source,
}

#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub query: String,
    pub limit: usize,
    /// Also return global (project-less) memories.
    pub include_global: bool,
    /// Default is `Active` only.
    pub include_inactive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hit {
    pub memory: Memory,
    /// Higher is better.
    pub score: f64,
}

/// Snapshot of a memory as of `revision`; history is append-only.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Revision {
    pub revision: u32,
    pub cause: String,
    pub snapshot: Memory,
}

/// Partial update; `None` leaves the field unchanged.
#[derive(Debug, Clone, Default)]
pub struct MemoryChanges {
    pub kind: Option<Kind>,
    pub title: Option<String>,
    pub content: Option<String>,
}

/// Single business-logic entry point shared by CLI and MCP (implemented in `memory-store`).
/// `project` is the caller's scope; reads of other projects' memories are `NotFound`.
pub trait MemoryService {
    fn save(&self, new: NewMemory) -> Result<Memory>;
    fn get(&self, project: Option<ProjectId>, id: MemoryId) -> Result<Memory>;
    fn update(
        &self,
        project: Option<ProjectId>,
        id: MemoryId,
        expected_revision: u32,
        changes: MemoryChanges,
    ) -> Result<Memory>;
    fn archive(
        &self,
        project: Option<ProjectId>,
        id: MemoryId,
        expected_revision: u32,
    ) -> Result<Memory>;
    fn list(
        &self,
        project: Option<ProjectId>,
        include_inactive: bool,
        limit: usize,
    ) -> Result<Vec<Memory>>;
    fn search(&self, project: Option<ProjectId>, query: SearchQuery) -> Result<Vec<Hit>>;
    fn history(&self, project: Option<ProjectId>, id: MemoryId) -> Result<Vec<Revision>>;
}
