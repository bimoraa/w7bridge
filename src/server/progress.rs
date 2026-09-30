/*! 요청에 제공한 progress token으로 출력 chunk를 전달해. 느린 client가 process pipe를 막지 않게 한도를 둬. */

use rmcp::{RoleServer, model::ProgressNotificationParam, service::RequestContext};
use serde_json::Value;
use std::{future::Future, time::Duration};
use tokio::{
    sync::mpsc,
    time::{Instant, timeout},
};

pub(super) async fn respond<F, R>(
    context: RequestContext<RoleServer>,
    operation: impl FnOnce(Option<mpsc::Sender<Value>>) -> F,
) -> R
where
    F: Future<Output = R>,
{

    let Some(token) = context.meta.get_progress_token() else {

        return operation(None).await;

    };
    let (sender, mut receiver) = mpsc::channel::<Value>(8);
    let operation = operation(Some(sender));
    tokio::pin!(operation);
    let mut sequence = 0;
    let mut active = true;
    let mut next = Instant::now();
    loop {

        tokio::select! {
            biased;
            data = receiver.recv(), if active => match data {
                Some(data) => {
                    if context.ct.is_cancelled() { continue; }
                    tokio::time::sleep_until(next).await;
                    sequence += 1;
                    let message = data["events"].as_array().map(|events| events.iter().map(|event| format!("[{}] {}", event["stream"].as_str().unwrap_or("output"), event["text"].as_str().unwrap_or(""))).collect::<String>()).filter(|text| !text.is_empty()).unwrap_or_else(|| format!("process {}: {}", data["process_id"].as_str().unwrap_or(""), data["status"].as_str().unwrap_or("")));
                    let mut notification = ProgressNotificationParam::new(token.clone(), sequence as f64).with_message(message);
                    notification.meta = Some(serde_json::Map::from_iter([("io.w7bridge/output".into(),data)]).into());
                    if !matches!(timeout(Duration::from_millis(200), context.peer.notify_progress(notification)).await, Ok(Ok(()))) {
                        receiver.close();
                        active = false;
                    }
                    next = Instant::now() + Duration::from_millis(16);
                }
                None => active = false,
            },
            result = &mut operation => return result,
        }

    }

}
