/*! SSH child와 지속 MCP peer 연결의 종료를 소유해. */

use super::{handshake::check_transport, transport::Options};
use crate::{
    ConnectError,
    config::Pair,
    filesystem::FileEntry,
    protocol::types::Failure,
    sync::{Peer, SyncError},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use rmcp::{RoleClient, ServiceExt, model::CallToolRequestParams, service::RunningService};
use serde_json::{Value, json};
use std::{ffi::OsString, process::Stdio, time::Duration};
use tokio::{
    process::{Child, Command},
    time::timeout,
};
pub(super) async fn probe(mut command: Command, limit: Duration) -> Result<(), ConnectError> {

    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .map_err(|source| ConnectError::Process { operation: "SSH 연결", source })?;
    let input = child.stdout.take().ok_or(ConnectError::Protocol)?;
    let output = child.stdin.take().ok_or(ConnectError::Protocol)?;
    let result = tokio::select! {
        result = timeout(limit, check_transport(input, output)) => result.unwrap_or(Err(ConnectError::Timeout)),
        _ = tokio::signal::ctrl_c() => Err(ConnectError::Cancelled),
    };
    // 실패와 timeout도 local SSH를 회수해. remote 서버는 stdin 종료로 같은 수명을 따라가.
    let _ = child.start_kill();
    let cleanup = timeout(Duration::from_secs(5), child.wait()).await;
    if result.is_err() && !matches!(cleanup, Ok(Ok(_))) {

        eprintln!("SSH 프로세스 종료도 확인할 수 없습니다");

    }
    result?;
    cleanup.map_err(|_| ConnectError::Cleanup)?.map_err(|_| ConnectError::Cleanup)?;
    Ok(())

}

impl Pair {

    pub(crate) fn options(&self) -> Result<Options, Failure> {

        let mut args = vec![OsString::from("--host"), self.host.clone().into()];
        for (flag, value) in
            [("--executable", &self.executable), ("--config", &self.config), ("--identity", &self.identity)]
        {

            if let Some(value) = value {

                args.extend([flag.into(), value.into()]);

            }

        }
        if let Some(port) = self.port {

            args.extend(["--port".into(), port.to_string().into()]);

        }
        if self.service {

            args.push("--service".into());

        }
        Ok(Options::parse(&args)?)

    }
    pub(crate) fn binding(&self) -> Result<String, Failure> {

        Ok(serde_json::to_string(&(self.options()?.ssh_args(), &self.remote_project, self.local_root.canonicalize()?))?)

    }

}

pub(crate) struct Remote {

    client: RunningService<RoleClient, ()>,
    child: Child,
    project: String,
    root_key: Option<String>,
    generation: std::sync::atomic::AtomicU64,
    lease: std::sync::atomic::AtomicU64,
    last_heartbeat: std::sync::Mutex<std::time::Instant>,

}

impl Remote {

    pub(crate) async fn connect(pair: &Pair, shutdown: tokio_util::sync::CancellationToken) -> Result<Self, Failure> {

        let mut child = Command::new("ssh")
            .args(pair.options()?.ssh_args())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()?;
        let input = child.stdout.take().ok_or(SyncError::Offline)?;
        let output = child.stdin.take().ok_or(SyncError::Offline)?;
        let handshake = tokio::select! {
            result = timeout(Duration::from_secs(30), ().serve((input, output))) => result,
            _ = shutdown.cancelled() => { let _ = child.start_kill(); let _ = timeout(Duration::from_secs(5), child.wait()).await; return Err(SyncError::Offline.into()); },
        };
        let client = match handshake {

            Ok(Ok(client)) => client,
            _ => {

                let _ = child.start_kill();
                let _ = timeout(Duration::from_secs(5), child.wait()).await;
                return Err(SyncError::Offline.into());

            }

        };
        if !client
            .peer_info()
            .is_some_and(|info| info.server_info.as_ref().is_some_and(|server| server.name == "w7bridge"))
        {

            let _ = child.start_kill();
            let _ = timeout(Duration::from_secs(5), child.wait()).await;
            return Err(SyncError::Peer.into());

        }
        Ok(Self {

            client,
            child,
            project: pair.remote_project.clone(),
            root_key: None,
            generation: std::sync::atomic::AtomicU64::new(0),
            lease: std::sync::atomic::AtomicU64::new(6),
            last_heartbeat: std::sync::Mutex::new(std::time::Instant::now()),

        })

    }

    async fn request(&self, name: &str, mut args: Value) -> Result<Value, SyncError> {

        args["project_id"] = json!(self.project);
        let args = args.as_object().cloned().ok_or(SyncError::Peer)?;
        let result = timeout(
            Duration::from_secs(30),
            self.client.call_tool(CallToolRequestParams::new(name.to_owned()).with_arguments(args)),
        )
        .await
        .map_err(|_| SyncError::Offline)?
        .map_err(|_| SyncError::Offline)?;
        if result.is_error == Some(true) {

            if result.structured_content.as_ref().and_then(|value| value.get("code")).and_then(Value::as_str)
                == Some("conflict")
            {

                return Err(SyncError::File(crate::FileError::Conflict));

            }
            if result.structured_content.as_ref().and_then(|value| value.get("code")).and_then(Value::as_str)
                == Some("not_found")
            {

                return Err(SyncError::File(crate::FileError::Io(std::io::Error::from(std::io::ErrorKind::NotFound))));

            }
            return Err(SyncError::Peer);

        }
        result.structured_content.ok_or(SyncError::Peer)

    }

    pub(crate) async fn call(&self, name: &str, args: Value) -> Result<Value, SyncError> {

        let result = self.request(name, args).await?;
        let heartbeat = if matches!(name, "list_files" | "read_file" | "write_file") {

            let mut last = self.last_heartbeat.lock().map_err(|_| SyncError::State)?;
            if last.elapsed() >= Duration::from_secs(2) {

                *last = std::time::Instant::now();
                true

            } else {

                false

            }

        } else {

            false

        };
        if heartbeat {

            self.request(
                "sync_checkpoint",
                json!({"generation": self.generation.load(std::sync::atomic::Ordering::Relaxed),
                "status": "syncing", "manifest_hash": "0".repeat(64), "conflicts": [],
                "lease_seconds": self.lease.load(std::sync::atomic::Ordering::Relaxed)}),
            )
            .await?;

        }
        Ok(result)

    }

    pub(crate) async fn identity(&mut self) -> Result<(crate::filesystem::FileSettings, String), SyncError> {

        let result = self.call("list_files", json!({})).await?;
        let settings: crate::filesystem::FileSettings =
            serde_json::from_value(result["settings"].clone()).map_err(|_| SyncError::Peer)?;
        let key = result["root_key"].as_str().ok_or(SyncError::Peer)?;
        if key.len() != 64 || !key.bytes().all(|byte| byte.is_ascii_hexdigit()) {

            return Err(SyncError::Peer);

        }
        self.root_key = Some(key.to_owned());
        let identity = crate::filesystem::digest(&serde_json::to_vec(&(key, &settings))?);
        Ok((settings, identity))

    }

    pub(crate) async fn synchronize(&self, session: &mut crate::sync::Session, interval: u64) -> Result<(), SyncError> {

        let status = self.call("sync_status", json!({})).await?;
        let generation = status["requested_generation"].as_u64().ok_or(SyncError::Peer)?;
        let lease_seconds = (interval * 3).clamp(6, 180);
        self.generation.store(generation, std::sync::atomic::Ordering::Relaxed);
        self.lease.store(lease_seconds, std::sync::atomic::Ordering::Relaxed);
        self.call(
            "sync_checkpoint",
            json!({"generation": generation, "status": "syncing", "manifest_hash": session.manifest_hash()?,
            "conflicts": [], "lease_seconds": lease_seconds}),
        )
        .await?;
        let report = session.round(self).await?.clone();
        let accepted = self.call("sync_checkpoint", json!({"generation": generation, "status": report.status,
            "manifest_hash": session.manifest_hash()?, "conflicts": report.conflicts.iter().map(|conflict| &conflict.path).collect::<Vec<_>>(),
            "lease_seconds": lease_seconds})).await?;
        if accepted["accepted"] == false {

            session.syncing("peer가 checkpoint 직전에 변경되었습니다")?;

        }
        Ok(())

    }

    pub(crate) async fn close(mut self) {

        let _ = timeout(Duration::from_secs(5), self.client.cancel()).await;
        let _ = self.child.start_kill();
        let _ = timeout(Duration::from_secs(5), self.child.wait()).await;

    }

}

impl Peer for Remote {

    async fn list(&self) -> Result<Vec<FileEntry>, SyncError> {

        let result = self.call("list_files", json!({})).await?;
        if self.root_key.as_deref() != result["root_key"].as_str() {

            return Err(SyncError::State);

        }
        serde_json::from_value(result["files"].clone()).map_err(|_| SyncError::Peer)

    }
    async fn read(&self, path: &str) -> Result<Vec<u8>, SyncError> {

        let result = self.call("read_file", json!({ "path": path })).await?;
        let encoded = result["content_base64"].as_str().ok_or(SyncError::Peer)?;
        if encoded.len() > 1_398_104 {

            return Err(SyncError::Peer);

        }
        STANDARD.decode(encoded).map_err(|_| SyncError::Peer)

    }
    async fn write(&self, path: &str, content: Option<&[u8]>, expected: Option<&str>) -> Result<(), SyncError> {

        self.call(
            "write_file",
            json!({ "path": path, "content_base64": content.map(|bytes| STANDARD.encode(bytes)),
            "expected_hash": expected }),
        )
        .await?;
        Ok(())

    }

}
