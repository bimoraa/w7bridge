/*! manifest와 복원된 baseline을 전송 전에 검증해. */

use super::{SyncError, state::State};
use crate::{
    FileError,
    filesystem::{FileEntry, FileStore},
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn valid_hash(hash: &str) -> bool {

    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))

}

pub(super) fn validate(
    store: &FileStore,
    local: &BTreeMap<String, String>,
    remote: &[FileEntry],
) -> Result<(), SyncError> {

    let mut seen = BTreeSet::new();
    let mut bytes = 0usize;
    if remote.len() > 10000 {

        return Err(FileError::Limit.into());

    }
    for entry in remote {

        if entry.bytes > store.settings().max_file_bytes
            || !valid_hash(&entry.sha256)
            || !seen.insert(entry.path.clone())
        {

            return Err(SyncError::Peer);

        }
        bytes = bytes.checked_add(entry.bytes).ok_or(SyncError::Peer)?;
        if bytes > 256 * 1024 * 1024 {

            return Err(FileError::Limit.into());

        }

    }
    let paths: BTreeSet<_> = local.keys().chain(remote.iter().map(|entry| &entry.path)).cloned().collect();
    validate_paths(store, &paths)

}

fn validate_paths(store: &FileStore, paths: &BTreeSet<String>) -> Result<(), SyncError> {

    if paths.iter().any(|path| !store.permits(path)) {

        return Err(SyncError::State);

    }
    crate::filesystem::validate_paths(paths.iter().map(String::as_str)).map_err(Into::into)

}

pub(super) fn validate_state(store: &FileStore, state: &State) -> Result<(), SyncError> {

    if state.baseline.len() > 10000
        || state.uncertain.len() > 10000
        || state.report.conflicts.len() > 10000
        || state.peer_identity.as_ref().is_some_and(|value| !valid_hash(value))
        || state.baseline.values().flatten().any(|value| !valid_hash(value))
    {

        return Err(SyncError::State);

    }
    let mut paths: BTreeSet<_> = state.baseline.keys().chain(state.uncertain.iter()).cloned().collect();
    if let Some(pending) = &state.pending {

        if pending.expected.iter().chain(pending.intended.iter()).any(|value| !valid_hash(value)) {

            return Err(SyncError::State);

        }
        paths.insert(pending.path.clone());

    }
    validate_paths(store, &paths)

}
