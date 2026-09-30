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
    changed: Arc<Notify>,

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
        self.changed.notify_waiters();

    }
    fn finish( &self, result: Result<Output, super::ExecutionError>, revision: Option<&str>, final_revision: Option<&str>, ) {

        if let Ok(mut log) = self.log.lock() {

            log.result = Some(match result {

                Ok(output) => {

                    serde_json::to_value(output).unwrap_or_else(|_| json!({"error": "출력을 변환할 수 없습니다"}))

                }
                Err(error) => json!({ "error": error.to_string() }),

            });
            if let Some(result) = log.result.as_mut() {

                result["revision_at_start"] = json!(revision);
                result["revision_at_completion"] = json!(final_revision);
                result["revision_matches_at_completion"] =
                    json!(revision.zip(final_revision).map(|(start, end)| start == end));

            }

        }
        self.changed.notify_waiters();

    }
    fn completed(&self) -> bool {

        self.log.lock().is_ok_and(|log| log.result.is_some())

    }

}

struct Process {

    project: String,
    command: String,
    revision: Option<String>,
    cancellation: CancellationToken,
    done: Arc<Notify>,
    live: Live,
    snapshot: Option<tempfile::TempDir>,

}

pub(crate) struct Processes {

    entries: Mutex<BTreeMap<String, Arc<Process>>>,
    executor: Arc<Executor>,
    next_id: AtomicU64,
    events: Arc<crate::protocol::message::Events>,

}

/** 요청의 응답 대기 한도와 선택적인 출력 stream을 함께 소유한다. */
pub(crate) struct RunOptions {

    pub yield_time: Option<Duration>,
    pub output: Option<tokio::sync::mpsc::Sender<Value>>,

}

impl Processes {

    pub fn new( executor: Arc<Executor>, events: Arc<crate::protocol::message::Events>, ) -> Self {

        Self { entries: Mutex::new(BTreeMap::new()), executor, next_id: AtomicU64::new(1), events }

    }

    pub async fn start(
        &self,
        policy: &Policy,
        project: &str,
        command: &str,
        expected_revision: Option<&str>,
    ) -> Result<Value, String> {

        self.spawn(policy, project, command, true, CancellationToken::new(), expected_revision).await

    }

    async fn spawn( &self, policy: &Policy, project: &str, command: &str, detached: bool, cancellation: CancellationToken, expected_revision: Option<&str>, ) -> Result<Value, String> {

        let (root, definition) = policy.resolve(project, command).map_err(|error| error.to_string())?;
        let mut root = root.to_owned();
        let mut definition: CommandDefinition = definition.clone();
        if !detached {

            definition.background = false;

        }
        let mut files = match policy.files(project) {

            Ok(files) => Some(files),
            Err(crate::FileError::Disabled) => None,
            Err(error) => return Err(error.to_string()),

        };
        let snapshot = if definition.source_snapshot {

            let source = files.clone().ok_or("source snapshot에는 파일 권한이 필요합니다")?;
            let (directory, copied, _) = tokio::task::spawn_blocking(move || source.snapshot())
                .await
                .map_err(|_| "source snapshot 작업을 완료할 수 없습니다")?
                .map_err(|error| error.to_string())?;
            root = directory.path().to_owned();
            files = Some(copied);
            Some(directory)

        } else {

            None

        };
        let revision = files
            .as_ref()
            .map(crate::sync::state::coordination::manifest)
            .transpose()
            .map_err(|error| error.to_string())?;
        if expected_revision.is_some_and(|expected| revision.as_deref() != Some(expected)) {

            return Err("sync 확인 뒤 source revision이 변경되었습니다. 다시 sync한 뒤 명령을 실행하세요".into());

        }
        let id =
            SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| "시계를 읽을 수 없습니다")?.as_nanos().to_string()
                + "-"
                + &self.next_id.fetch_add(1, Ordering::Relaxed).to_string();
        let process = Arc::new(Process {

            project: project.into(),
            command: command.into(),
            revision: revision.clone(),
            snapshot,
            cancellation,
            done: Arc::new(Notify::new()),
            live: Live {

                log: Arc::new(Mutex::new(Log { events: VecDeque::new(), bytes: 0, cursor: 0, result: None })),
                changed: Arc::new(Notify::new()),

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
        let events = self.events.clone();
        let task_id = id.clone();
        let (started, ready) = oneshot::channel();
        let (announced, announcement) = oneshot::channel();
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
            let mut kind = match &result {

                Ok(output) if output.success => "command_completed",
                Ok(output) if matches!(output.status, crate::protocol::response::Status::Cancelled) => {

                    "command_cancelled"

                }
                Ok(output) if matches!(output.status, crate::protocol::response::Status::TimedOut) => {

                    "command_timed_out"

                }
                _ => "process_crashed",

            };
            let exit_code = result.as_ref().ok().and_then(|output| output.exit_code);
            let error = result.as_ref().err().map(ToString::to_string);
            let final_revision =
                files.as_ref().and_then(|files| crate::sync::state::coordination::manifest(files).ok());
            if task_process.revision.is_some() && task_process.revision != final_revision {

                kind = "verification_failed";

            }
            task_process.live.finish(result, task_process.revision.as_deref(), final_revision.as_deref());
            if let Ok(mut log) = task_process.live.log.lock()
                && let Some(result) = log.result.as_mut()
            {

                result["source_snapshot"] = json!(task_process.snapshot.is_some());
                result["artifact_root"] = json!(task_process.snapshot.as_ref().map(|snapshot| snapshot.path()));
                result["revision_verified"] =
                    json!(task_process.snapshot.is_some() && task_process.revision == final_revision);
                if task_process.revision.is_some() && task_process.revision != final_revision {

                    result["success"] = json!(false);
                    result["verification_error"] = json!("명령 실행 중 source revision이 변경되었습니다");

                }

            }
            task_process.done.notify_waiters();
            let _ = announcement.await;
            events.publish(
                &task_process.project,
                kind,
                json!({"process_id":task_id,"command":task_process.command,
                "revision_at_start":task_process.revision,"revision_at_completion":final_revision,"revision_verified":task_process.snapshot.is_some() && task_process.revision==final_revision,"exit_code":exit_code,"error":error}),
            );

        });
        let done = process.done.notified();
        tokio::pin!(done);
        done.as_mut().enable();
        let ready = ready.await;
        if matches!(ready, Ok(Ok(()))) {

            self.events.publish(
                project,
                "process_started",
                json!({"process_id":id,"command":command,"revision_at_start":revision}),
            );

        }
        let _ = announced.send(());
        match ready {

            Ok(Ok(())) => Ok(json!({ "process_id": id, "status": "running", "revision_at_start":revision })),
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
            json!({"process_id": id, "command": process.command, "revision_at_start":process.revision, "source_snapshot":process.snapshot.is_some(), "artifact_root":process.snapshot.as_ref().map(|snapshot|snapshot.path()), "status": if process.live.completed() { "stopped" } else { "running" }})).collect::<Vec<_>>()}))

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
            json!({ "process_id": id, "command": process.command, "revision_at_start":process.revision, "status": if log.result.is_some() { "stopped" } else { "running" },
            "next_cursor": log.cursor, "truncated": cursor + 1 < first,
            "events": log.events.iter().filter(|event| event.cursor > cursor).collect::<Vec<_>>(), "result": log.result }),
        )

    }

    /** 새 출력이나 종료까지 기다린다. timeout은 빈 event 응답이며 취소는 오류다. */
    pub async fn read_wait( &self, project: &str, id: &str, cursor: u64, seconds: u64, cancellation: CancellationToken, ) -> Result<Value, String> {

        if seconds > 30 {

            return Err("출력 대기 한도는 0..=30초입니다".into());

        }
        let process = self.get(project, id)?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
        loop {

            // 확인 전에 waiter를 등록해서 출력 도착과 대기 사이의 event를 잃지 않아.
            let changed = process.live.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if cancellation.is_cancelled() {

                return Err("출력 대기가 취소되었습니다".into());

            }
            let output = self.read(project, id, cursor)?;
            if seconds == 0 || output["next_cursor"].as_u64() != Some(cursor) || output["status"] == "stopped" {

                return Ok(output);

            }
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Err("출력 대기가 취소되었습니다".into()),
                _ = tokio::time::sleep_until(deadline) => return self.read(project, id, cursor),
                _ = changed => {},
            }

        }

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

    pub async fn restart(
        &self,
        policy: &Policy,
        project: &str,
        id: &str,
        expected_revision: Option<&str>,
    ) -> Result<Value, String> {

        let process = self.get(project, id)?;
        self.stop(project, id).await?;
        self.start(policy, project, &process.command, expected_revision).await

    }

    /** 첫 출력에서 handle을 반환하거나 종료까지 기다리며 같은 ring/cursor를 stream에 전달한다. */
    pub async fn run( &self, policy: &Policy, project: &str, command: &str, cancellation: CancellationToken, expected_revision: Option<&str>, options: RunOptions, ) -> Result<Value, String> {

        let RunOptions { yield_time, output: mut output_channel } = options;
        let token = CancellationToken::new();
        let cancel_on_drop = token.clone().drop_guard();
        let started = self.spawn(policy, project, command, yield_time.is_some(), token, expected_revision).await?;
        let id = started["process_id"].as_str().ok_or("process handle을 읽을 수 없습니다")?;
        let deadline = yield_time.map(|duration| tokio::time::Instant::now() + duration);
        let mut cursor = 0;
        loop {

            if cancellation.is_cancelled() {

                self.stop(project, id).await?;
                return Err("명령 요청이 취소되었습니다".into());

            }
            let mut output = self.read(project, id, cursor)?;
            output["project_id"] = json!(project);
            if let Some(sender) = &output_channel {

                tokio::select! {
                    biased;
                    _ = cancellation.cancelled() => {
                        self.stop(project, id).await?;
                        return Err("명령 요청이 취소되었습니다".into());
                    }
                    result = sender.send(output.clone()) => if result.is_err() { output_channel = None; },
                }

            }
            if output["status"] == "stopped" {

                let mut result = output["result"].clone();
                result["process_id"] = json!(id);
                result["next_cursor"] = output["next_cursor"].clone();
                result["events"] = output["events"].clone();
                result["truncated"] = output["truncated"].clone();
                return Ok(result);

            }
            cursor = output["next_cursor"].as_u64().ok_or("출력 cursor를 읽을 수 없습니다")?;
            if deadline.is_some_and(|deadline| tokio::time::Instant::now() >= deadline)
                || (deadline.is_some() && cursor > 0)
            {

                output["stdout"] = json!(
                    output["events"]
                        .as_array()
                        .map(|events| events
                            .iter()
                            .filter(|event| event["stream"] == "stdout")
                            .filter_map(|event| event["text"].as_str())
                            .collect::<String>())
                        .unwrap_or_default()
                );
                output["stderr"] = json!(
                    output["events"]
                        .as_array()
                        .map(|events| events
                            .iter()
                            .filter(|event| event["stream"] == "stderr")
                            .filter_map(|event| event["text"].as_str())
                            .collect::<String>())
                        .unwrap_or_default()
                );
                let _ = cancel_on_drop.disarm();
                return Ok(output);

            }
            let waiting = self.read_wait(project, id, cursor, 30, cancellation.clone());
            tokio::pin!(waiting);
            tokio::select! {
                result = &mut waiting => { result?; }
                _ = async {
                    match deadline {
                        Some(deadline) => tokio::time::sleep_until(deadline).await,
                        None => std::future::pending().await,
                    }
                } => {},
            }

        }

    }

    pub async fn sync_restart( &self, policy: &Policy, project: &str, ) -> Vec<String> {

        let files = match policy.files(project) {

            Ok(files) => files,
            Err(error) => return vec![error.to_string()],

        };
        let revision = match crate::sync::state::coordination::manifest(&files) {

            Ok(hash) => hash,
            Err(error) => return vec![error.to_string()],

        };
        let entries = self
            .entries
            .lock()
            .map(|entries| entries.iter().map(|(id, process)| (id.clone(), process.clone())).collect::<Vec<_>>())
            .unwrap_or_default();
        let mut errors = Vec::new();
        for (id, process) in entries {

            if process.project == project
                && !process.live.completed()
                && process.revision.as_deref() != Some(&revision)
                && policy.resolve(project, &process.command).is_ok_and(|(_, command)| command.restart_on_sync)
                && let Err(error) = self.restart(policy, project, &id, Some(&revision)).await
            {

                self.events.publish(project, "restart_failed", json!({"process_id":id,"error":error}));
                errors.push(error);

            }

        }
        errors

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

pub(crate) struct ChildGuard {

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

pub(crate) async fn stop(child: &mut dyn ChildWrapper) -> Result<(), ExecutionError> {

    child.start_kill().map_err(|source| failure("트리 종료", source))?;
    timeout(Duration::from_secs(5), child.wait())
        .await
        .map_err(|_| ExecutionError::CleanupTimeout)?
        .map_err(|source| failure("종료 확인", source))?;
    Ok(())

}

#[cfg(test)]
#[path = "../../tests/unit/process_revision.rs"]
mod revision_tests;
