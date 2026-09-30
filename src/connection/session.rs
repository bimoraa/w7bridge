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
    chunk_sync: bool,
    peer_sync: std::sync::atomic::AtomicBool,
    sync_peer: String,
    bandwidth: u64,
    next_transfer: std::sync::Mutex<tokio::time::Instant>,
    expected_device: Option<String>,
    pub(super) git_executable: Option<std::path::PathBuf>,
    pub(super) git_baseline_name: String,
    rpc_latency_ms: std::sync::atomic::AtomicU64,

}

impl Remote {

    pub(crate) async fn forward( &self, name: &str, mut arguments: serde_json::Map<String,Value>, cancellation: tokio_util::sync::CancellationToken, ) -> Result<rmcp::model::CallToolResult,SyncError> {

        if name != "capture_screenshot" && name != "bridge_info" {

            arguments.insert("project_id".into(), json!(self.project));

        }
        self.select_peer(name, &mut arguments);
        let mut request = self
            .client
            .send_cancellable_request(
                rmcp::model::ClientRequest::CallToolRequest(rmcp::model::CallToolRequest::new(
                    CallToolRequestParams::new(name.to_owned()).with_arguments(arguments),
                )),
                rmcp::service::PeerRequestOptions::no_options(),
            )
            .await
            .map_err(|_| SyncError::Offline)?;
        let response = tokio::select! {
            result = &mut request.rx => match result.map_err(|_| SyncError::Offline)? {
                Ok(response) => response,
                Err(rmcp::service::ServiceError::McpError(error)) => return Ok(rmcp::model::CallToolResult::structured_error(
                    json!({"code":"remote_rpc_error","rpc_code":error.code,"message":error.message}),
                )),
                Err(_) => return Err(SyncError::Offline),
            },
            _ = cancellation.cancelled() => {
                let _ = timeout(Duration::from_secs(5), request.cancel(Some("hub 요청 취소".into()))).await;
                return Err(SyncError::Offline);
            },
        };
        match response {

            rmcp::model::ServerResult::CallToolResult(result) => Ok(result),
            _ => Err(SyncError::Peer),

        }

    }

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
        let mut remote = Self {

            client,
            child,
            project: pair.remote_project.clone(),
            root_key: None,
            generation: std::sync::atomic::AtomicU64::new(0),
            lease: std::sync::atomic::AtomicU64::new(6),
            last_heartbeat: std::sync::Mutex::new(std::time::Instant::now()),
            chunk_sync: false,
            peer_sync: std::sync::atomic::AtomicBool::new(false),
            sync_peer: crate::filesystem::digest(pair.binding()?.as_bytes()),
            bandwidth: pair.bandwidth_bytes_per_second,
            next_transfer: std::sync::Mutex::new(tokio::time::Instant::now()),
            expected_device: pair.expected_device_id.clone(),
            git_executable: pair.git_executable.clone(),
            git_baseline_name: format!("git-pair-{}.json", crate::filesystem::digest(pair.binding()?.as_bytes())),
            rpc_latency_ms: std::sync::atomic::AtomicU64::new(0),

        };
        let negotiation = tokio::select! {
            result=remote.negotiate()=>result,
            _=shutdown.cancelled()=>Err(SyncError::Offline),
        };
        match negotiation {

            Ok(chunks) => {

                if !chunks && remote.bandwidth > 0 {

                    remote.close().await;
                    return Err(SyncError::PeerConfig("bandwidth 제한에는 chunk_sync peer가 필요합니다").into());

                }
                remote.chunk_sync = chunks;
                Ok(remote)

            }
            Err(error) => {

                remote.close().await;
                Err(error.into())

            }

        }

    }

    fn select_peer(&self, name: &str, arguments: &mut serde_json::Map<String, Value>) {

        if self.peer_sync.load(std::sync::atomic::Ordering::Relaxed)
            && matches!(
                name,
                "sync_status"
                    | "sync_checkpoint"
                    | "wait_for_sync"
                    | "project_status"
                    | "start_process"
                    | "run_command"
                    | "restart_process"
            )
        {

            arguments.insert("sync_peer".into(), json!(self.sync_peer));

        }

    }

    async fn request(&self, name: &str, mut args: Value) -> Result<Value, SyncError> {

        let started = std::time::Instant::now();
        if name != "bridge_info" {

            args["project_id"] = json!(self.project);

        }
        let mut args = args.as_object().cloned().ok_or(SyncError::Peer)?;
        self.select_peer(name, &mut args);
        let result = timeout(
            Duration::from_secs(30),
            self.client.call_tool(CallToolRequestParams::new(name.to_owned()).with_arguments(args)),
        )
        .await
        .map_err(|_| SyncError::Offline)?
        .map_err(|_| SyncError::Offline)?;
        self.rpc_latency_ms.store(started.elapsed().as_millis() as u64, std::sync::atomic::Ordering::Relaxed);
        if result.is_error == Some(true) {

            if name.contains("git") {

                return Err(SyncError::Git(
                    result
                        .structured_content
                        .as_ref()
                        .and_then(|value| value["message"].as_str())
                        .unwrap_or("peer가 Git 인계를 거부했습니다")
                        .chars()
                        .take(512)
                        .collect(),
                ));

            }
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
        let heartbeat = if matches!(
            name,
            "list_files"
                | "read_file"
                | "write_file"
                | "file_chunks"
                | "read_chunk"
                | "prepare_transfer"
                | "put_chunk"
                | "commit_transfer"
        ) {

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

    async fn negotiate( &self, ) -> Result<bool,SyncError> {

        let tools = timeout(Duration::from_secs(30), self.client.list_tools(None))
            .await
            .map_err(|_| SyncError::Offline)?
            .map_err(|_| SyncError::Offline)?;
        if !tools.tools.iter().any(|tool| tool.name == "bridge_info") {

            if self.expected_device.is_some() || self.git_executable.is_some() {

                return Err(SyncError::Peer);

            }
            return Ok(false);

        }
        let info = self.request("bridge_info", json!({"versions":[2,1]})).await?;
        if !matches!(info["protocol_version"].as_u64(), Some(1 | 2)) {

            return Err(SyncError::Peer);

        }
        if self.expected_device.as_ref().is_some_and(|device| info["device_id"].as_str() != Some(device)) {

            return Err(SyncError::Peer);

        }
        self.peer_sync.store(
            info["features"].as_array().is_some_and(|features| features.iter().any(|feature| feature == "peer_sync")),
            std::sync::atomic::Ordering::Relaxed,
        );
        Ok(info["features"].as_array().is_some_and(|features| features.iter().any(|feature| feature == "chunk_sync"))
            && ["file_chunks", "read_chunk", "prepare_transfer", "put_chunk", "commit_transfer"]
                .iter()
                .all(|name| tools.tools.iter().any(|tool| tool.name == *name)))

    }

    pub(super) async fn throttle( &self, bytes: usize, ) -> Result<(),SyncError> {

        if self.bandwidth == 0 || bytes == 0 {

            return Ok(());

        }
        let deadline = {

            let mut next = self.next_transfer.lock().map_err(|_| SyncError::State)?;
            let charged = bytes.div_ceil(3) * 4 + 1024;
            let duration = Duration::from_secs_f64(charged as f64 / self.bandwidth as f64);
            *next = (*next).max(tokio::time::Instant::now()) + duration;
            *next

        };
        while tokio::time::Instant::now() < deadline {

            tokio::select! {
                _=tokio::time::sleep_until(deadline)=>break,
                _=tokio::time::sleep(Duration::from_secs(2))=>{
                    self.request("sync_checkpoint",json!({"generation":self.generation.load(std::sync::atomic::Ordering::Relaxed),
                        "status":"syncing","manifest_hash":"0".repeat(64),"conflicts":[],"lease_seconds":self.lease.load(std::sync::atomic::Ordering::Relaxed)})).await?;
                },
            }

        }
        Ok(())

    }

    async fn download( &self, path: &str, local: Option<&crate::filesystem::FileStore>, ) -> Result<Vec<u8>,SyncError> {

        if !self.chunk_sync {

            let result = self.call("read_file", json!({"path":path})).await?;
            let encoded = result["content_base64"].as_str().ok_or(SyncError::Peer)?;
            if encoded.len() > 1_398_104 {

                return Err(SyncError::Peer);

            }
            let bytes = STANDARD.decode(encoded).map_err(|_| SyncError::Peer)?;
            self.throttle(bytes.len()).await?;
            return Ok(bytes);

        }
        let description = self.call("file_chunks", json!({"path":path})).await?;
        let hash = description["sha256"].as_str().ok_or(SyncError::Peer)?;
        let length = description["bytes"].as_u64().ok_or(SyncError::Peer)?;
        let chunks: Vec<crate::filesystem::chunks::Chunk> =
            serde_json::from_value(description["chunks"].clone()).map_err(|_| SyncError::Peer)?;
        if length > 64 * 1024 * 1024
            || chunks.len() > 1024
            || chunks.iter().any(|chunk| chunk.bytes > 65536 || chunk.bytes == 0)
            || chunks.iter().map(|chunk| chunk.bytes as u64).sum::<u64>() != length
        {

            return Err(SyncError::Peer);

        }
        let prepared = if let Some(local) = local {

            let expected = match local.read(path) {

                Ok(bytes) => Some(crate::filesystem::digest(&bytes)),
                Err(crate::FileError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error.into()),

            };
            let value = local.prepare_transfer(path, expected.as_deref(), hash, chunks.clone())?;
            if value["already_committed"] == true {

                return Ok(local.read(path)?);

            }
            Some(value)

        } else {

            None

        };
        let mut bytes = Vec::with_capacity(length as usize);
        for (index, chunk) in chunks.iter().enumerate() {

            let missing = prepared.as_ref().is_none_or(|value| {

                value["missing"]
                    .as_array()
                    .is_some_and(|missing| missing.iter().any(|value| value.as_u64() == Some(index as u64)))

            });
            if missing {

                self.throttle(chunk.bytes).await?;
                let result =
                    self.call("read_chunk", json!({"path":path,"index":index,"expected_hash":chunk.sha256})).await?;
                let encoded = result["content_base64"].as_str().ok_or(SyncError::Peer)?;
                if encoded.len() > 87384 {

                    return Err(SyncError::Peer);

                }
                let content = STANDARD.decode(encoded).map_err(|_| SyncError::Peer)?;
                if content.len() != chunk.bytes || crate::filesystem::digest(&content) != chunk.sha256 {

                    return Err(SyncError::Peer);

                }
                if let (Some(local), Some(prepared)) = (local, prepared.as_ref()) {

                    local.put_chunk(prepared["transfer_id"].as_str().ok_or(SyncError::Peer)?, index, &content)?;

                } else {

                    bytes.extend(content);

                }

            }

        }
        if let (Some(local), Some(prepared)) = (local, prepared.as_ref()) {

            bytes = local.transfer_content(prepared["transfer_id"].as_str().ok_or(SyncError::Peer)?)?;

        }
        if crate::filesystem::digest(&bytes) != hash {

            return Err(SyncError::File(crate::FileError::Conflict));

        }
        Ok(bytes)

    }

    pub(crate) async fn identity(&mut self) -> Result<(crate::filesystem::FileSettings, String), SyncError> {

        let result =
            self.call("list_files", if self.chunk_sync { json!({"protocol_version":2}) } else { json!({}) }).await?;
        if self.chunk_sync && result["requires_sync"] != true {

            return Err(SyncError::PeerConfig("pairing 대상에는 requires_sync = true가 필요합니다"));

        }
        let mut settings: crate::filesystem::FileSettings =
            serde_json::from_value(result["settings"].clone()).map_err(|_| SyncError::Peer)?;
        let key = result["root_key"].as_str().ok_or(SyncError::Peer)?;
        if key.len() != 64 || !key.bytes().all(|byte| byte.is_ascii_hexdigit()) {

            return Err(SyncError::Peer);

        }
        self.root_key = Some(key.to_owned());
        let identity = if self.chunk_sync {

            crate::filesystem::digest(&serde_json::to_vec(&(key, &settings))?)

        } else {

            settings.max_file_bytes = 1_048_576;
            #[derive(serde::Serialize)]
            struct Legacy<'a> {

                enabled: bool,
                exclude_dirs: &'a [String],
                context_files: &'a [String],

            }
            crate::filesystem::digest(&serde_json::to_vec(&(
                key,
                Legacy {

                    enabled: settings.enabled,
                    exclude_dirs: &settings.exclude_dirs,
                    context_files: &settings.context_files,

                },
            ))?)

        };
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
        if report.status == crate::sync::Status::Synced
            && self.git_executable.is_some()
            && !self.git_handoff(session.files()).await?
        {

            session.git_conflict(
                "양쪽 Git 상태가 서로 다르게 변경되었습니다. 원본 metadata를 보존하고 build를 중단합니다",
            )?;

        }
        let report = session.report().clone();
        let mut checkpoint = json!({"generation": generation, "status": report.status,
            "manifest_hash": session.manifest_hash()?, "conflicts": report.conflicts.iter().map(|conflict| &conflict.path).collect::<Vec<_>>(),
            "lease_seconds": lease_seconds});
        if self.chunk_sync {

            checkpoint["latency_ms"] = json!(self.rpc_latency_ms.load(std::sync::atomic::Ordering::Relaxed));

        }
        let accepted = self.call("sync_checkpoint", checkpoint).await?;
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

        let result =
            self.call("list_files", if self.chunk_sync { json!({"protocol_version":2}) } else { json!({}) }).await?;
        if self.root_key.as_deref() != result["root_key"].as_str() {

            return Err(SyncError::State);

        }
        serde_json::from_value(result["files"].clone()).map_err(|_| SyncError::Peer)

    }
    async fn read(&self, path: &str) -> Result<Vec<u8>, SyncError> {

        self.download(path, None).await

    }
    async fn read_reusing( &self, path: &str, local: &crate::filesystem::FileStore, ) -> Result<Vec<u8>,SyncError> {

        self.download(path, Some(local)).await

    }
    async fn write(&self, path: &str, content: Option<&[u8]>, expected: Option<&str>) -> Result<(), SyncError> {

        if self.chunk_sync
            && let Some(content) = content
            && content.len() > 65536
        {

            let prepared=self.call("prepare_transfer",json!({"path":path,"expected_hash":expected,"sha256":crate::filesystem::digest(content),"chunks":crate::filesystem::chunks::describe(content)})).await?;
            if prepared["already_committed"] == true {

                return Ok(());

            }
            let id = prepared["transfer_id"].as_str().ok_or(SyncError::Peer)?;
            let missing = prepared["missing"].as_array().ok_or(SyncError::Peer)?;
            for index in missing {

                let index = index
                    .as_u64()
                    .filter(|index| *index < content.len().div_ceil(65536) as u64)
                    .ok_or(SyncError::Peer)? as usize;
                let chunk = &content[index * 65536..((index + 1) * 65536).min(content.len())];
                self.throttle(chunk.len()).await?;
                self.call("put_chunk", json!({"transfer_id":id,"index":index,"content_base64":STANDARD.encode(chunk)}))
                    .await?;

            }
            self.call("commit_transfer", json!({"transfer_id":id})).await?;
            return Ok(());

        }
        if content.is_some_and(|bytes| bytes.len() > 1_048_576) {

            return Err(SyncError::Peer);

        }
        self.throttle(content.map_or(0, <[u8]>::len)).await?;

        self.call(
            "write_file",
            json!({ "path": path, "content_base64": content.map(|bytes| STANDARD.encode(bytes)),
            "expected_hash": expected }),
        )
        .await?;
        Ok(())

    }

}

#[cfg(test)]
#[path = "../../tests/unit/forward_peer.rs"]
mod forward_tests;
