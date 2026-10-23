//! Scope resolver: maps a directory to a stable `ProjectId`.
//!
//! Policy: identity is a UUID stored in the DB, keyed by the canonical *main* checkout path.
//! Git worktrees resolve to their main repository and are recorded as `worktree` aliases;
//! a plain directory is its own project. A moved/renamed path is a NEW project until the user
//! links it with `add_alias`. Git remotes are never used as identity (column `remote_url` is
//! reserved for informational aliases and is not auto-populated in A).

use std::fs;
use std::path::{Path, PathBuf};

use memory_core::{Error, ProjectId, Result};
use rusqlite::{Connection, OptionalExtension, params};

use crate::{SqliteStore, begin, micros, now, storage};

/// Returns `(checkout_dir, project_root)`; they differ only for linked worktrees.
fn find_root(dir: &Path) -> Result<(PathBuf, PathBuf)> {
    let dir = dir
        .canonicalize()
        .map_err(|e| Error::InvalidArgument(format!("{}: {e}", dir.display())))?;
    for d in dir.ancestors() {
        let dot_git = d.join(".git");
        if dot_git.is_dir() {
            return Ok((d.into(), d.into()));
        }
        if dot_git.is_file() {
            return Ok((
                d.into(),
                worktree_main(d, &dot_git).unwrap_or_else(|| d.into()),
            ));
        }
    }
    Ok((dir.clone(), dir))
}

/// For a linked worktree `.git` file (`gitdir: <main>/.git/worktrees/<name>`), the main checkout.
/// Submodules and bare setups return `None` and are treated as their own root.
fn worktree_main(checkout: &Path, dot_git: &Path) -> Option<PathBuf> {
    let text = fs::read_to_string(dot_git).ok()?;
    let gitdir = checkout.join(text.strip_prefix("gitdir:")?.trim());
    let gitdir = gitdir.canonicalize().unwrap_or(gitdir);
    let worktrees = gitdir
        .parent()
        .filter(|p| p.file_name().is_some_and(|n| n == "worktrees"))?;
    let common = worktrees
        .parent()
        .filter(|p| p.file_name().is_some_and(|n| n == ".git"))?;
    common.parent().map(Path::to_path_buf)
}

fn lookup(conn: &Connection, path: &Path) -> Result<Option<String>> {
    conn.query_row(
        "SELECT id FROM projects WHERE canonical_path=?1 UNION SELECT project_id FROM project_aliases WHERE path=?1",
        [path.to_string_lossy()],
        |r| r.get(0),
    )
    .optional()
    .map_err(storage)
}

/// The already registered project for `dir`, if any; unlike `resolve_project` it never creates
/// one, and an unreadable directory is just "none".
pub(crate) fn find_project(conn: &Connection, dir: &Path) -> Option<String> {
    let (checkout, root) = find_root(dir).ok()?;
    lookup(conn, &root).ok()?.or(lookup(conn, &checkout).ok()?)
}

impl SqliteStore {
    /// Finds or creates the project for `dir`.
    pub fn resolve_project(&self, dir: &Path) -> Result<ProjectId> {
        let (checkout, root) = find_root(dir)?;
        let mut conn = self.conn();
        let tx = begin(&mut conn)?;
        let id = match lookup(&tx, &root)?.or(lookup(&tx, &checkout)?) {
            Some(id) => id,
            None => {
                let id = ProjectId::new().to_string();
                tx.execute(
                    "INSERT INTO projects (id, canonical_path, created_at) VALUES (?1,?2,?3)",
                    params![id, root.to_string_lossy(), micros(now())],
                )
                .map_err(storage)?;
                id
            }
        };
        if checkout != root {
            tx.execute(
                "INSERT OR IGNORE INTO project_aliases (project_id, path, kind) VALUES (?1,?2,'worktree')",
                params![id, checkout.to_string_lossy()],
            )
            .map_err(storage)?;
        }
        tx.commit().map_err(storage)?;
        id.parse()
            .map_err(|e| Error::Internal(format!("corrupt project id: {e}")))
    }

    /// Links an additional path (e.g. a moved checkout) to an existing project.
    /// Fails if the path already belongs to a project, so one project never steals another's path.
    pub fn add_alias(&self, project: ProjectId, path: &Path, kind: &str) -> Result<()> {
        let path = path
            .canonicalize()
            .map_err(|e| Error::InvalidArgument(format!("{}: {e}", path.display())))?;
        let mut conn = self.conn();
        let tx = begin(&mut conn)?;
        let taken: Option<String> = tx
            .query_row(
                "SELECT id FROM projects WHERE canonical_path=?1 UNION SELECT project_id FROM project_aliases WHERE path=?1",
                [path.to_string_lossy()],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage)?;
        if taken.is_some() {
            return Err(Error::InvalidArgument(format!(
                "{} already belongs to a project",
                path.display()
            )));
        }
        tx.execute(
            "INSERT INTO project_aliases (project_id, path, kind) VALUES (?1,?2,?3)",
            params![project.to_string(), path.to_string_lossy(), kind],
        )
        .map_err(|e| match e {
            rusqlite::Error::SqliteFailure(f, _)
                if f.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                Error::NotFound(format!("project {project}"))
            }
            e => storage(e),
        })?;
        tx.commit().map_err(storage)
    }
}
