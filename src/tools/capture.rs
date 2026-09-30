/*! screenshot은 source 파일 공유와 독립된 명시적 capability야. client는 경로와 실행 파일을 지정할 수 없어. */

use crate::{capture::Capture, error::CaptureError};
use base64::{Engine, engine::general_purpose::STANDARD};
use rmcp::{
    ErrorData,
    model::{CallToolResult, ContentBlock, Tool, ToolAnnotations},
};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {

    display: Option<u32>,

}

pub(crate) fn definition() -> Tool {

    let schema = json!({ "type": "object", "additionalProperties": false,
        "properties": { "display": { "type": "integer", "minimum": 1, "maximum": 16,
            "description": "1은 main display, 나머지는 OS의 다른 display (기본: 1)" } } });
    Tool::new(
        "capture_screenshot",
        "서버 소유자가 허용한 현재 machine의 screenshot을 PNG로 반환합니다. 화면 녹화나 지속 capture는 하지 않습니다.",
        schema.as_object().cloned().unwrap_or_default(),
    )
    .with_annotations(ToolAnnotations::new().read_only(true).destructive(false).idempotent(false).open_world(false))

}

pub(crate) async fn call( capture: &Capture, arguments: Map<String, Value>, cancellation: CancellationToken, ) -> Result<CallToolResult, ErrorData> {

    let arguments: Arguments = serde_json::from_value(Value::Object(arguments))
        .map_err(|_| ErrorData::invalid_params("screenshot 인자를 확인하세요", None))?;
    let display = arguments.display.unwrap_or(1);
    if !(1..=16).contains(&display) {

        return Err(ErrorData::invalid_params("display는 1..=16이어야 합니다", None));

    }
    Ok(match capture.take(display, cancellation).await {

        Ok(image) => {

            let mut result =
                CallToolResult::success(vec![ContentBlock::image(STANDARD.encode(image.bytes), "image/png")]);
            result.structured_content = Some(json!({ "display": display, "width": image.width, "height": image.height,
                "machine_os": std::env::consts::OS }));
            result

        }
        Err(error) => {

            let code = match error {

                CaptureError::Disabled => "disabled",
                CaptureError::Busy => "busy",
                CaptureError::Cancelled => "cancelled",
                CaptureError::Timeout => "timeout",
                CaptureError::Unavailable => "unavailable",
                #[cfg(not(any(target_os = "macos", windows)))]
                CaptureError::Unsupported => "unsupported",
                CaptureError::Limit => "limit",
                CaptureError::Data => "data",
                CaptureError::Cleanup { .. } => "cleanup",

            };
            CallToolResult::structured_error(json!({ "code": code, "message": error.to_string() }))

        }

    })

}
