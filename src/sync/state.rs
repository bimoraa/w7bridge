/*! pairing baseline, journal과 관찰 상태를 소유해. */

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::{SystemTime, UNIX_EPOCH},
};
/** CLI와 status 파일에서 사용하는 현재 pairing 상태다. */
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Status {

    Synced,
    Syncing,
    Conflict,
    Offline,

}

/** 충돌 snapshot의 파일 이름과 원본 경로를 연결한다. 삭제된 쪽 hash는 null이다. */
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Conflict {

    pub path: String,
    pub local_hash: Option<String>,
    pub remote_hash: Option<String>,
    pub snapshot: String,

}

/** timestamp는 관찰 시각이다. daemon이 중단된 경우 마지막 관찰만 남으므로 freshness를 확인해야 한다. */
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Report {

    pub status: Status,
    pub observed_at: u64,
    pub conflicts: Vec<Conflict>,
    pub error: Option<String>,

}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Pending {

    pub(super) path: String,
    pub(super) remote_target: bool,
    pub(super) expected: Option<String>,
    pub(super) intended: Option<String>,

}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct State {

    pub(super) version: u32,
    pub(super) binding: String,
    pub(super) baseline: BTreeMap<String, Option<String>>,
    pub(super) pending: Option<Pending>,
    #[serde(default)]
    pub(super) peer_identity: Option<String>,
    #[serde(default)]
    pub(super) uncertain: BTreeSet<String>,
    pub(super) report: Report,

}

pub(super) fn now() -> u64 {

    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |duration| duration.as_secs())

}

pub(crate) mod coordination {

    use crate::{
        files::{FileStore, digest, hashes},
        security::Policy,
    };
    use serde_json::{Value, json};
    use std::{
        collections::BTreeMap,
        sync::Mutex,
        time::{Duration, Instant},
    };
    use tokio_util::sync::CancellationToken;

    pub(crate) struct Checkpoint<'a> {

        pub generation: u64,
        pub status: &'a str,
        pub hash: &'a str,
        pub conflicts: Vec<String>,
        pub lease_seconds: u64,
        pub latency_ms: Option<u64>,

    }

    struct Receipt {

        requested: u64,
        confirmed: u64,
        status: String,
        hash: String,
        conflicts: Vec<String>,
        received: Instant,
        lease: Duration,
        latency_ms: Option<u64>,
        completed_hash: Option<String>,

    }

    /** Mac sync daemon의 observation을 저장한다. 원격 client가 registry를 수정하지 않는다. */
    pub(crate) struct Coordinator {

        receipts: Mutex<BTreeMap<(String, String), Receipt>>,

    }

    impl Coordinator {

        pub fn new() -> Self {

            Self { receipts: Mutex::new(BTreeMap::new()) }

        }

        pub fn status(&self, project: &str, files: &FileStore) -> Result<Value, String> {

            let peers = {

                let receipts = self.receipts.lock().map_err(|_| "sync 상태를 읽을 수 없습니다")?;
                receipts
                    .iter()
                    .filter(|((id, _), receipt)| id == project && receipt.received.elapsed() <= receipt.lease)
                    .map(|((_, peer), _)| peer.clone())
                    .collect::<Vec<_>>()

            };
            if peers.is_empty() {

                return self.status_peer(project, files, None);

            }
            let mut reports = Vec::new();
            for peer in &peers {

                reports.push(self.status_peer(project, files, Some(peer))?);

            }
            let index = reports
                .iter()
                .position(|value| value["status"] == "conflict")
                .or_else(|| reports.iter().position(|value| value["status"] != "synced"))
                .unwrap_or(0);
            let mut status = reports[index].clone();
            status["peers"] = json!(reports);
            Ok(status)

        }

        pub fn status_peer( &self, project: &str, files: &FileStore, peer: Option<&str>, ) -> Result<Value, String> {

            let key = peer_key(project, peer)?;

            let (requested, confirmed, mut status, hash, conflicts, age, lease, latency_ms) = {

                let receipts = self.receipts.lock().map_err(|_| "sync 상태를 읽을 수 없습니다")?;
                match receipts.get(&key) {

                    Some(receipt) => (
                        receipt.requested,
                        receipt.confirmed,
                        receipt.status.clone(),
                        receipt.hash.clone(),
                        receipt.conflicts.clone(),
                        receipt.received.elapsed(),
                        receipt.lease,
                        receipt.latency_ms,
                    ),
                    None => {

                        return Ok(
                            json!({ "status": "offline", "requested_generation": 0, "confirmed_generation": 0, "conflicts": [] }),
                        );

                    }

                }

            };
            if age > lease {

                status = "offline".into();

            } else if status == "synced"
                && (requested > confirmed || manifest(files).map_err(|error| error.to_string())? != hash)
            {

                status = "syncing".into();

            }
            Ok(json!({ "status": status, "requested_generation": requested, "confirmed_generation": confirmed,
            "conflicts": conflicts, "manifest_hash": hash, "checkpoint_age_seconds": age.as_secs(), "lease_seconds": lease.as_secs(), "rpc_latency_ms":latency_ms }))

        }

        pub fn checkpoint_peer(
            &self,
            project: &str,
            files: &FileStore,
            checkpoint: Checkpoint<'_>,
            peer: Option<&str>,
        ) -> Result<Value, String> {

            let Checkpoint { generation, status, hash, conflicts, lease_seconds, latency_ms } = checkpoint;
            if !matches!(status, "synced" | "syncing" | "conflict")
                || !(3..=180).contains(&lease_seconds)
                || hash.len() != 64
                || !hash.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                || conflicts.len() > 10000
                || conflicts.iter().any(|path| !files.permits(path))
                || latency_ms.is_some_and(|latency| latency > 60000)
            {

                return Err("sync checkpoint 인자가 올바르지 않습니다".into());

            }
            let matched =
                if status == "synced" { manifest(files).map_err(|error| error.to_string())? == hash } else { true };
            let key = peer_key(project, peer)?;
            let mut receipts = self.receipts.lock().map_err(|_| "sync 상태를 읽을 수 없습니다")?;
            if !receipts.contains_key(&key) && receipts.keys().filter(|(id, _)| id == project).count() >= 16 {

                return Err("project sync peer 한도는 16입니다".into());

            }
            let receipt = receipts.entry(key).or_insert_with(|| Receipt {

                requested: 0,
                confirmed: 0,
                status: "offline".into(),
                hash: String::new(),
                conflicts: vec![],
                received: Instant::now(),
                lease: Duration::from_secs(lease_seconds),
                latency_ms: None,
                completed_hash: None,

            });
            if generation > receipt.requested {

                return Err("sync generation이 현재 요청보다 앞에 있습니다".into());

            }
            if generation < receipt.confirmed {

                return Err("이미 확인한 round보다 오래된 sync checkpoint입니다".into());

            }
            receipt.status = if matched { status.into() } else { "syncing".into() };
            receipt.hash = hash.into();
            receipt.conflicts = conflicts;
            receipt.received = Instant::now();
            receipt.lease = Duration::from_secs(lease_seconds);
            receipt.latency_ms = latency_ms.or(receipt.latency_ms);
            let changed = matched && status == "synced" && receipt.completed_hash.as_deref() != Some(hash);
            if matched && status == "synced" {

                receipt.confirmed = receipt.confirmed.max(generation);
                receipt.completed_hash = Some(hash.into());

            }
            Ok(
                json!({ "accepted": matched, "requested_generation": receipt.requested, "changed":changed, "manifest_hash":hash }),
            )

        }

        pub async fn wait_peer(
            &self,
            project: &str,
            files: &FileStore,
            seconds: u64,
            cancellation: CancellationToken,
            peer: Option<&str>,
        ) -> Result<Value, String> {

            if !(1..=120).contains(&seconds) {

                return Err("sync 대기 한도는 1..=120초입니다".into());

            }
            if cancellation.is_cancelled() {

                return Err("sync 대기가 취소되었습니다".into());

            }
            let selected_peer = if peer.is_none() {

                let receipts = self.receipts.lock().map_err(|_| "sync 상태를 읽을 수 없습니다")?;
                let peers = receipts
                    .iter()
                    .filter(|((id, peer), receipt)| {

                        id == project && !peer.is_empty() && receipt.received.elapsed() <= receipt.lease

                    })
                    .map(|((_, peer), _)| peer.clone())
                    .collect::<Vec<_>>();
                if peers.len() > 1 {

                    return Err("여러 sync peer에서는 hub의 project/device ID를 선택하세요".into());

                }
                peers.into_iter().next()

            } else {

                None

            };
            let peer = peer.or(selected_peer.as_deref());
            let key = peer_key(project, peer)?;
            let generation = {

                let mut receipts = self.receipts.lock().map_err(|_| "sync 상태를 읽을 수 없습니다")?;
                if !receipts.contains_key(&key) && receipts.keys().filter(|(id, _)| id == project).count() >= 16 {

                    return Err("project sync peer 한도는 16입니다".into());

                }
                let receipt = receipts.entry(key).or_insert_with(|| Receipt {

                    requested: 0,
                    confirmed: 0,
                    status: "offline".into(),
                    hash: String::new(),
                    conflicts: vec![],
                    received: Instant::now(),
                    lease: Duration::ZERO,
                    latency_ms: None,
                    completed_hash: None,

                });
                receipt.requested = receipt.requested.checked_add(1).ok_or("sync generation 한도를 초과했습니다")?;
                receipt.requested

            };
            let started = Instant::now();
            loop {

                let status = self.status_peer(project, files, peer)?;
                let all = self.status(project, files)?;
                if all["status"] == "conflict" {

                    return Err("다른 paired device의 sync conflict를 해결하세요".into());

                }
                if status["status"] == "synced"
                    && status["confirmed_generation"].as_u64().is_some_and(|confirmed| confirmed >= generation)
                {

                    return Ok(status);

                }
                if status["status"] == "conflict" {

                    return Err("sync conflict를 해결한 뒤 명령을 실행하세요".into());

                }
                if started.elapsed() >= Duration::from_secs(seconds) {

                    return Err("sync 확인 시간이 초과되었습니다. daemon과 peer 연결을 확인하세요".into());

                }
                tokio::select! { _ = cancellation.cancelled() => return Err("sync 대기가 취소되었습니다".into()),
                _ = tokio::time::sleep(Duration::from_millis(200)) => {} }

            }

        }

        pub async fn gate_peer(
            &self,
            policy: &Policy,
            project: &str,
            cancellation: CancellationToken,
            peer: Option<&str>,
        ) -> Result<Option<String>, String> {

            if policy.requires_sync(project).map_err(|error| error.to_string())? {

                let files = policy.files(project).map_err(|error| error.to_string())?;
                let confirmed = self.wait_peer(project, &files, 30, cancellation, peer).await?;
                return confirmed["manifest_hash"]
                    .as_str()
                    .map(|hash| Some(hash.to_owned()))
                    .ok_or_else(|| "sync revision을 확인할 수 없습니다".into());

            }
            Ok(None)

        }

    }

    fn peer_key( project: &str, peer: Option<&str>, ) -> Result<(String,String),String> {

        if peer.is_some_and(|peer| {

            !peer.is_empty()
                && (peer.len() != 64
                    || !peer.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))

        }) {

            return Err("sync peer ID는 SHA-256이어야 합니다".into());

        }
        Ok((project.into(), peer.unwrap_or_default().into()))

    }

    pub(crate) fn manifest(files: &FileStore) -> Result<String, crate::FileError> {

        Ok(digest(&serde_json::to_vec(&hashes(&files.list()?)).map_err(|_| crate::FileError::Data)?))

    }

}

#[cfg(test)]
#[path = "../../tests/unit/coordination.rs"]
mod coordination_tests;
