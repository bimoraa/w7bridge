use std::{
    io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use rmcp::{
    ErrorData, RoleServer, ServerHandler, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResponse, Implementation, ListToolsResult, PaginatedRequestParams,
        ServerCapabilities, ServerConfig, Tool,
    },
    service::RequestContext,
};
use tokio::io::{AsyncRead, ReadBuf, Stdin};
use tokio_util::sync::CancellationToken;

use crate::{Config, PolicyError, execution::Executor, security::Policy, tools};

/**
MCP 도구를 불변 registry와 공유 executor에 연결한다.

복사한 Bridge도 같은 실행 slot과 registry를 공유한다. Windows service 설치는 담당하지 않는다.
*/
#[derive(Clone)]
pub struct Bridge {

    policy: Arc<Policy>,
    executor: Arc<Executor>,
    tools: Arc<Vec<Tool>>,

}

impl Bridge {

    /**
    모든 프로젝트와 명령을 검증하고 서버 도구를 구성한다.

    # Errors

    중복 ID, 잘못된 이름·경로·명령은 PolicyError다. 하나라도 잘못되면 부분 registry를 노출하지 않는다.
    host는 종료 시 `shutdown`을 취소하고 transport future의 종료를 기다려야 한다.
    */
    pub fn new(config: Config, shutdown: CancellationToken) -> Result<Self, PolicyError> {

        Ok(Self {

            policy: Arc::new(Policy::new(config.projects)?),
            executor: Arc::new(Executor::new(config.execution, shutdown)),
            tools: Arc::new(tools::definitions()),

        })

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

        Ok(tools::call(&self.policy, &self.executor, request, context.ct).await?.into())

    }

}

/**
stdio로 MCP 연결을 처리한다. stdout에는 protocol 메시지만 쓴다.

연결 종료 시 `shutdown`을 취소한다. host가 token을 취소하면 SDK 연결도 종료하고 종료 완료를 기다린다.
host는 이 future를 먼저 drop하지 말고 token 취소 후 완료를 기다려야 한다.

# Errors

초기화와 연결 처리 실패는 한국어 오류를 반환한다. 실제 자식 실행 실패는 MCP 도구 결과로 전달한다.
*/
pub async fn serve_stdio(bridge: Bridge, shutdown: CancellationToken) -> Result<(), &'static str> {

    let input = ShutdownInput { stdin: tokio::io::stdin(), shutdown: shutdown.clone() };
    let service = tokio::select! {
        biased;
        _ = shutdown.cancelled() => return Ok(()),
        result = bridge.serve((input, tokio::io::stdout())) => result.map_err(|_| "MCP 초기화에 실패했습니다")?,
    };
    let cancellation = service.cancellation_token();
    let waiting = service.waiting();
    tokio::pin!(waiting);

    let result = tokio::select! {
        result = &mut waiting => result,
        _ = shutdown.cancelled() => {
            cancellation.cancel();
            waiting.await
        }
    };

    shutdown.cancel();
    result.map(|_| ()).map_err(|_| "MCP 연결 처리에 실패했습니다")

}

struct ShutdownInput {

    stdin: Stdin,
    shutdown: CancellationToken,

}

impl AsyncRead for ShutdownInput {

    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {

        let before = buffer.filled().len();
        let has_capacity = buffer.remaining() > 0;
        let result = Pin::new(&mut self.stdin).poll_read(context, buffer);

        // SDK의 요청 대기보다 먼저 종료를 알려서 연결이 끊긴 작업을 중단해.
        if matches!(&result, Poll::Ready(Err(_)))
            || matches!(&result, Poll::Ready(Ok(()))) && has_capacity && buffer.filled().len() == before
        {

            self.shutdown.cancel();

        }

        result

    }

}
