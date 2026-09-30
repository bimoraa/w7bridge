use crate::{
    Config, PolicyError,
    execution::{Executor, Processes},
    security::Policy,
};
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    model::{
        CallToolRequestParams, CallToolResponse, Implementation, ListToolsResult, PaginatedRequestParams,
        ServerCapabilities, ServerConfig, Tool,
    },
    service::RequestContext,
};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
/**
MCP 도구를 불변 registry와 공유 executor에 연결한다.

복사한 Bridge도 같은 실행 slot과 registry를 공유한다. Windows service 설치는 담당하지 않는다.
*/
#[derive(Clone)]
pub struct Bridge {

    pub(super) policy: Arc<Policy>,
    pub(super) codex: Arc<crate::security::codex::Discovery>,
    pub(super) executor: Arc<Executor>,
    tools: Arc<Vec<Tool>>,
    pub(super) file_slots: Arc<Semaphore>,
    pub(super) processes: Arc<Processes>,
    pub(super) coordinator: Arc<crate::sync::Coordinator>,
    pub(super) capture: Arc<crate::capture::Capture>,

}

impl Bridge {

    /**
    모든 프로젝트와 명령을 검증하고 서버 도구를 구성한다.

    # Errors

    중복 ID, 잘못된 이름·경로·명령은 PolicyError다. 하나라도 잘못되면 부분 registry를 노출하지 않는다.
    host는 종료 시 `shutdown`을 취소하고 transport future의 종료를 기다려야 한다.
    */
    pub fn new(config: Config, shutdown: CancellationToken) -> Result<Self, PolicyError> {

        let enabled = config.screenshots.enabled;
        let executor = Arc::new(Executor::new(config.execution, shutdown.clone()));
        Ok(Self {

            codex: Arc::new(crate::security::codex::Discovery::new(config.codex)),
            policy: Arc::new(Policy::new(config.projects)?),
            executor: executor.clone(),
            processes: Arc::new(Processes::new(executor)),
            tools: Arc::new(super::router::definitions(enabled)),
            file_slots: Arc::new(Semaphore::new(2)),
            coordinator: Arc::new(crate::sync::Coordinator::new()),
            capture: Arc::new(crate::capture::Capture::new(enabled, shutdown)),

        })

    }

    /** host 종료 시 공유 process 트리의 정리를 기다린다. 연결 하나의 종료에는 호출하지 않는다. */
    pub async fn shutdown(&self) {

        self.processes.shutdown().await;

    }

}

impl ServerHandler for Bridge {

    fn get_info(&self) -> ServerConfig {

        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("w7bridge", env!("CARGO_PKG_VERSION")))
            .with_instructions("list_projects로 등록된 명령을 확인한 뒤 실행하세요. 명령은 파일을 변경하거나 외부 서비스에 연결할 수 있습니다.")

    }

    fn get_tool(&self, name: &str) -> Option<Tool> {

        self.tools.iter().find(|tool| tool.name == name).cloned()

    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {

        if request.is_some_and(|request| request.cursor.is_some()) {

            return Err(ErrorData::invalid_params("이 서버는 cursor를 사용하지 않습니다", None));

        }

        Ok(ListToolsResult { tools: self.tools.as_ref().clone(), ..Default::default() })

    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {

        Ok(super::router::call(self, request, context.ct).await?.into())

    }

}
