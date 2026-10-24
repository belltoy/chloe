//! The capture queue: a directory with one file per event, shared by many short-lived hook
//! processes and drained by `chloe events import`.
//!
//! Layout: `<dir>/pending/<time>-<pid>.json` (one event), `<dir>/rejected/` (quarantine).
//! A hook writes `.<name>.tmp` and renames it into place, so the importer only ever sees whole
//! files and needs no lock. The importer deletes a file only after the database transaction that
//! stored its event has committed, so a crash at any point means "imported again", which the
//! idempotency key turns into a no-op.
//!
//! Reliability boundary: an event is safe once `enqueue` returns, against a crash of the hook,
//! the agent or `chloe`. Data is not fsynced (an fsync per hook costs more than the hook), so a
//! power loss or OS crash can lose the last few seconds of events. An event whose hook is
//! killed before the rename, or whose write fails (full disk, unwritable directory), is lost
//! and only reported on stderr. There is no cap on queue size if nothing ever imports.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use memory_core::AgentEvent;

const PENDING: &str = "pending";
const REJECTED: &str = "rejected";
/// Whole events are far below this; anything bigger is damage, not an event.
const MAX_ENTRY_BYTES: u64 = 64 * 1024;
/// A `.tmp` file this old belongs to a hook that died between create and rename.
const STALE_TMP: Duration = Duration::from_secs(600);

/// Keeps two events queued by one process apart even if they share a capture time.
static SEQ: AtomicU64 = AtomicU64::new(0);

/// The queue belongs to one database, next to it. The default database keeps the original
/// `<dir>/spool`; any other file `x.db` gets `<dir>/x.db.spool`, so two databases in one
/// directory never share (and never drain) each other's events.
pub fn dir_for(db_path: &Path) -> PathBuf {
    let dir = db_path.parent().unwrap_or(Path::new("."));
    match db_path.file_name().and_then(|n| n.to_str()) {
        Some(memory_core::config::DB_FILE) | None => dir.join("spool"),
        Some(name) => dir.join(format!("{name}.spool")),
    }
}

fn private_dir(path: &Path) -> std::io::Result<()> {
    let mut b = std::fs::DirBuilder::new();
    b.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut b, 0o700);
    b.create(path)
}

/// Queue one event. The file name sorts by capture time; the pid keeps concurrent hooks apart.
pub fn enqueue(dir: &Path, event: &AgentEvent) -> std::io::Result<()> {
    let pending = dir.join(PENDING);
    private_dir(&pending)?;
    let name = format!(
        "{:020}-{}-{}.json",
        event.occurred_at.unix_timestamp_nanos().max(0),
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    );
    let tmp = pending.join(format!(".{name}.tmp"));
    let mut o = std::fs::OpenOptions::new();
    o.create_new(true).write(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    let mut f = o.open(&tmp)?;
    let written = serde_json::to_writer(&mut f, event)
        .map_err(std::io::Error::other)
        .and_then(|()| f.flush());
    if let Err(e) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    std::fs::rename(&tmp, pending.join(name)).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// A queued file and what it held; `event` is `None` when the file is not a readable event.
#[derive(Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub event: Option<AgentEvent>,
}

/// Up to `limit` queued entries, oldest first. Also removes stale `.tmp` leftovers of hooks that
/// died mid-write. A missing queue is an empty queue.
pub fn pending(dir: &Path, limit: usize) -> std::io::Result<Vec<Entry>> {
    let pending = dir.join(PENDING);
    let rd = match std::fs::read_dir(&pending) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e),
    };
    let mut names = Vec::new();
    for item in rd {
        let item = item?;
        let name = item.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            let stale = item
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| SystemTime::now().duration_since(t).ok())
                .is_some_and(|age| age > STALE_TMP);
            if stale {
                let _ = std::fs::remove_file(item.path());
            }
        } else if name.ends_with(".json") {
            names.push(name);
        }
    }
    names.sort();
    Ok(names
        .into_iter()
        .take(limit)
        .map(|n| {
            let path = pending.join(n);
            let event = read_entry(&path);
            Entry { path, event }
        })
        .collect())
}

fn read_entry(path: &Path) -> Option<AgentEvent> {
    if std::fs::metadata(path).ok()?.len() > MAX_ENTRY_BYTES {
        return None;
    }
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// The entry's event is stored (or already was): drop the file. Already gone is fine, since a
/// second importer may have got there first.
pub fn done(entry: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(entry) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Move an entry that can never be imported (unreadable, or refused by the store) out of
/// `pending` so it is not retried forever, but keep it for inspection.
pub fn quarantine(dir: &Path, entry: &Path) -> std::io::Result<()> {
    let rejected = dir.join(REJECTED);
    private_dir(&rejected)?;
    let name = entry.file_name().unwrap_or_default();
    match std::fs::rename(entry, rejected.join(name)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// `(pending, rejected)` file counts, for `doctor`.
pub fn counts(dir: &Path) -> std::io::Result<(usize, usize)> {
    let count = |sub: &str, ext: &str| match std::fs::read_dir(dir.join(sub)) {
        Ok(rd) => Ok(rd
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().ends_with(ext))
            .count()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(e) => Err(e),
    };
    Ok((count(PENDING, ".json")?, count(REJECTED, ".json")?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use memory_core::{EVENT_SCHEMA_VERSION, EventKind};

    fn ev(session: &str, secs: i64) -> AgentEvent {
        AgentEvent {
            schema_version: EVENT_SCHEMA_VERSION,
            native_agent: "claude_code".into(),
            native_session_id: session.into(),
            native_event_id: None,
            kind: EventKind::SessionEnd,
            occurred_at: time::OffsetDateTime::from_unix_timestamp(1_800_000_000 + secs).unwrap(),
            cwd: None,
            tool_name: None,
            text: None,
            redactions: vec![],
        }
    }

    #[test]
    fn each_database_has_its_own_queue() {
        let default = dir_for(Path::new("/d/memory.db"));
        assert_eq!(default, Path::new("/d/spool"));
        let (a, b) = (dir_for(Path::new("/d/a.db")), dir_for(Path::new("/d/b.db")));
        assert_ne!(a, b);
        assert_ne!(a, default);
        assert_eq!(a, Path::new("/d/a.db.spool"));
    }

    #[test]
    fn enqueue_makes_private_whole_files_oldest_first() {
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().join("spool");
        enqueue(&dir, &ev("late", 9)).unwrap();
        enqueue(&dir, &ev("early", 1)).unwrap();
        let got = pending(&dir, 10).unwrap();
        let sessions: Vec<_> = got
            .iter()
            .map(|e| e.event.as_ref().unwrap().native_session_id.as_str())
            .collect();
        assert_eq!(sessions, ["early", "late"]);
        assert_eq!(counts(&dir).unwrap(), (2, 0));
        assert_eq!(pending(&dir, 1).unwrap().len(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&got[0].path), 0o600);
            assert_eq!(mode(&dir.join("pending")), 0o700);
        }
    }

    #[test]
    fn many_writers_lose_nothing() {
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().to_path_buf();
        let handles: Vec<_> = (0..32)
            .map(|i| {
                let dir = dir.clone();
                std::thread::spawn(move || enqueue(&dir, &ev(&format!("s{i}"), 0)).unwrap())
            })
            .collect();
        handles.into_iter().for_each(|h| h.join().unwrap());
        assert_eq!(pending(&dir, 100).unwrap().len(), 32);
    }

    #[test]
    fn damaged_leftover_and_missing_entries_are_handled() {
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().join("spool");
        assert!(pending(&dir, 10).unwrap().is_empty(), "no queue yet");
        enqueue(&dir, &ev("ok", 0)).unwrap();
        let p = dir.join("pending");
        std::fs::write(p.join("00000000000000000001-1-0.json"), "{\"schema_ver").unwrap();
        std::fs::write(p.join("00000000000000000002-1-0.json"), vec![b'x'; 70_000]).unwrap();
        std::fs::write(p.join(".half.tmp"), "partial").unwrap(); // a hook mid-write: left alone
        let got = pending(&dir, 10).unwrap();
        assert_eq!(got.len(), 3, "a .tmp file is never an entry");
        assert_eq!(got.iter().filter(|e| e.event.is_none()).count(), 2);
        for e in got.iter().filter(|e| e.event.is_none()) {
            quarantine(&dir, &e.path).unwrap();
            quarantine(&dir, &e.path).unwrap(); // already moved: not an error
        }
        let ok = got.iter().find(|e| e.event.is_some()).unwrap();
        done(&ok.path).unwrap();
        done(&ok.path).unwrap();
        assert_eq!(counts(&dir).unwrap(), (0, 2));
        assert!(
            p.join(".half.tmp").exists(),
            "fresh tmp files are not swept"
        );
    }
}
