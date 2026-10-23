use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }

        impl std::str::FromStr for $name {
            type Err = uuid::Error;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                s.parse().map(Self)
            }
        }
    };
}

id_type!(ProjectId);
id_type!(MemoryId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Fact,
    Decision,
    Procedure,
    Constraint,
    Gotcha,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Active,
    Disputed,
    Superseded,
    Archived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Manual,
    Agent,
    Extracted,
}

/// A stored memory. `project_id == None` means global scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Memory {
    pub id: MemoryId,
    pub project_id: Option<ProjectId>,
    pub kind: Kind,
    pub title: String,
    pub content: String,
    pub status: Status,
    pub source: Source,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    /// Optimistic-concurrency counter; starts at 1, bumped on every change.
    pub revision: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enums_serialize_snake_case() {
        assert_eq!(serde_json::to_string(&Kind::Gotcha).unwrap(), "\"gotcha\"");
        assert_eq!(
            serde_json::to_string(&Status::Superseded).unwrap(),
            "\"superseded\""
        );
    }

    #[test]
    fn id_roundtrips_through_str() {
        let id = MemoryId::new();
        assert_eq!(id.to_string().parse::<MemoryId>().unwrap(), id);
    }
}
