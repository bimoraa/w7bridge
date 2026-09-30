/*! 명령별 process handle과 bounded live output을 공유해. PID를 client 입력으로 받지 않아. */

use super::{Executor, failure};
use crate::error::ExecutionError;
use crate::protocol::response::Output;
use crate::{config::CommandDefinition, security::Policy};
use process_wrap::tokio::ChildWrapper;
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Notify, oneshot};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Serialize)]
struct Event {

    cursor: u64,
    stream: &'static str,
    content_base64: String,
    text: String,

}

struct Log {

    events: VecDeque<Event>,
    bytes: usize,
    cursor: u64,
    result: Option<Value>,

}

#[derive(Clone)]
pub(crate) struct Live {

    log: Arc<Mutex<Log>>,

}

impl Live {

    pub fn append(&self, stream: &'static str, bytes: &[u8]) {

        use base64::{Engine, engine::general_purpose::STANDARD};
        if let Ok(mut log) = self.log.lock() {

            log.cursor += 1;
            let event = Event {

                cursor: log.cursor,
                stream,
                content_base64: STANDARD.encode(bytes),
                text: String::from_utf8_lossy(bytes).into_owned(),

            };
            log.bytes += event.content_base64.len() + event.text.len();
            log.events.push_back(event);
            while log.bytes > 131_072 || log.events.len() > 256 {

                if let Some(event) = log.events.pop_front() {

                    log.bytes -= event.content_base64.len() + event.text.len();

                }

            }

        }

    }
    fn finish(&self, result: Result<Output, super::ExecutionError>) {

        if let Ok(mut log) = self.log.lock() {

            log.result = Some(match result {

                Ok(output) => {

                    serde_json::to_value(output).unwrap_or_else(|_| json!({"error": "출력을 변환할 수 없습니다"}))

                }
                Err(error) => json!({ "error": error.to_string() }),

            });

        }

    }
    fn completed(&self) -> bool {

        self.log.lock().is_ok_and(|log| log.result.is_some())

    }

}

struct Process {

    project: String,
    command: String,
    cancellation: CancellationToken,
    done: Arc<Notify>,
    live: Live,

}

pub(crate) struct Processes {

    entries: Mutex<BTreeMap<String, Arc<Process>>>,
    executor: Arc<Executor>,
    next_id: AtomicU64,

}

impl Processes {

    pub fn new(executor: Arc<Executor>) -> Self {

        Self { entries: Mutex::new(BTreeMap::new()), executor, next_id: AtomicU64::new(1) }

    }

    pub async fn start(&self, policy: &Policy, project: &str, command: &str) -> Result<Value, String> {

        let (root, definition) = policy.resolve(project, command).map_err(|error| error.to_string())?;
        let root = root.to_owned();
        let definition: CommandDefinition = definition.clone();
        let id =
            SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| "시계를 읽을 수 없습니다")?.as_nanos().to_string()
                + "-"
                + &self.next_id.fetch_add(1, Ordering::Relaxed).to_string();
        let process = Arc::new(Process {

            project: project.into(),
            command: command.into(),
            cancellation: CancellationToken::new(),
            done: Arc::new(Notify::new()),
            live: Live {

                log: Arc::new(Mutex::new(Log { events: VecDeque::new(), bytes: 0, cursor: 0, result: None })),

            },

        });
        {

            let mut entries = self.entries.lock().map_err(|_| "process 상태를 읽을 수 없습니다")?;
            if entries
                .values()
                .any(|entry| entry.project == project && entry.command == command && !entry.live.completed())
            {

                return Err("같은 project 명령이 이미 실행 중입니다".into());

            }
            while entries.len() >= 32 {

                let key = entries.iter().find(|(_, entry)| entry.live.completed()).map(|(key, _)| key.clone());
                if let Some(key) = key {

                    entries.remove(&key);

                } else {

                    return Err("process 보관 한도를 초과했습니다".into());

                }

            }
            entries.insert(id.clone(), process.clone());

        }
        let executor = self.executor.clone();
        let task_process = process.clone();
        let (started, ready) = oneshot::channel();
        tokio::spawn(async move {

            let result = executor
                .run_observed(
                    &root,
                    &definition,
                    task_process.cancellation.clone(),
                    Some(task_process.live.clone()),
                    Some(started),
                )
                .await;
            task_process.live.finish(result);
            task_process.done.notify_waiters();

        });
        let done = process.done.notified();
        tokio::pin!(done);
        done.as_mut().enable();
        match ready.await {

            Ok(Ok(())) => Ok(json!({ "process_id": id, "status": "running" })),
            _ => {

                if !process.live.completed() {

                    let _ = tokio::time::timeout(Duration::from_secs(6), done).await;

                }
                let output = self.read(project, &id, 0)?;
                Err(output["result"]["error"].as_str().unwrap_or("프로세스를 시작할 수 없습니다").into())

            }

        }

    }

    pub fn list(&self, project: &str) -> Result<Value, String> {

        let entries = self.entries.lock().map_err(|_| "process 상태를 읽을 수 없습니다")?;
        Ok(json!({"processes": entries.iter().filter(|(_, process)| process.project == project).map(|(id, process)|
            json!({"process_id": id, "command": process.command, "status": if process.live.completed() { "stopped" } else { "running" }})).collect::<Vec<_>>()}))

    }

    fn get(&self, project: &str, id: &str) -> Result<Arc<Process>, String> {

        self.entries
            .lock()
            .map_err(|_| "process 상태를 읽을 수 없습니다")?
            .get(id)
            .filter(|entry| entry.project == project)
            .cloned()
            .ok_or_else(|| "프로젝트의 process handle이 없습니다".into())

    }

    pub fn read(&self, project: &str, id: &str, cursor: u64) -> Result<Value, String> {

        let process = self.get(project, id)?;
        let log = process.live.log.lock().map_err(|_| "process 출력을 읽을 수 없습니다")?;
        if cursor > log.cursor {

            return Err("cursor가 현재 process 출력보다 앞에 있습니다".into());

        }
        let first = log.events.front().map_or(log.cursor + 1, |event| event.cursor);
        Ok(
            json!({ "process_id": id, "command": process.command, "status": if log.result.is_some() { "stopped" } else { "running" },
            "next_cursor": log.cursor, "truncated": cursor + 1 < first,
            "events": log.events.iter().filter(|event| event.cursor > cursor).collect::<Vec<_>>(), "result": log.result }),
        )

    }

    pub async fn stop(&self, project: &str, id: &str) -> Result<Value, String> {

        let process = self.get(project, id)?;
        let done = process.done.notified();
        tokio::pin!(done);
        done.as_mut().enable();
        if !process.live.completed() {

            process.cancellation.cancel();
            tokio::time::timeout(Duration::from_secs(6), done)
                .await
                .map_err(|_| "프로세스 종료 확인 시간이 초과되었습니다")?;

        }
        self.read(project, id, 0)

    }

    pub async fn restart(&self, policy: &Policy, project: &str, id: &str) -> Result<Value, String> {

        let process = self.get(project, id)?;
        self.stop(project, id).await?;
        self.start(policy, project, &process.command).await

    }

    pub async fn shutdown(&self) {

        let entries =
            self.entries.lock().map(|entries| entries.values().cloned().collect::<Vec<_>>()).unwrap_or_default();
        for process in entries {

            let done = process.done.notified();
            tokio::pin!(done);
            done.as_mut().enable();
            if !process.live.completed() {

                process.cancellation.cancel();
                if tokio::time::timeout(Duration::from_secs(6), done).await.is_err() {

                    eprintln!("process 종료 확인 시간이 초과되었습니다");

                }

            }

        }

    }

}

pub(super) struct ChildGuard {

    pub child: Box<dyn ChildWrapper>,
    pub active: bool,

}

impl Drop for ChildGuard {

    fn drop(&mut self) {

        if self.active {

            // 취소된 future에서도 트리를 종료해. 정상 경로의 오류는 덮어쓰지 않아.
            let _ = self.child.start_kill();

        }

    }

}

pub(super) async fn stop(child: &mut dyn ChildWrapper) -> Result<(), ExecutionError> {

    child.start_kill().map_err(|source| failure("트리 종료", source))?;
    timeout(Duration::from_secs(5), child.wait())
        .await
        .map_err(|_| ExecutionError::CleanupTimeout)?
        .map_err(|source| failure("종료 확인", source))?;
    Ok(())

}
