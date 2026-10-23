//! Interim event sink: one JSON line per event appended to a private file.
//!
//! shortcut: an append-only file with no events table, no dedup and no draining. B0-04 adds
//! the table and idempotent import, B0-05 the real queue and crash recovery.

use std::io::Write;
use std::path::{Path, PathBuf};

use memory_core::AgentEvent;

pub const SPOOL_FILE: &str = "events.jsonl";

/// The spool lives next to the database: `<db dir>/spool/events.jsonl`.
pub fn path_for(db_path: &Path) -> PathBuf {
    db_path
        .parent()
        .unwrap_or(Path::new("."))
        .join("spool")
        .join(SPOOL_FILE)
}

/// Append one event as a single `write` on an `O_APPEND` file, so concurrent hook processes
/// do not interleave lines. Directory is `0700`, file `0600` on Unix.
pub fn append(path: &Path, event: &AgentEvent) -> std::io::Result<()> {
    let mut line = serde_json::to_vec(event).map_err(std::io::Error::other)?;
    line.push(b'\n');
    if let Some(dir) = path.parent() {
        let mut b = std::fs::DirBuilder::new();
        b.recursive(true);
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut b, 0o700);
        b.create(dir)?;
    }
    let mut o = std::fs::OpenOptions::new();
    o.create(true).append(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    o.open(path)?.write_all(&line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use memory_core::{EVENT_SCHEMA_VERSION, EventKind};

    #[test]
    fn appends_private_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = path_for(&dir.path().join("memory.db"));
        let ev = AgentEvent {
            schema_version: EVENT_SCHEMA_VERSION,
            native_agent: "claude_code".into(),
            native_session_id: "s".into(),
            native_event_id: None,
            kind: EventKind::SessionEnd,
            occurred_at: time::OffsetDateTime::UNIX_EPOCH,
            cwd: None,
            tool_name: None,
            text: None,
            redactions: vec![],
        };
        append(&path, &ev).unwrap();
        append(&path, &ev).unwrap();
        let body = std::fs::read_to_string(&path).unwrap();
        assert_eq!(body.lines().count(), 2);
        assert_eq!(
            serde_json::from_str::<AgentEvent>(body.lines().next().unwrap()).unwrap(),
            ev
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&path), 0o600);
            assert_eq!(mode(path.parent().unwrap()), 0o700);
        }
    }
}
