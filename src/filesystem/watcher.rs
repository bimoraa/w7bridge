/*! pairing별 poll·reconnect loop를 유지해. 파일 감지와 비교는 sync Session이 소유해. */

use crate::{
    config::Pair,
    connection::session::Remote,
    filesystem::{FileSettings, FileStore},
    protocol::types::Failure,
    sync::{Session, SyncError},
};
use serde_json::json;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub(crate) fn local_files( pair: &Pair, ) -> Result<FileStore,Failure> {

    let files = FileStore::new(&pair.local_root, FileSettings { enabled: true, ..Default::default() })?;
    let name = format!("policy-{}.json", crate::filesystem::digest(pair.binding()?.as_bytes()));
    match files.load_metadata(&name)? {

        Some(bytes) => Ok(files.with_settings(serde_json::from_slice(&bytes)?)?),
        None => Ok(files),

    }

}

pub(crate) fn cache_policy( pair: &Pair, files: &FileStore, ) -> Result<(),Failure> {

    let name = format!("policy-{}.json", crate::filesystem::digest(pair.binding()?.as_bytes()));
    files.save_metadata(&name, &serde_json::to_vec(files.settings())?)?;
    Ok(())

}

pub(crate) async fn watch(
    pair: Pair,
    interval: u64,
    once: bool,
    status: bool,
    shutdown: CancellationToken,
) -> Result<(), Failure> {

    let files = local_files(&pair)?;
    let binding = pair.binding()?;
    let lock_name = format!("sync-{}.lock", crate::filesystem::digest(binding.as_bytes()));
    if status {

        let session = Session::open_pair(files.clone(), binding.clone())?;
        let mut report = session.report().clone();
        match files.lock(&lock_name) {

            Ok(_) => {

                report.status = crate::sync::Status::Offline;
                report.error = Some("sync daemon이 실행 중이 아닙니다".into());

            }
            Err(crate::FileError::Busy) => {}
            Err(error) => return Err(error.into()),

        }
        println!("{}", json!({ "project_id": pair.remote_project, "report": report }));
        return Ok(());

    }
    // 구버전 daemon이 살아 있으면 새 pair journal과 동시에 쓰지 않아.
    if files.load_metadata("sync.json")?.is_some() {

        let _legacy = files.lock("sync.lock")?;

    }
    let _lock = files.lock(&lock_name)?;
    let mut session = Session::open_pair(files.clone(), binding.clone())?;
    let mut remote: Option<Remote> = None;
    let mut backoff = interval;
    let mut last = String::new();
    loop {

        if shutdown.is_cancelled() {

            break;

        }
        if remote.is_none() {

            let connection = Remote::connect(&pair, shutdown.clone()).await;
            if shutdown.is_cancelled() {

                break;

            }
            match connection {

                Ok(mut peer) => {

                    // 양쪽 모두 peer의 authoritative 제외 설정을 사용해.
                    let settings = tokio::select! {
                        result = peer.identity() => result,
                        _ = shutdown.cancelled() => { peer.close().await; break; }
                    };
                    match settings {

                        Ok((settings, identity)) => {

                            let store = files.with_settings(settings)?;
                            let replacement = Session::open_pair(store, binding.clone());
                            match replacement {

                                Ok(mut replacement) => {

                                    if let Err(error) = replacement.bind_peer(identity) {

                                        peer.close().await;
                                        session.offline(&error.to_string())?;
                                        return Err(error.into());

                                    }
                                    cache_policy(&pair, replacement.files())?;
                                    session = replacement;
                                    remote = Some(peer);

                                }
                                Err(error) => {

                                    peer.close().await;
                                    session.offline(&error.to_string())?;
                                    return Err(error.into());

                                }

                            }

                        }
                        Err(error) => {

                            session.offline(&error.to_string())?;
                            peer.close().await;

                        }

                    }

                }
                Err(error) => {

                    session.offline(&error.to_string())?;

                }

            }

        }
        let mut failure = None;
        if let Some(peer) = &remote {

            let outcome = tokio::select! {
                result = peer.synchronize(&mut session, interval) => result,
                _ = shutdown.cancelled() => break,
            };
            match outcome {

                Ok(()) => backoff = interval,
                Err(SyncError::File(crate::FileError::Conflict)) => {

                    session.syncing("round 중 파일이 변경되었습니다")?

                }
                Err(error) => {

                    session.offline(&error.to_string())?;
                    if matches!(error, SyncError::State) {

                        if let Some(peer) = remote.take() {

                            peer.close().await;

                        }
                        return Err(error.into());

                    }
                    failure = Some(error);

                }

            }

        }
        if failure.is_some()
            && let Some(peer) = remote.take()
        {

            peer.close().await;

        }
        let report = serde_json::to_string(session.report())?;
        let signature = serde_json::to_string(&(
            session.report().status.clone(),
            &session.report().conflicts,
            &session.report().error,
        ))?;
        if signature != last {

            eprintln!("{}: {report}", pair.remote_project);
            last = signature;

        }
        if once {

            if let Some(peer) = remote.take() {

                peer.close().await;

            }
            return if session.report().status == crate::sync::Status::Synced {

                Ok(())

            } else {

                Err("sync가 완료되지 않았습니다. status와 conflict snapshot을 확인하세요".into())

            };

        }
        tokio::select! { _ = shutdown.cancelled() => break, _ = tokio::time::sleep(Duration::from_secs(backoff)) => {} }
        if remote.is_none() {

            backoff = (backoff * 2).min(30);

        }

    }
    session.offline("sync host가 종료되었습니다")?;
    if let Some(peer) = remote {

        peer.close().await;

    }
    Ok(())

}
