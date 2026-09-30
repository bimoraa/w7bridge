/*! Codex가 관리하는 로컬 project metadata를 읽어. 실행·파일 registry는 수정하지 않아. */

use crate::config::CodexSettings;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
    time::Duration,
};
use thiserror::Error;

pub(crate) struct Discovery {

    home: Option<PathBuf>,
    enabled: bool,

}

#[derive(Serialize)]
pub(crate) struct Project {

    pub id: String,
    pub name: String,
    pub roots: Vec<PathBuf>,
    pub available: bool,

}

#[derive(Serialize)]
pub(crate) struct Snapshot {

    #[serde(skip_serializing)]
    pub projects: Vec<Project>,
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

}

#[derive(Debug, Error)]
enum DiscoveryError {

    #[error("Codex project 저장소를 읽을 수 없습니다")]
    Read(#[from] io::Error),
    #[error("Codex project database를 읽을 수 없습니다. 잠금 또는 schema를 확인하세요")]
    Database(#[from] rusqlite::Error),
    #[error("Codex project JSON 형식이 올바르지 않습니다")]
    Json(#[from] serde_json::Error),
    #[error("Codex project 목록이 읽기 한도를 초과했습니다")]
    Limit,

}

#[derive(Deserialize)]
struct LegacyState {

    #[serde(default, rename = "local-projects")]
    projects: BTreeMap<String, serde_json::Value>,

}

#[derive(Deserialize)]
struct LegacyProject {

    id: String,
    name: String,
    #[serde(rename = "rootPaths")]
    roots: Vec<PathBuf>,

}

impl Discovery {

    pub(crate) fn new(settings: CodexSettings) -> Self {

        let home = settings.home.or_else(|| {

            std::env::var_os("CODEX_HOME").filter(|value| !value.is_empty()).map(PathBuf::from).or_else(|| {

                let variable = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
                std::env::var_os(variable).filter(|value| !value.is_empty()).map(|home| PathBuf::from(home).join(".codex"))

            })

        });
        Self { home: home.filter(|home| home.is_absolute()), enabled: settings.enabled }

    }

    /** 매 호출에 최신 목록을 읽는다. 없으면 missing, 읽기 실패는 error이며 이전 목록을 재사용하지 않는다. */
    pub(crate) fn read(&self) -> Snapshot {

        let mut snapshot = Snapshot { projects: Vec::new(), status: "missing", source: None, error: None };
        if !self.enabled {

            snapshot.status = "disabled";
            return snapshot;

        }
        let Some(home) = &self.home else { return snapshot };
        match self.read_home(home) {

            Ok(Some((projects, source))) => {

                snapshot.projects = projects;
                snapshot.source = Some(source);
                snapshot.status = "ready";

            }
            Ok(None) => {}
            Err(error) => {

                snapshot.status = "error";
                snapshot.error = Some(error.to_string());

            }

        }
        snapshot

    }

    fn read_home(&self, home: &Path) -> Result<Option<(Vec<Project>, &'static str)>, DiscoveryError> {

        let entries = match fs::read_dir(home) {

            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),

        };
        let mut database = None;
        for entry in entries {

            let entry = entry?;
            let name = entry.file_name();
            let Some(version) = name.to_str().and_then(|name| name.strip_prefix("state_"))
                .and_then(|name| name.strip_suffix(".sqlite")).and_then(|version| version.parse::<u32>().ok())
            else { continue };
            if database.as_ref().is_none_or(|(current, _)| version > *current) {

                database = Some((version, entry.path()));

            }

        }
        if let Some((_, path)) = database {

            let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
            connection.busy_timeout(Duration::from_millis(250))?;
            let has_projects: bool = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'projects')", [],
                |row| row.get(0),
            )?;
            if has_projects {

                return Ok(Some((read_database(&connection)?, "codex_database")));

            }

        }

        let path = home.join(".codex-global-state.json");
        let file = match File::open(path) {

            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),

        };
        let mut bytes = Vec::new();
        file.take(16 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > 16 * 1024 * 1024 {

            return Err(DiscoveryError::Limit);

        }
        let state: LegacyState = serde_json::from_slice(&bytes)?;
        if state.projects.len() > 1024 {

            return Err(DiscoveryError::Limit);

        }
        let mut projects = Vec::new();
        for (key, value) in state.projects {

            let Ok(record) = serde_json::from_value::<LegacyProject>(value) else { continue };
            if key == record.id
                && let Some(project) = project(record.id, record.name, record.roots)
            {

                projects.push(project);

            }

        }
        Ok(Some((projects, "codex_legacy_json")))

    }

}

fn read_database(connection: &Connection) -> Result<Vec<Project>, DiscoveryError> {

    let mut statement = connection.prepare(
        "SELECT p.id, p.name, r.path FROM projects p JOIN project_roots r ON p.id = r.project_id
         ORDER BY p.position, p.id, r.position LIMIT 4097",
    )?;
    let mut rows = statement.query([])?;
    let mut records = BTreeMap::<String, (String, Vec<PathBuf>)>::new();
    let mut count = 0;
    while let Some(row) = rows.next()? {

        count += 1;
        if count > 4096 {

            return Err(DiscoveryError::Limit);

        }
        let id: String = row.get(0)?;
        let name: String = row.get(1)?;
        let root: String = row.get(2)?;
        records.entry(id).or_insert_with(|| (name, Vec::new())).1.push(root.into());

    }
    if records.len() > 1024 {

        return Err(DiscoveryError::Limit);

    }
    Ok(records.into_iter().filter_map(|(id, (name, roots))| project(id, name, roots)).collect())

}

fn project(id: String, name: String, roots: Vec<PathBuf>) -> Option<Project> {

    if id.is_empty() || id.len() > 64 || !id.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_-".contains(&byte))
        || name.is_empty() || name.len() > 512 || name.chars().any(char::is_control) || roots.len() > 64
    {

        return None;

    }
    let mut valid_roots = Vec::new();
    for root in roots {

        let Some(value) = root.to_str() else { continue };
        if root.is_absolute() && value.len() <= 4096 && !value.chars().any(char::is_control) && !valid_roots.contains(&root) {

            valid_roots.push(root);

        }

    }
    if valid_roots.is_empty() {

        return None;

    }
    let available = valid_roots.iter().all(|root| root.is_dir());
    Some(Project { id, name, roots: valid_roots, available })

}

#[cfg(test)]
#[path = "../../tests/unit/codex_projects.rs"]
mod tests;
