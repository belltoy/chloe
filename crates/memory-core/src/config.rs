use directories::{BaseDirs, ProjectDirs};
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::{Error, Result};

pub const DB_FILE: &str = "memory.db";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Config {
    pub db_path: PathBuf,
    pub log_level: String,
}

impl Config {
    /// Resolve from explicit overrides (flag/env already merged by the caller);
    /// the default data dir is only consulted when `db_path` is `None`, so tests
    /// that pass a `db_path` can never touch the user's database.
    pub fn resolve(db_path: Option<PathBuf>, log_level: Option<String>) -> Result<Self> {
        let db_path = match db_path {
            Some(p) => p,
            None => default_data_dir()?.join(DB_FILE),
        };
        Ok(Self {
            db_path,
            log_level: log_level.unwrap_or_else(|| "info".into()),
        })
    }
}

fn default_data_dir() -> Result<PathBuf> {
    let err = || Error::Internal("cannot determine data directory".into());
    if cfg!(target_os = "macos") {
        // Developer-CLI convention: follow XDG on macOS too (no space in the path).
        let home = BaseDirs::new().ok_or_else(err)?.home_dir().to_path_buf();
        return Ok(xdg_data_dir(
            std::env::var_os("XDG_DATA_HOME").map(PathBuf::from),
            &home,
        ));
    }
    // Linux already honours XDG_DATA_HOME; Windows uses its own convention.
    Ok(ProjectDirs::from("", "", "agent-memory")
        .ok_or_else(err)?
        .data_dir()
        .to_path_buf())
}

/// Per the XDG spec a relative `XDG_DATA_HOME` is invalid and ignored.
fn xdg_data_dir(xdg_data_home: Option<PathBuf>, home: &Path) -> PathBuf {
    xdg_data_home
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".local/share"))
        .join("agent-memory")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_db_path_wins() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.db");
        let c = Config::resolve(Some(p.clone()), None).unwrap();
        assert_eq!(c.db_path, p);
        assert_eq!(c.log_level, "info");
    }

    #[test]
    fn xdg_data_home_used_when_absolute_else_fallback() {
        let home = Path::new("/home/u");
        assert_eq!(
            xdg_data_dir(Some("/x".into()), home),
            Path::new("/x/agent-memory")
        );
        let fallback = Path::new("/home/u/.local/share/agent-memory");
        assert_eq!(xdg_data_dir(Some("rel".into()), home), fallback);
        assert_eq!(xdg_data_dir(None, home), fallback);
    }

    #[test]
    fn default_path_is_in_data_dir() {
        let c = Config::resolve(None, Some("debug".into())).unwrap();
        assert!(c.db_path.ends_with(DB_FILE));
        assert_eq!(c.log_level, "debug");
    }
}
