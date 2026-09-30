/*! 파일 교체 전에 이전 content를 보존하고 조건부 recovery를 제공해. */

use super::{FileStore, digest};
use crate::FileError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Revision {

    id: String,
    path: String,
    previous: Option<String>,
    result: Option<String>,
    observed_at_ms: u128,
    phase: String,

}

impl FileStore {

    fn revisions( &self, ) -> Result<Vec<Revision>, FileError> {

        let mut records = Vec::new();
        for entry in fs::read_dir(self.metadata_dir()?)? {

            let entry = entry?;
            let name = entry.file_name();
            if let Some(id) = name
                .to_str()
                .and_then(|name| name.strip_prefix("history-record-"))
                .and_then(|name| name.strip_suffix(".json"))
            {

                let revision: Revision = serde_json::from_slice(
                    &self.load_metadata(&format!("history-record-{id}.json"))?.ok_or(FileError::Data)?,
                )
                .map_err(|_| FileError::Data)?;
                if revision.id != id
                    || !self.permits(&revision.path)
                    || !valid_hash(id)
                    || [&revision.previous, &revision.result].into_iter().flatten().any(|hash| !valid_hash(hash))
                    || !matches!(revision.phase.as_str(), "prepared" | "committed")
                {

                    return Err(FileError::Data);

                }
                records.push(revision);
                if records.len() > 65 {

                    return Err(FileError::Limit);

                }

            }

        }
        records.sort_by_key(|record| (record.observed_at_ms, record.id.clone()));
        Ok(records)

    }

    fn prune_history( &self, incoming: &[&[u8]], limit: usize, ) -> Result<(), FileError> {

        let mut records = self.revisions()?;
        let incoming: std::collections::BTreeMap<_, _> =
            incoming.iter().map(|bytes| (digest(bytes), bytes.len())).collect();
        loop {

            let hashes: BTreeSet<String> = records
                .iter()
                .flat_map(|record| [&record.previous, &record.result])
                .flatten()
                .cloned()
                .chain(incoming.keys().cloned())
                .collect();
            let mut bytes = 0usize;
            for hash in &hashes {

                bytes = bytes
                    .checked_add(match incoming.get(hash) {

                        Some(length) => *length,
                        None => self.load_metadata(&format!("history-blob-{hash}.bin"))?.ok_or(FileError::Data)?.len(),

                    })
                    .ok_or(FileError::Limit)?;

            }
            if records.len() < 64 && bytes <= limit {

                for entry in fs::read_dir(self.metadata_dir()?)? {

                    let entry = entry?;
                    let name = entry.file_name();
                    if let Some(hash) = name
                        .to_str()
                        .and_then(|name| name.strip_prefix("history-blob-"))
                        .and_then(|name| name.strip_suffix(".bin"))
                    {

                        if !valid_hash(hash) {

                            return Err(FileError::Data);

                        }
                        if !hashes.contains(hash) {

                            fs::remove_file(entry.path())?;

                        }

                    }

                }
                return Ok(());

            }
            let index = records.iter().position(|record| record.phase == "committed").ok_or(FileError::Limit)?;
            let record = records.remove(index);
            fs::remove_file(self.metadata_dir()?.join(format!("history-record-{}.json", record.id)))?;

        }

    }

    pub(super) fn prepare_revision( &self, path: &str, previous: Option<&[u8]>, content: Option<&[u8]>, ) -> Result<Revision, FileError> {

        self.prune_history(&[previous, content].into_iter().flatten().collect::<Vec<_>>(), 256 * 1024 * 1024)?;
        for bytes in [previous, content].into_iter().flatten() {

            let hash = digest(bytes);
            let name = format!("history-blob-{hash}.bin");
            match self.load_metadata(&name)? {

                Some(saved) if digest(&saved) != hash => return Err(FileError::Data),
                Some(_) => {}
                None => self.save_metadata(&name, bytes)?,

            }

        }
        let temporary = tempfile::Builder::new().rand_bytes(32).tempfile_in(self.metadata_dir()?)?;
        let revision = Revision {

            id: digest(temporary.path().file_name().ok_or(FileError::Path)?.as_encoded_bytes()),
            path: path.into(),
            previous: previous.map(digest),
            result: content.map(digest),
            observed_at_ms: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
            phase: "prepared".into(),

        };
        self.save_revision(&revision)?;
        Ok(revision)

    }

    fn save_revision( &self, record: &Revision, ) -> Result<(), FileError> {

        self.save_metadata(
            &format!("history-record-{}.json", record.id),
            &serde_json::to_vec(record).map_err(|_| FileError::Data)?,
        )

    }

    pub(super) fn commit_revision( &self, mut record: Revision, ) -> Result<(), FileError> {

        record.phase = "committed".into();
        self.save_revision(&record)

    }

    pub(crate) fn history( &self, ) -> Result<Value, FileError> {

        let _lock = self.lock("access.lock")?;
        let mut records = Vec::new();
        for mut record in self.revisions()? {

            let current = match self.read_unlocked(&record.path) {

                Ok(bytes) => Some(digest(&bytes)),
                Err(FileError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error),

            };
            if record.phase == "prepared" && current == record.result {

                record.phase = "committed".into();
                self.save_revision(&record)?;

            }
            let mut value = serde_json::to_value(&record).map_err(|_| FileError::Data)?;
            value["current_hash"] = json!(current);
            value["recovery_required"] = json!(record.phase == "prepared");
            records.push(value);

        }
        records.reverse();
        Ok(json!({"records":records,"retention_records":64,"retention_bytes":256*1024*1024}))

    }

    pub(crate) fn restore( &self, id: &str, version: &str, expected: Option<&str>, ) -> Result<Value, FileError> {

        if !valid_hash(id) || !matches!(version, "previous" | "result") {

            return Err(FileError::Data);

        }
        let (path, content) = {

            let _lock = self.lock("access.lock")?;
            let records = self.revisions()?;
            let record = records.iter().find(|record| record.id == id).ok_or(FileError::Data)?;
            let hash = if version == "previous" { &record.previous } else { &record.result };
            let content = hash
                .as_ref()
                .map(|hash| {

                    let bytes = self.load_metadata(&format!("history-blob-{hash}.bin"))?.ok_or(FileError::Data)?;
                    if digest(&bytes) != *hash {

                        return Err(FileError::Data);

                    }
                    Ok(bytes)

                })
                .transpose()?;
            (record.path.clone(), content)

        };
        let sha256 = self.write(&path, content.as_deref(), expected)?;
        Ok(json!({"path":path,"sha256":sha256,"restored_from":id,"version":version}))

    }

}

fn valid_hash( hash: &str, ) -> bool {

    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))

}

#[cfg(test)]
#[path = "../../tests/unit/history.rs"]
mod tests;
