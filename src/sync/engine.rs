/*! 단일 sync round와 write-ahead journal을 실행해. */

use super::{
    Peer, SyncError, manifest, planner,
    state::{Pending, Report, State, Status, now},
};
use crate::{
    FileError,
    filesystem::{FileStore, digest, hashes},
};
use std::collections::{BTreeMap, BTreeSet};
/** 로컬 project의 단일 sync owner다. process 간 독점은 `sync.lock`을 유지하는 host가 담당한다. */
pub struct Session {

    pub(super) local: FileStore,
    pub(super) state: State,
    metadata_name: String,

}

impl Session {

    /** 기존 baseline을 읽거나 새 pairing을 시작한다. 다른 peer로 재사용하면 실패한다. */
    pub fn open(local: FileStore, binding: String) -> Result<Self, SyncError> {

        Self::open_named(local, binding, "sync.json".into())

    }

    pub(crate) fn open_pair( local: FileStore, binding: String, ) -> Result<Self,SyncError> {

        let name = format!("sync-{}.json", digest(binding.as_bytes()));
        if local.load_metadata(&name)?.is_none()
            && let Some(bytes) = local.load_metadata("sync.json")?
        {

            let previous: State = serde_json::from_slice(&bytes).map_err(|_| SyncError::State)?;
            if previous.binding == binding {

                manifest::validate_state(&local, &previous)?;
                local.save_metadata(&name, &bytes)?;

            }

        }
        Self::open_named(local, binding, name)

    }

    fn open_named( local: FileStore, binding: String, metadata_name: String, ) -> Result<Self,SyncError> {

        let state = match local.load_metadata(&metadata_name)? {

            Some(bytes) => {

                let state: State = serde_json::from_slice(&bytes).map_err(|_| SyncError::State)?;
                if state.version != 1 || state.binding != binding {

                    return Err(SyncError::State);

                }
                manifest::validate_state(&local, &state)?;
                state

            }
            None => State {

                version: 1,
                binding,
                baseline: BTreeMap::new(),
                pending: None,
                peer_identity: None,
                uncertain: BTreeSet::new(),
                report: Report { status: Status::Offline, observed_at: now(), conflicts: vec![], error: None },

            },

        };
        Ok(Self { local, state, metadata_name })

    }

    /** peer root와 제외 정책 identity를 고정한다. 변경되면 baseline을 자동으로 초기화하지 않는다. */
    pub fn bind_peer(&mut self, identity: String) -> Result<(), SyncError> {

        if !manifest::valid_hash(&identity) {

            return Err(SyncError::State);

        }
        if self.state.peer_identity.as_ref().is_some_and(|previous| previous != &identity) {

            return Err(SyncError::State);

        }
        self.state.peer_identity = Some(identity);
        self.save()

    }

    /** 로컬 파일의 현재 정렬된 manifest hash를 반환한다. IO를 수행한다. */
    pub fn manifest_hash(&self) -> Result<String, SyncError> {

        Ok(digest(&serde_json::to_vec(&hashes(&self.local.list()?))?))

    }

    /** network 없이 마지막 status를 읽는다. observed_at으로 중단된 daemon의 오래된 값을 식별한다. */
    pub fn report(&self) -> &Report {

        &self.state.report

    }

    pub(crate) fn files( &self, ) -> &FileStore {

        &self.local

    }

    pub(crate) fn git_conflict( &mut self, reason: &str, ) -> Result<(),SyncError> {

        self.state.report.status = Status::Conflict;
        self.state.report.error = Some(reason.into());
        self.state.report.observed_at = now();
        self.save()

    }

    /** 파일이 계속 바뀌는 round의 상태를 저장한다. baseline은 유지한다. */
    pub fn syncing(&mut self, reason: &str) -> Result<(), SyncError> {

        self.state.report.status = Status::Syncing;
        self.state.report.observed_at = now();
        self.state.report.error = Some(reason.into());
        self.save()

    }

    /** 연결 실패를 저장한다. baseline과 미확인 write journal은 유지한다. */
    pub fn offline(&mut self, error: &str) -> Result<(), SyncError> {

        self.state.report.status = Status::Offline;
        self.state.report.observed_at = now();
        self.state.report.error = Some(error.to_owned());
        self.save()

    }

    /** 한 번 비교하고 변경을 전달한다. 동시 수정은 양쪽 원본을 유지하고 별도 snapshot으로 보존한다.
    최초 sync는 한쪽에만 있는 파일을 복사하며 서로 다른 기존 파일은 conflict다.
    파일별 journal을 먼저 저장하므로 응답 유실 후 같은 삭제/쓰기를 무조건 재시도하지 않는다. */
    pub async fn round(&mut self, peer: &impl Peer) -> Result<&Report, SyncError> {

        self.state.report.status = Status::Syncing;
        self.state.report.observed_at = now();
        self.state.report.error = None;
        self.save()?;
        let local_entries = self.local.list()?;
        let local = hashes(&local_entries);
        let remote_entries = peer.list().await?;
        manifest::validate(&self.local, &local, &remote_entries)?;
        let remote = hashes(&remote_entries);
        if let Some(pending) = self.state.pending.take() {

            let target = if pending.remote_target { &remote } else { &local };
            let current = target.get(&pending.path).cloned();
            if current == pending.intended {

                self.state.baseline.insert(pending.path, pending.intended);

            } else if current != pending.expected {

                self.state.uncertain.insert(pending.path);

            }
            self.save()?;

        }
        let paths: BTreeSet<_> = local.keys().chain(remote.keys()).chain(self.state.baseline.keys()).cloned().collect();
        let sizes: BTreeMap<_, _> =
            local_entries.iter().chain(&remote_entries).map(|entry| (&entry.path, entry.bytes)).collect();
        let mut paths: Vec<_> = paths.into_iter().collect();
        paths.sort_by_key(|path| {

            let deletion =
                self.state.baseline.contains_key(path) && (!local.contains_key(path) || !remote.contains_key(path));
            let source = path.starts_with("src/")
                || matches!(path.as_str(), "AGENTS.md" | "MEMORY.md" | "PLANS.md" | "Cargo.toml" | "package.json")
                || [".rs", ".ts", ".tsx", ".js", ".jsx", ".py", ".swift", ".lua"]
                    .iter()
                    .any(|extension| path.ends_with(extension));
            (deletion, !source, sizes.get(path).copied().unwrap_or(usize::MAX), path.clone())

        });
        let mut conflicts = Vec::new();
        for path in paths {

            let left = local.get(&path).cloned();
            let right = remote.get(&path).cloned();
            if left == right {

                self.state.uncertain.remove(&path);
                if left.is_some() {

                    self.state.baseline.insert(path, left);

                } else {

                    self.state.baseline.remove(&path);

                }
                continue;

            }
            let baseline = self.state.baseline.get(&path);
            let direction = planner::direction(self.state.uncertain.contains(&path), baseline, &left, &right);
            let Some(remote_target) = direction else {

                conflicts.push(self.preserve(peer, &path, left.as_deref(), right.as_deref()).await?);
                continue;

            };
            let (source, expected) = if remote_target { (&left, &right) } else { (&right, &left) };
            let content = self.content(peer, &path, source.as_deref(), !remote_target).await?;
            self.state.pending = Some(Pending {

                path: path.clone(),
                remote_target,
                expected: expected.clone(),
                intended: source.clone(),

            });
            self.save()?;
            let result = if remote_target {

                peer.write(&path, content.as_deref(), expected.as_deref()).await

            } else {

                self.local.write(&path, content.as_deref(), expected.as_deref()).map(|_| ()).map_err(Into::into)

            };
            match result {

                Ok(()) => {

                    if !remote_target && let Some(hash) = source {

                        self.local.complete_staging(&path, hash)?;

                    }
                    self.state.baseline.insert(path, source.clone());
                    self.state.pending = None;
                    self.save()?;

                }
                Err(SyncError::File(FileError::Conflict)) => {

                    // 목록 이후 수정된 파일은 다음 round에서 다시 읽고 충돌을 보존해.
                    return Err(SyncError::File(FileError::Conflict));

                }
                Err(error) => return Err(error),

            }

        }
        let final_local = hashes(&self.local.list()?);
        let final_remote = peer.list().await?;
        manifest::validate(&self.local, &final_local, &final_remote)?;
        let converged = final_local == hashes(&final_remote);
        self.state.report = Report {

            status: if !conflicts.is_empty() {

                Status::Conflict

            } else if converged {

                Status::Synced

            } else {

                Status::Syncing

            },
            observed_at: now(),
            conflicts,
            error: None,

        };
        self.save()?;
        Ok(&self.state.report)

    }

    pub(super) async fn content(
        &self,
        peer: &impl Peer,
        path: &str,
        hash: Option<&str>,
        remote: bool,
    ) -> Result<Option<Vec<u8>>, SyncError> {

        let Some(hash) = hash else {

            let result = if remote { peer.read(path).await } else { self.local.read(path).map_err(Into::into) };
            return match result {

                Err(SyncError::File(FileError::Io(error))) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Ok(_) => Err(SyncError::File(FileError::Conflict)),
                Err(error) => Err(error),

            };

        };
        let content = if remote { peer.read_reusing(path, &self.local).await? } else { self.local.read(path)? };
        if digest(&content) != hash {

            return Err(SyncError::File(FileError::Conflict));

        }
        Ok(Some(content))

    }

    fn save(&self) -> Result<(), SyncError> {

        self.local.save_metadata(&self.metadata_name, &serde_json::to_vec(&self.state)?)?;
        Ok(())

    }

}
