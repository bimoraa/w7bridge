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
pub(crate) async fn watch(
    pair: Pair,
    interval: u64,
    once: bool,
    status: bool,
    shutdown: CancellationToken,
) -> Result<(), Failure> {

    let files = FileStore::new(&pair.local_root, FileSettings { enabled: true, ..Default::default() })?;
    if status {

        let session = Session::open(files.clone(), pair.binding()?)?;
        let mut report = session.report().clone();
        match files.lock("sync.lock") {

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
    let _lock = files.lock("sync.lock")?;
    let mut session = Session::open(files.clone(), pair.binding()?)?;
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
                            let replacement = Session::open(store, pair.binding()?);
                            match replacement {

                                Ok(mut replacement) => {

                                    if let Err(error) = replacement.bind_peer(identity) {

                                        peer.close().await;
                                        session.offline(&error.to_string())?;
                                        return Err(error.into());

                                    }
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
