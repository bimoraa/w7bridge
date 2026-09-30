/*! MCP 초기화, 필수 도구와 조회 응답을 확인해. */

use crate::ConnectError;
use rmcp::{ServiceExt, model::CallToolRequestParams};
use tokio::io::{AsyncRead, AsyncWrite};
pub(super) async fn check_transport(
    input: impl AsyncRead + Send + Unpin + 'static,
    output: impl AsyncWrite + Send + Unpin + 'static,
) -> Result<(), ConnectError> {

    let client = ().serve((input, output)).await.map_err(|_| ConnectError::Protocol)?;
    let result = async {

        let info = client.peer_info().ok_or(ConnectError::Server)?;
        if !info.server_info.as_ref().is_some_and(|server| server.name == "w7bridge")
            || info.capabilities.tools.is_none()
        {

            return Err(ConnectError::Server);

        }
        let tools = client.list_tools(None).await.map_err(|_| ConnectError::Protocol)?;
        if tools.next_cursor.is_some()
            || !["list_projects", "run_command"].iter().all(|name| tools.tools.iter().any(|tool| tool.name == *name))
        {

            return Err(ConnectError::Server);

        }
        let listing = client
            .call_tool(CallToolRequestParams::new("list_projects").with_arguments(Default::default()))
            .await
            .map_err(|_| ConnectError::Protocol)?;
        if listing.is_error == Some(true)
            || !listing
                .structured_content
                .as_ref()
                .and_then(|value| value.get("projects"))
                .is_some_and(|projects| projects.is_array())
        {

            return Err(ConnectError::Server);

        }
        Ok(())

    }
    .await;
    client.cancel().await.map_err(|_| ConnectError::Protocol)?;
    result

}
