/*! 로컬 MCP는 offline peer 상태를 유지하고 선택한 device/project에 도구를 전달해. */

use crate::{
    config::{Pair, SyncSettings},
    connection::session::Remote,
    filesystem::{FileSettings, FileStore, digest},
    protocol::types::Failure,
    sync::Session,
};
use rmcp::{
    ErrorData, RoleServer, ServerHandler, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, Implementation, ListToolsResult,
        PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
    },
    service::RequestContext,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

struct Peer {

    pair: Pair,
    remote: Mutex<Option<Arc<Remote>>>,
    status: Mutex<Value>,

}

#[derive(Clone)]
struct Hub {

    peers: Arc<BTreeMap<String, Arc<Peer>>>,
    tools: Arc<Vec<Tool>>,
    shutdown: CancellationToken,

}

pub(crate) async fn run( settings: SyncSettings, ) -> Result<(),Failure> {

    let shutdown = CancellationToken::new();
    let mut peers = BTreeMap::new();
    let mut targets = BTreeSet::new();
    let mut roots = BTreeSet::new();
    for pair in &settings.pairs {

        let target = pair.binding().or_else(|_| {

            std::fs::create_dir_all(&pair.local_root)?;
            pair.binding()

        })?;
        if !targets.insert(serde_json::to_string(&(pair.options()?.ssh_args(), &pair.remote_project))?) {

            return Err("hub 대상이 중복되었습니다".into());

        }
        let root = pair.local_root.canonicalize()?;
        if roots
            .iter()
            .any(|other: &std::path::PathBuf| root != *other && (root.starts_with(other) || other.starts_with(&root)))
        {

            return Err("hub project root는 서로 포함할 수 없습니다".into());

        }
        roots.insert(root);
        let device =
            pair.expected_device_id.clone().unwrap_or_else(|| format!("device-{}", &digest(pair.host.as_bytes())[..8]));
        let id = format!("{device}__{}", pair.remote_project);
        if peers.insert(id.clone(),Arc::new(Peer {pair:pair.clone(),remote:Mutex::new(None),status:Mutex::new(json!({"device_online":false,"connection":"not_connected","binding_hash":digest(target.as_bytes())}))})).is_some(){return Err("hub의 device/project ID가 중복되었습니다".into())}

    }
    let mut tools = super::router::definitions(settings.pairs.iter().any(|pair| pair.screenshot_config.is_some()));
    tools.retain(|tool| tool.name != "sync_checkpoint" && tool.name != "bridge_info");
    for tool in &mut tools {

        if let Some(Value::Object(properties)) = Arc::make_mut(&mut tool.input_schema).get_mut("properties") {

            properties.remove("sync_peer");

        }

        if tool.name == "capture_screenshot" {

            let schema = Arc::make_mut(&mut tool.input_schema);
            if let Some(Value::Object(properties)) = schema.get_mut("properties") {

                properties.insert("project_id".into(), json!({"type":"string"}));

            }
            if let Some(Value::Array(required)) = schema.get_mut("required") {

                required.push(json!("project_id"));

            } else {

                schema.insert("required".into(), json!(["project_id"]));

            }

        }

    }
    let hub = Hub { peers: Arc::new(peers), tools: Arc::new(tools), shutdown: shutdown.clone() };
    let mut tasks = tokio::task::JoinSet::new();
    for peer in hub.peers.values() {

        let peer = peer.clone();
        let token = shutdown.clone();
        let interval = settings.interval_seconds;
        tasks.spawn(async move {

            loop {

                let result =
                    crate::filesystem::watcher::watch(peer.pair.clone(), interval, false, false, token.clone()).await;
                if token.is_cancelled() {

                    break;

                }
                if let Err(error) = result {

                    let mut status = peer.status.lock().await;
                    status["sync_host_error"] = json!(error.to_string());

                }
                tokio::select! {_=token.cancelled()=>break,_=tokio::time::sleep(Duration::from_secs(5))=>{}}

            }

        });

    }
    let serving = hub.serve((tokio::io::stdin(), tokio::io::stdout())).await;
    let result = match serving {

        Ok(service) => {

            let cancellation = service.cancellation_token();
            let waiting = service.waiting();
            tokio::pin!(waiting);
            tokio::select! {result=&mut waiting=>result.map(|_|()).map_err(Into::into),_=tokio::signal::ctrl_c()=>{cancellation.cancel();waiting.await.map(|_|()).map_err(Into::into)}}

        }
        Err(error) => Err(error.into()),

    };
    shutdown.cancel();
    while tasks.join_next().await.is_some() {}
    result

}

impl Hub {

    async fn connect( &self, peer: &Peer, cancellation: CancellationToken, ) -> Result<Arc<Remote>,String> {

        let mut remote = tokio::select! {value=peer.remote.lock()=>value,_=cancellation.cancelled()=>return Err("hub 연결 요청이 취소되었습니다".into())};
        if let Some(remote) = remote.as_ref() {

            return Ok(remote.clone());

        }
        let result = tokio::time::timeout(Duration::from_secs(10), Remote::connect(&peer.pair, cancellation)).await;
        match result {

            Ok(Ok(connection)) => {

                let connection = Arc::new(connection);
                *remote = Some(connection.clone());
                let mut status = peer.status.lock().await;
                status["device_online"] = json!(true);
                status["connection"] = json!("connected");
                status["last_error"] = Value::Null;
                Ok(connection)

            }
            _ => {

                let mut status = peer.status.lock().await;
                status["device_online"] = json!(false);
                status["connection"] = json!("offline");
                status["last_error"] = json!("SSH 연결, 인증 또는 protocol 확인을 완료할 수 없습니다");
                Err("device가 offline이거나 SSH pairing/protocol이 맞지 않습니다".into())

            }

        }

    }

    async fn status( &self, id: &str, peer: &Peer, cancellation: CancellationToken, ) -> Value {

        let response = match self.connect(peer, cancellation.clone()).await {

            Ok(remote) => match tokio::time::timeout(
                Duration::from_secs(5),
                remote.forward("project_status", serde_json::Map::new(), cancellation),
            )
            .await
            {

                Ok(Ok(result)) if result.is_error != Some(true) => {

                    let mut value = result.structured_content.unwrap_or_else(|| json!({"device_online":true}));
                    value["project_id"] = json!(id);
                    value["remote_project_id"] = json!(peer.pair.remote_project);
                    value["local_root"] = json!(peer.pair.local_root);
                    *peer.status.lock().await = value.clone();
                    return value;

                }
                _ => {

                    *peer.remote.lock().await = None;
                    false

                }

            },
            Err(_) => false,

        };
        let mut value = peer.status.lock().await.clone();
        value["device_online"] = json!(response);
        value["project_id"] = json!(id);
        value["remote_project_id"] = json!(peer.pair.remote_project);
        value["local_root"] = json!(peer.pair.local_root);
        if let Ok(files) = FileStore::new(&peer.pair.local_root, FileSettings { enabled: true, ..Default::default() })
            && let Ok(binding) = peer.pair.binding()
            && let Ok(session) = Session::open_pair(files, binding)
        {

            value["last_local_sync"] = json!(session.report());

        }
        value["cached_remote_state"] = json!(true);
        value

    }

}

impl ServerHandler for Hub {

    fn get_info( &self, ) -> ServerConfig {

        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("w7bridge", env!("CARGO_PKG_VERSION")))

    }

    async fn list_tools( &self, request: Option<PaginatedRequestParams>, _context: RequestContext<RoleServer>, ) -> Result<ListToolsResult,ErrorData> {

        if request.is_some_and(|request| request.cursor.is_some()) {

            return Err(ErrorData::invalid_params("hub tool cursor는 지원하지 않습니다", None));

        }
        Ok(ListToolsResult { tools: self.tools.as_ref().clone(), ..Default::default() })

    }

    async fn call_tool( &self, request: CallToolRequestParams, context: RequestContext<RoleServer>, ) -> Result<CallToolResponse,ErrorData> {

        let mut arguments = request.arguments.unwrap_or_default();
        if arguments.contains_key("sync_peer") {

            return Err(ErrorData::invalid_params("hub가 pairing peer ID를 선택합니다", None));

        }
        if request.name == "list_projects" {

            if !arguments.is_empty() {

                return Err(ErrorData::invalid_params("list_projects에는 인자가 없습니다", None));

            }
            let mut projects = Vec::new();
            let mut tasks = tokio::task::JoinSet::new();
            for (id, peer) in self.peers.iter() {

                let hub = self.clone();
                let id = id.clone();
                let peer = peer.clone();
                let cancellation = context.ct.clone();
                tasks.spawn(async move {

                    let status = hub.status(&id,&peer,cancellation).await;
                    json!({"id":id,"device_id":status.get("device_id").cloned().or_else(||peer.pair.expected_device_id.as_ref().map(|id|json!(id))),"root":status["root"],"local_root":peer.pair.local_root,"remote_project_id":peer.pair.remote_project,"commands":status["commands"],"status":status})

                });

            }
            while let Some(result) = tasks.join_next().await {

                projects
                    .push(result.map_err(|_| ErrorData::internal_error("hub 상태 작업을 완료할 수 없습니다", None))?);

            }
            projects.sort_by(|left, right| left["id"].as_str().cmp(&right["id"].as_str()));
            return Ok(CallToolResult::structured(json!({"projects":projects,"hub_os":std::env::consts::OS})).into());

        }
        if !self.tools.iter().any(|tool| tool.name == request.name) {

            return Err(ErrorData::invalid_params("등록되지 않은 hub 도구입니다", None));

        }
        let id = arguments
            .remove("project_id")
            .and_then(|id| id.as_str().map(str::to_owned))
            .ok_or_else(|| ErrorData::invalid_params("hub의 device/project ID가 필요합니다", None))?;
        let peer =
            self.peers.get(&id).ok_or_else(|| ErrorData::invalid_params("등록되지 않은 hub project입니다", None))?;
        if request.name == "project_status" {

            if !arguments.is_empty() {

                return Err(ErrorData::invalid_params("project_status 추가 인자는 없습니다", None));

            }
            return Ok(CallToolResult::structured(self.status(&id, peer, context.ct).await).into());

        }
        if matches!(request.name.as_ref(), "start_process" | "run_command" | "restart_process") && !peer.pair.service {

            return Ok(CallToolResult::structured_error(
                json!({"message":"reconnect 후 process handle을 유지하려면 Windows service pairing이 필요합니다"}),
            )
            .into());

        }
        if request.name == "capture_screenshot" {

            let Some(config) = &peer.pair.screenshot_config else {

                return Ok(CallToolResult::structured_error(
                    json!({"message":"이 project의 screenshot_config를 owner가 먼저 승인해야 합니다"}),
                )
                .into());

            };
            let mut pair = peer.pair.clone();
            pair.service = false;
            pair.config = Some(config.clone());
            pair.git_executable = None;
            pair.bandwidth_bytes_per_second = 0;
            let remote = match Remote::connect(&pair, context.ct.clone()).await {

                Ok(remote) => remote,
                Err(error) => return Ok(CallToolResult::structured_error(json!({"message":error.to_string()})).into()),

            };
            let result = remote.forward("capture_screenshot", arguments, context.ct).await;
            remote.close().await;
            return Ok(match result {

                Ok(result) => result.into(),
                Err(error) => CallToolResult::structured_error(json!({"message":error.to_string()})).into(),

            });

        }
        if self.shutdown.is_cancelled() {

            return Ok(CallToolResult::structured_error(json!({"message":"hub가 종료 중입니다"})).into());

        }
        let remote = match self.connect(peer, context.ct.clone()).await {

            Ok(remote) => remote,
            Err(error) => {

                return Ok(CallToolResult::structured_error(json!({"device_online":false,"message":error})).into());

            }

        };
        let cancellation = context.ct.clone();
        match super::progress::respond(context, |output| {

            remote.forward_with_output(&request.name, arguments, cancellation, output, Some(&id))

        })
        .await
        {

            Ok(result) => Ok(result.into()),
            Err(error) => {

                let mut cached = peer.remote.lock().await;
                if cached.as_ref().is_some_and(|current| Arc::ptr_eq(current, &remote)) {

                    *cached = None;

                }
                let mut status = peer.status.lock().await;
                status["device_online"] = json!(false);
                status["last_error"] = json!(error.to_string());
                Ok(CallToolResult::structured_error(json!({"code":"unknown_outcome","device_online":false,"message":"연결이 끊겼습니다. 명령을 자동 재실행하지 않습니다. reconnect 후 process/history로 결과를 확인하세요"})).into())

            }

        }

    }

}
