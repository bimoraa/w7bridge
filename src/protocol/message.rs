/*! project별 bounded event stream과 reconnect cursor를 소유해. */

use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

#[derive(Serialize)]
struct Event {

    cursor: u64,
    timestamp_ms: u128,
    project_id: String,
    kind: String,
    data: Value,

}

struct State {

    cursor: u64,
    bytes: usize,
    events: VecDeque<(Event, usize)>,

}

pub(crate) struct Events {

    state: Mutex<State>,
    changed: Notify,
    pub boot_id: String,

}

impl Events {

    pub fn new( ) -> Self {

        Self {

            state: Mutex::new(State { cursor: 0, bytes: 0, events: VecDeque::new() }),
            changed: Notify::new(),
            boot_id: format!(
                "{}-{}",
                std::process::id(),
                SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos()
            ),

        }

    }

    pub fn publish( &self, project: &str, kind: &str, data: Value, ) {

        if let Ok(mut state) = self.state.lock() {

            state.cursor += 1;
            let event = Event {

                cursor: state.cursor,
                timestamp_ms: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
                project_id: project.into(),
                kind: kind.into(),
                data,

            };
            let bytes = serde_json::to_vec(&event).map_or(0, |bytes| bytes.len());
            state.bytes += bytes;
            state.events.push_back((event, bytes));
            while state.events.len() > 1024 || state.bytes > 1_048_576 {

                if let Some((_, bytes)) = state.events.pop_front() {

                    state.bytes -= bytes;

                }

            }

        }
        self.changed.notify_waiters();

    }

    pub async fn read( &self, project: &str, cursor: u64, seconds: u64, cancellation: CancellationToken, ) -> Result<Value, String> {

        if seconds > 30 {

            return Err("event 대기 한도는 0..=30초입니다".into());

        }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
        loop {

            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if cancellation.is_cancelled() {

                return Err("event 대기가 취소되었습니다".into());

            }
            let response = {

                let state = self.state.lock().map_err(|_| "event 상태를 읽을 수 없습니다")?;
                if cursor > state.cursor {

                    return Err("event cursor가 현재 상태보다 앞에 있습니다. boot_id를 확인하세요".into());

                }
                let first = state.events.front().map_or(state.cursor + 1, |(event, _)| event.cursor);
                json!({"boot_id":self.boot_id,"next_cursor":state.cursor,"truncated":cursor.saturating_add(1)<first,
                    "events":state.events.iter().filter(|(event, _)| event.project_id==project && event.cursor>cursor).map(|(event, _)| event).collect::<Vec<_>>()})

            };
            if seconds == 0
                || response["next_cursor"].as_u64() != Some(cursor)
                || tokio::time::Instant::now() >= deadline
            {

                return Ok(response);

            }
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Err("event 대기가 취소되었습니다".into()),
                _ = tokio::time::sleep_until(deadline) => {},
                _ = changed => {},
            }

        }

    }

}

#[cfg(test)]
#[path = "../../tests/unit/events.rs"]
mod tests;
