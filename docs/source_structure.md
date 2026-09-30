# source 구조와 이동 계약

사용자가 지정한 `src/` tree를 따른다. 기존 구현은 아래 owner로 이동하고 미구현 모듈은 한국어 문서만 둔다. 예약 모듈은 도구 등록이나 별도 state store를 만들지 않는다.

| 이전 경로 | 새 owner | consumer | 기존 검증 |
| --- | --- | --- | --- |
| `main.rs`, `install.rs` CLI | `app/bootstrap.rs`, `app/lifecycle.rs`, `config/paths.rs` | binary entry | CLI, installer unit/integration |
| `install.rs` 파일 배치 | `filesystem/copy.rs` | bootstrap | installer unit/integration |
| `config.rs` | `config/model.rs`, `config/loader.rs` | Bridge, executor, registry | config unit |
| `connect.rs` | `connection/client.rs`, `transport.rs`, `handshake.rs`, `session.rs` | connect CLI, sync watcher | connect unit |
| `execution/command.rs`, `output.rs` | `execution/runner.rs`, `command.rs`, `sandbox.rs`, `protocol/response.rs` | run/process tools | command integration |
| `execution/managed.rs`, `process.rs` | `execution/process.rs` | executor, Bridge | process lifecycle 검증은 담당 채팅에서 후속 진행 |
| `files.rs` | `filesystem/transfer.rs`, `paths.rs`, `metadata.rs`, `config/model.rs` | file tools, sync | files unit |
| `security.rs` | `security/permissions.rs` | Bridge, tools | policy unit |
| `server.rs`, `tools.rs` routing | `server/handler.rs`, `runtime.rs`, `router.rs` | stdio host | protocol integration |
| `tools/command.rs`, `process.rs` | `tools/execution/run_command.rs`, `mod.rs` | router | command/protocol integration |
| `tools/system.rs` | `tools/project/status.rs` | router | protocol integration |
| `tools/filesystem.rs` | `tools/files/mod.rs`, `read.rs`, `write.rs`, `search.rs`, `protocol/request.rs` | router | files unit, protocol integration |
| `sync.rs` library | `sync/engine.rs`, `state.rs`, `conflict.rs`, `manifest.rs`, `planner.rs` | watcher, library consumer | sync unit |
| `sync_cli.rs` | `config/model.rs`, `connection/session.rs`, `app/lifecycle.rs`, `filesystem/watcher.rs` | sync CLI | 기존 sync unit, 후속 SSH 검증 |
| `error.rs` | `error/app.rs`, `execution.rs`, `filesystem.rs` | 각 owner | compiler 및 기존 suite |
| `platform.rs`, `platform/windows.rs` | `platform/mod.rs`, OS별 `process.rs` | executor | macOS 실행, Windows target check |

`w7bridge::files`는 기존 library consumer가 쓰는 이름이라 `filesystem`의 re-export로 유지한다. 다음 public API breaking release에서 consumer를 `filesystem`으로 옮긴 뒤 alias를 제거한다. root `SyncError`와 `sync::SyncError`는 authoritative library의 같은 타입을 공개한다. consumer가 없는 one-shot error와 transport helper는 담당 채팅의 동의를 받아 제거했다.

`memory/`와 `tools/memory/`는 공유 프로젝트 파일의 context 인계 경계다. vector database, embeddings, semantic memory, 별도 AI memory engine과 채팅 추출 작업은 구현하지 않는다. `tools/sync/`는 sync_status, wait_for_sync, sync_checkpoint를 등록한다. daemon round와 CLI lifecycle은 각각 sync/와 app·filesystem owner에 유지한다.

이동 중 두 기능 채팅은 mutation·formatter·build를 멈춘다. source와 tests backup은 repository 밖에 보관하고, 이동 후에는 새 경로와 검증 결과를 두 채팅에 전달한다. Windows 설치 binary는 이 source 이동만으로 갱신하지 않는다.

## 이동 완료 검증 — 2026-09-30

요청한 Rust source 77개가 모두 있고 추가 경로는 없다. 이동 checkpoint에서 최종 formatter check, workspace/all-targets compiler check와 Clippy가 통과했다. macOS workspace test 37개가 통과했으며 ignored 2개는 integration harness가 호출하는 subprocess helper다. Windows GNU target의 workspace/all-targets check도 통과했다. 전용 `CARGO_TARGET_DIR=target/source-structure`를 사용해 다른 snapshot의 cached xtask와 분리했다.

기존 discovery assertion은 process 도구 추가분을 포함해 9개 이름을 정확히 검사하도록 맞췄고 policy fixture에는 기존 `background` field의 기본값을 반영했다. 테스트 body는 `tests/`에 유지했다. 새 구조의 native Windows 실행과 실제 SSH 연결은 이 이동 작업에서 다시 검증하지 않았다. 후속 기능 변경은 이 checkpoint 이후 별도 검증한다. commit/push와 설치 binary 갱신은 하지 않았다.


## 후속 기능 통합

이동 checkpoint 후 Windows SCM host/relay는 server/runtime, Mac LaunchAgent는 platform/macos/process, 공유 sync generation/lease는 sync/state, MCP 상태·대기·checkpoint는 tools/sync에 구현했다. process의 live output과 handle은 기존 execution/process를 확장했다. native lifecycle과 실제 SSH 검증은 [최신 결과](sync_validation.md)에 기록했다.

별도로 승인된 명시적 screenshot 기능에는 capture.rs, error/capture.rs, tools/capture.rs를 추가했다. 예약 filesystem/platform 모듈을 OS helper에 사용한다. 이 확장은 이동 checkpoint의 77개 inventory와 구분한다. memory와 project switch의 예약 모듈은 계속 도구를 등록하지 않는다.
