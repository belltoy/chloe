use crate::{Kind, Memory, MemoryId, ProjectId, Result};

#[derive(Debug, Clone)]
pub struct NewMemory {
    pub project_id: Option<ProjectId>,
    pub kind: Kind,
    pub title: String,
    pub content: String,
}

/// Partial update; `None` leaves the field unchanged.
#[derive(Debug, Clone, Default)]
pub struct MemoryChanges {
    pub kind: Option<Kind>,
    pub title: Option<String>,
    pub content: Option<String>,
}

/// Single business-logic entry point shared by CLI and MCP (implemented in A1).
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
}
