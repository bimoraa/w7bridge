/*! 등록된 탐색 root만 bounded scan하고 권한 없는 후보를 반환해. */

use crate::{config::DiscoverySettings, filesystem::digest};
use serde::Serialize;
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, io, path::Path};

pub(crate) struct Discovery {

    settings: DiscoverySettings,

}

#[derive(Serialize)]
pub(crate) struct Snapshot {

    #[serde(skip_serializing)]
    pub projects: Vec<Value>,
    pub status: &'static str,
    pub visited: usize,
    pub errors: Vec<String>,

}

impl Discovery {

    pub fn new( settings: DiscoverySettings, ) -> Self {

        Self { settings }

    }

    pub fn read( &self, ) -> Snapshot {

        let mut snapshot = Snapshot { projects: Vec::new(), status: "ready", visited: 0, errors: Vec::new() };
        let mut seen = BTreeSet::new();
        for root in &self.settings.roots {

            let result = root.canonicalize().and_then(|root| self.scan(&root, 0, &mut seen, &mut snapshot));
            if result.is_err() {

                snapshot.status = "error";
                snapshot.errors.push("탐색 root를 읽을 수 없거나 scan 한도를 초과했습니다".into());

            }

        }
        snapshot.projects.sort_by(|left, right| left["id"].as_str().cmp(&right["id"].as_str()));
        snapshot

    }

    fn scan( &self, root: &Path, depth: usize, seen: &mut BTreeSet<std::path::PathBuf>, snapshot: &mut Snapshot, ) -> io::Result<()> {

        if !seen.insert(root.to_owned()) {

            return Ok(());

        }
        snapshot.visited += 1;
        if snapshot.visited > 10000 || snapshot.projects.len() >= 1024 {

            return Err(io::Error::other("project scan 한도를 초과했습니다"));

        }
        if let Some(kind) = super::presets::detect(root) {

            let id = format!("folder-{}", &digest(root.as_os_str().as_encoded_bytes())[..24]);
            snapshot.projects.push(json!({"id":id,"name":root.file_name().unwrap_or_default().to_string_lossy(),
                "root":root,"roots":[root],"kind":kind,"available":true,"commands":[],"suggested_commands":["check","build","test","run"],"source":"folder"}));

        }
        if depth >= self.settings.max_depth {

            return Ok(());

        }
        for entry in fs::read_dir(root)? {

            let entry = entry?;
            let metadata = fs::symlink_metadata(entry.path())?;
            if metadata.is_dir() && !crate::filesystem::paths::redirected(&metadata) {

                let name = entry.file_name();
                if !crate::filesystem::paths::excluded_builtin(&name.to_string_lossy()) {

                    self.scan(&entry.path(), depth + 1, seen, snapshot)?;

                }

            }

        }
        Ok(())

    }

}

#[cfg(test)]
#[path = "../../tests/unit/discovery.rs"]
mod tests;
