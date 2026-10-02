# w7bridge 기능 계획과 context 인계

## 현재 작업 — device/project workflow

`codex/sync-safety-e2e`의 변경과 기능별 evidence는 [기능 수용 기준](docs/feature_acceptance.md), owner 설정과 API 수명 계약은 [작업 흐름](docs/bridge_workflow.md)에 기록한다. 아래 날짜별 기록은 이전 source의 검증이며 이번 변경의 통과 근거로 재사용하지 않는다. 이번 작업은 tests와 Cargo.lock의 ignore를 제거하여 검증 입력을 보존한다.

## 현재 상태

`list_projects`, `run_command`와 SSH 기반 `connect`가 구현되어 있다. 2026-09-30에 Mac에서 Windows `DESKTOP-8UEAOEB`로 MCP 초기화, 도구 발견과 프로젝트 조회를 확인했고, 등록된 `hostname` 명령도 Job Object를 통해 exit code 0으로 끝났다. 당시 Codex CLI 등록은 별도 설정에서 검증하고 테스트용 설치를 제거했다. 이후 `C:\w7bridge` 영구 설치와 기본 Codex 설정의 `w7bridge` 등록을 완료했다. 아래 영구 설치 검증 기록을 참고한다.

Connect MCP 구현과 이 문서는 현재 작업 tree의 변경이며 commit/push하지 않았다. 로컬 formatter, compiler check, Clippy와 19개 테스트는 통과했다. `tests/`와 `Cargo.lock`은 사용자의 선택에 따라 Git에서 제외되어 있으므로 새 checkout과 CI의 재현성은 아직 검증하지 않았다.

현재 작업 tree에는 파일 access, 양방향 sync daemon, 상태/fresh generation gate, live process output, Windows SCM service/relay와 Mac LaunchAgent가 구현되어 있다. 최종 Mac·Windows workspace 검사와 process/sync integration, 실제 service/SSH 흐름은 통과했다. 최신 전체 검사와 Windows native service/SSH E2E 결과는 [sync 검증](docs/sync_validation.md)에 기록했다. Codex project 자동 조회가 포함된 영구 binary 갱신과 실제 SSH MCP 검증은 아래 최신 기록을 참고한다.

## 제품 범위

w7bridge는 remote command execution, project registry, process management, file/project access, logs/status, Windows service와 secure remote bridge를 담당한다. vector database, embeddings, semantic memory 또는 custom AI memory engine은 현재 범위에 넣지 않는다.

## 프로젝트 파일로 context 인계

중요한 context는 한 Codex 채팅에만 남기지 않고 공유 프로젝트 파일에 기록한다. `AGENTS.md`는 작업 규칙, `MEMORY.md`는 결정과 유지할 context, `PLANS.md`는 계획과 진행 상태에 사용할 수 있다. 모든 프로젝트에 세 파일을 강제로 생성하지 않으며, 프로젝트가 이미 쓰는 다른 context 파일도 같은 방식으로 공유한다.

흐름은 **Windows에서 context 파일 수정 → 프로젝트 파일 sync → Mac에서 같은 파일 읽기 → 작업 계속**이다. 반대 방향도 같다. w7bridge는 파일을 전달하고 Codex는 그 파일을 읽고 수정한다. bridge가 채팅 내용을 자동 추출하거나 별도 memory 저장소를 만들지 않는다. 기기를 옮긴 뒤 Codex는 적용되는 `AGENTS.md`와 context 파일을 읽고 현재 source 및 Git 상태와 맞춰 확인한다.

context에는 이어서 작업할 사람이 필요한 결정, 현재 상태, 남은 작업과 검증 결과를 짧게 남긴다. 아직 확인하지 않은 내용은 검증된 결과처럼 기록하지 않는다.

승인된 memory MCP는 `read_memory`, `update_memory`로 이 파일을 직접 읽고 갱신한다. `memory/`는 context 경로·UTF-8 문서·revision을 소유하고 기존 `FileStore`와 sync를 사용한다. 갱신에는 필수 `expected_hash`를 받아 최신 editor·sync 변경을 보존한다. 파일 공유 권한이 없는 project와 미등록 context 경로는 거부한다. 별도 AI memory engine은 만들지 않는다.

## 파일 sharing과 sync 계약

- project registry에 등록된 root 안에서 source, 문서와 context 파일을 공유한다. `AGENTS.md`, `MEMORY.md`, `PLANS.md` 및 사용자가 지정한 context 파일은 기본 sync 대상이다.
- Git tracking과 sync 포함 여부는 별개다. `.gitignore`를 그대로 sync 제외 목록으로 사용해서 context 파일을 누락시키지 않는다. context 파일이 Git에서 제외되어 있어도 sync 포함 대상으로 유지한다.
- `.git` 디렉터리 또는 파일, `target`, `node_modules`, build artifact와 cache는 sync하지 않는다. 프로젝트별 build output/cache 위치는 명시적으로 정하며, 임의의 전체 dotfile 또는 Markdown 제외 규칙을 만들지 않는다.
- context 포함 규칙은 제외된 build/cache 디렉터리를 다시 열지 않는다. 예를 들어 `target/MEMORY.md`는 제외하고 project root의 `MEMORY.md`는 포함한다.
- 파일 목록과 읽기·쓰기·전송이 같은 제외 정책을 사용한다. root 밖 경로, path traversal와 root 밖을 가리키는 symlink는 허용하지 않는다.
- 양쪽에서 같은 파일을 수정했다면 조용히 덮어쓰지 않는다. 기존 파일의 버전을 확인하고 충돌을 보고하며 양쪽 변경을 보존한다. 읽는 쪽에는 완성된 파일을 전달한다.
- sync가 Codex 채팅 기록을 복제하거나 Windows와 Mac의 파일 경로를 동일하게 만든다고 가정하지 않는다. 각 기기의 project root는 따로 등록한다.

## 다음 작업

- [x] `list_projects`에서 Codex 로컬 project registry를 자동 조회한다. 최신 SQLite를 read-only로 읽고 legacy JSON은 구버전에만 사용한다. 자동 발견은 command·파일 권한을 변경하지 않는다.

- [x] Windows의 고정 경로에 server와 config를 설치하고 기본 Codex 설정에 연결한다.
- [ ] 실제 프로젝트의 build/test를 Mac의 Codex 채팅에서 실행하고 결과를 확인한다.
- [x] project file access와 양방향 sync를 구현한다. 먼저 source와 context 파일, 공통 제외 정책과 충돌 처리를 연결한다.
- [x] context 파일을 직접 읽고 hash가 일치할 때만 갱신하는 memory MCP를 구현한다.
- [ ] SSH, 인증, host key, server path, config와 handshake 실패를 구분하는 connection doctor를 추가한다.
- [ ] Windows 소유자가 registry를 관리할 local CLI, release binary와 공백 있는 경로 지원을 추가한다.
- [x] logs/status와 장기 실행 process의 상태·중단 권한을 추가한다.
- [x] Windows service host를 구현하고 SCM stop을 기존 종료 token 및 process cleanup에 연결한다.

## Context sync 완료 기준

- [x] Windows에서 `MEMORY.md`를 수정한 뒤 Mac에서 같은 내용을 읽는다. Mac에서 수정한 내용도 Windows에 전달된다.
- [x] `AGENTS.md`, `PLANS.md`와 사용자가 지정한 다른 context 파일도 양방향으로 전달된다.
- [x] context 파일을 Git에서 제외한 경우에도 sync에서 누락되지 않는다.
- [x] `.git`, `target`, `node_modules`, 프로젝트 build artifact와 cache는 양쪽 전송 목록 및 실제 전송에서 제외된다.
- [x] 관찰된 동시 수정 충돌은 양쪽 원본과 snapshot을 보존하며 build를 막는다. 교체 전 hash를 확인하고 완성된 파일을 전달한다. editor가 advisory lock을 따르지 않는 짧은 TOCTOU 한계는 README에 명시한다.
- [ ] 실제 Mac과 Windows에서 sync 후 Codex가 context 파일을 읽고 작업을 이어가는 흐름을 검증한다.

## Windows 영구 설치 검증 — 2026-09-30

- CLI `install`은 기본 `C:/w7bridge`에 executable과 config를 배치한다. `--config`는 최초 설정 복사, `--dir`은 다른 경로, `--update`는 기존 설정을 보존하는 executable 갱신이다. Windows service는 만들지 않으며 SSH 연결이 서버 수명을 소유한다.
- Windows `DESKTOP-8UEAOEB`에 `C:\w7bridge\w7bridge.exe`와 `C:\w7bridge\w7bridge.toml`을 실제 설치했다. registry는 빈 상태다. 기본 Codex 설정에 SSH alias `codex-pc-lan`을 사용하는 `w7bridge` MCP를 등록하고 다시 목록에서 enabled 상태와 설치 경로를 확인했다.
- Windows GNU target으로 installer와 기존 두 도구만 포함한 source snapshot을 build했다. 동시에 작업 중인 file-access 변경은 이 설치 binary에 포함하지 않았다. 설치 binary SHA-256은 `7962b14be8c4f3df05d775de522327410a7ba77968bed525f7aeb0635d244b03`이다.
- Windows native 실행에서 installer unit test 6개와 CLI integration test 2개가 통과했다. integration은 실제 binary 설치, MCP 초기화·`list_projects`, 연결 종료 후 갱신과 설정 byte 보존을 검사했다. 실제 Mac→Windows SSH 연결에서도 MCP 초기화·도구 발견·프로젝트 조회를 확인했다.
- 검증 시점의 로컬 workspace에서 formatter check, `cargo check --locked --workspace --all-targets`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace`가 통과했다. 전체 suite에서 32개 테스트가 통과하고 subprocess fixture 2개는 harness에 의해 호출되는 ignored helper였다. 이후 추가한 복사 실패 cleanup test도 installer source snapshot에서 통과했다. 마지막 workspace 실행은 동시에 추가 중인 `mod sync`의 `src/sync.rs`가 아직 없어 실패했다. 최신 sync 작업의 전체 검증은 별도 확인이 필요하다. 설치 binary에는 이 변경이 없다.
- `sshd`는 Running/Automatic이었다. PC reboot, 새 Codex 채팅에서 도구 호출, 실제 프로젝트 build/test는 아직 확인하지 않았다. 새 채팅부터 등록된 MCP를 사용한다. `tests/`와 `Cargo.lock`의 기존 Git 제외 정책은 유지했다. commit/push하지 않았다.

## 명시적 화면 캡처 검증 — 2026-09-30

- `screenshots.enabled`는 기본 `false`다. 로컬 소유자가 켜면 `capture_screenshot` MCP 도구를 등록한다. 파일 경로·실행 파일·계정 선택 인자는 받지 않는다. PNG는 8 MiB 및 2천만 pixel로 제한한다.
- macOS에서 실제 binary stdio를 통해 MCP 초기화, 도구 발견, opt-in·인자 거부와 PNG 응답을 검증했다. `cargo test --locked --test capture -- --include-ignored`의 2개 test가 통과했다. 별도 실제 MCP 요청으로 받은 3024×1964 PNG (1,073,967 byte)도 시각적으로 확인했다.
- Windows MSVC에서 library 35개 test와 같은 capture integration 2개 test가 통과했다. 이미 삭제된 task의 cleanup이 실패로 처리되던 PowerShell exit-code 문제를 수정하고 native 회귀 test를 추가했다.
- Mac→Windows SSH의 실제 MCP 요청으로 2560×1440 PNG (2,103,995 byte)를 받고 desktop 이미지임을 확인했다. 응답 후 서버는 exit code 0으로 종료했고 `w7bridge-capture-*` task, 임시 capture 폴더 및 worker process는 남지 않았다.
- Windows capture는 같은 사용자 SID의 Interactive/Limited 일회성 작업으로 실행한다. LocalService·LocalSystem·NetworkService에서는 거부한다. Windows service의 sync·명령 endpoint와 desktop 사용자의 opt-in stdio capture endpoint를 구분한다. Mac의 OS 화면 녹화 권한이나 Windows의 login desktop 권한을 자동으로 추가하지 않는다.
- 위 검증은 임시 native snapshot에서 수행했다. `C:\w7bridge`의 기존 영구 설치 binary와 registry는 변경하지 않았다. 최신 sync→wait→build/test/run의 전체 service E2E는 별도 기록으로 확인한다.
- 최종 source 105개의 hash가 Mac과 Windows에서 일치한 뒤 캡처를 다시 요청했다. Mac 3024×1964 PNG (1,500,495 byte)와 Windows 2560×1440 PNG (2,391,527 byte)를 MCP로 받고 시각적으로 확인했다. cleanup에서 원래 캡처 오류를 보존하는 마지막 변경도 양쪽 native 검사에 포함했다. 요청 뒤 task·임시 폴더·worker가 0개였고 검증용 capture config와 PNG는 제거했다.

## 파일 MCP native 검증 — 2026-09-30

- Mac→Windows SSH user stdio와 LocalService relay 양쪽에서 실제 `list_files`와 `read_file`을 호출했다. fixture manifest 7개에 Git ignore된 `MEMORY.md`, `AGENTS.md`, `CUSTOM.md`가 포함되고 `.git`, `target`, `node_modules`, `build`, `.cache`의 marker는 없었다.
- `MEMORY.md`의 18 byte와 응답 SHA-256 일치를 확인했다. `../service.toml`과 제외된 `.git`·`target`·`node_modules` 파일 읽기는 모두 structured `path` 오류로 거부했다. 이 검증은 source와 context 내용을 변경하지 않았다.


## 양방향 sync·service 검증 — 2026-09-30

[source snapshot과 전체 결과](docs/sync_validation.md)에 최종 검사와 native SSH 증거를 기록했다. Mac 49개, Windows 46개 test가 통과했고 native capture는 별도로 실행했다. Windows initial import, 양쪽 context 변경, build/test/run, live output, 충돌 보존/build gate, 연결 복구, process control, SCM/LaunchAgent stop 및 crash recovery를 확인했다.

운영 project의 root/command/toolchain은 아직 선택하지 않았다. 기존 영구 binary/config와 Codex 등록은 유지했으며 임시 service와 fixture를 제거하고 잔여 process가 없는 것을 확인했다. 실제 fatomic build/test, 새 Codex 채팅과 PC reboot는 미검증이다.

## memory MCP 검증 — 2026-09-30

- `memory/`의 경로·문서·revision·storage와 `tools/memory/`의 실제 MCP 읽기·갱신을 구현했다. registry의 file 권한, 제외 정책과 `FileStore`의 lock·조건부 쓰기를 그대로 사용한다.
- Mac에서 memory 관련 unit 7개와 실제 stdio integration 3개가 통과했다. UTF-8·CRLF 보존, 없는 문서 조회, custom context, server 재시작 후 파일 유지, stale hash 거부, 잘못된 경로·symlink·인자·encoding·크기, 취소와 동시 작업 한도를 검사했다. 기존 `Session`을 사용한 양방향 context sync와 양쪽 동시 수정 보존도 검사했다.
- 전체 workspace는 66개 test가 통과했고 helper/native capture 6개는 ignored였다. 최종 formatter check, workspace/all-targets compiler check와 Clippy `-D warnings`도 통과했다.
- 같은 source의 Windows GNU release build는 Mac의 기존 toolchain으로 통과했다. 이후 전송 방법을 수정해 Windows 영구 binary를 갱신하고 실제 SSH MCP 초기화·도구 발견·project 조회를 확인했다. 자세한 hash와 설치 기록은 아래 항목에 있다. Windows native launcher는 실제 Cargo 작업 전에 종료했으므로 MSVC suite와 memory 읽기·갱신의 실제 Windows 호출은 미검증이다.
- 진행 중 checkout이 `main`으로 바뀌어 최종 작업은 `/Users/hillaryabigail/.codex/worktrees/codex-project-discovery/w7bridge`에 유지했다. `tests/`와 `Cargo.lock`의 기존 Git 제외 정책은 보존했다. 주 checkout 이동과 commit/push는 수행하지 않았으며 Windows 설치 갱신은 조율한 Connect MCP 작업에서 수행했다.

## Codex project 자동 조회와 설치 갱신 — 2026-09-30

- `list_projects`는 호출할 때마다 Windows Codex의 최신 `state_*.sqlite`에서 project와 root만 read-only로 읽는다. SQLite가 없는 구버전에서는 `.codex-global-state.json`의 `local-projects`만 사용한다. `[codex].home`, `CODEX_HOME`, 현재 계정의 `.codex` 순서로 경로를 선택한다. 수동 registry의 command와 파일 권한은 유지하고 자동 발견으로 권한을 추가하지 않는다.
- Mac에서 discovery unit 4개와 실제 stdio integration 1개가 통과했다. 같은 연결에서 WAL 변경·이름 변경·삭제, legacy 범위, 잘못된 schema와 경로, missing/disabled 상태 및 권한 분리를 검사했다. 전체 workspace 66개 test와 formatter, compiler check, Clippy가 통과했다. 추가한 live SSH test는 기본 실행에서 ignored이고 실제 환경을 지정해 별도로 통과했다.
- Windows GNU release build와 workspace/all-targets compiler check가 통과했다. 실제 Windows에서 해당 binary의 `--version` 및 Rust `rmcp` client를 통한 Mac→Windows SSH MCP 초기화·도구 발견·`list_projects`를 확인했다. candidate와 영구 설치 양쪽에서 Codex project 11개를 받았고 모든 root가 존재했다. `fatomic`, `cloud`, `roblox-tools` 등이 포함되며 command는 빈 배열이었다.
- `C:/w7bridge/w7bridge.exe`를 installer의 `install --update`로 갱신했다. installed SHA-256은 `7ab3ba67384b7b62c569caf3362d17ee94861e0650945b4a5def6055e2f53a35`이며 원본 build와 일치한다. config SHA-256 `775a6f8a02513a319f793aec664f904c33529eb8562efd1a2cec278006d773a9`는 갱신 전후 동일하다. 기존 executable은 `D:/w7bridge-build-project-discovery-01a0f14b/w7bridge-installed-backup.exe`에 보존했다.
- Windows MSVC 전체 suite와 memory MCP의 실제 Windows 호출은 이번 검증에 포함하지 못했다. native launcher가 Cargo를 실행하기 전에 종료했다. 현재 Codex 채팅의 기존 MCP transport는 닫혀 있어 native tool 재호출은 `Transport closed`였다. 새 installed binary에 연결하려면 Codex MCP를 재연결해야 한다. 실제 Rust SDK MCP 검증과 현재 Codex tool 연결 상태를 구분한다.
- 최종 source와 Git에서 제외된 test·lockfile은 위 managed worktree에 보존했다. 이 작업에서 commit/push하지 않았다.
- 사용자가 주 repository에서 source가 보이지 않는 문제를 확인한 뒤 `/Users/hillaryabigail/Documents/ChatGPT/w7bridge`를 `codex/source-structure`로 되돌렸다. managed worktree의 최종 변경과 제외된 test·lockfile도 주 repository에 반영했다. 덮어쓴 기존 test·lockfile은 `/tmp/w7bridge-repo-restore-backup-01a0f14b`에 보존했고 원본 managed worktree도 유지한다.

## Windows console 창 억제 — 2026-10-02

- 실제 실행 중인 host는 `D:/w7bridge/fatomic-2158/w7bridge-desktop.exe`다. 주 checkout의 미완료 sync 작업을 보존하기 위해 `8136b64` 기준의 별도 managed worktree와 `codex/hide-windows-console` branch에서 수정했다.
- Windows managed child는 `CreationFlags(CREATE_NO_WINDOW)`와 기존 `JobObject`를 함께 사용한다. 주기적인 Git 조회, 등록된 명령과 capture·update 검증에 같은 정책을 적용한다. 직접 실행하는 SSH·Codex child와 updater task 관리 명령에도 console 억제를 적용했다. process 수명과 piped output은 유지한다.
- Mac과 native Windows MSVC에서 최종 formatter check, workspace/all-targets check, Clippy `-D warnings` 및 workspace test가 통과했다. Mac은 101 passed / 9 ignored, Windows는 100 passed / 10 ignored다. ignored 항목은 기존 native/live 검증과 subprocess helper이며 새 console fixture는 두 native 회귀 test가 subprocess로 호출한다.
- native 회귀 test 두 개는 managed child와 SSH 형태의 직접 child에서 `GetConsoleWindow() == NULL`, stdout/stderr 전달 및 정상 종료를 확인했다. 기존 command integration은 timeout·취소·연결 종료의 descendant cleanup을 확인했다. compiler 입력 152개 파일의 Mac/Windows SHA-256이 모두 일치했다.
- native debug binary 두 개를 실제 설치 경로에 반영하고 기존 `w7bridge-fatomic-host` task를 재시작했다. CLI SHA-256은 `74f11f673da872b6d66d9bd8d2d2c3594d6ca5662943592bc90d4949858ef1e3`, desktop SHA-256은 `14df50546da20338d778f5df27e5a7955252f8c7c06d2e37d6933d14b9612566`이다. owner config hash는 설치 전후 `14fadaf3bebfcc38ca8c617d32a2233838cf59d40a0da7636b9197264eb5e21c`로 같다. 이전 binary·config·task XML은 `D:/w7bridge/fatomic-2158/console-fix-backup-20261002`에 보존했다.
- 재시작한 host는 desktop session 1에서 window handle 0으로 실행 중이다. 실제 Mac→Windows SSH MCP 초기화·도구 발견·project 조회가 통과했고 기존 Codex hub도 새 boot ID로 재연결했다. fatomic Git 상태 조회 응답을 확인했다. 전체 project sync 완료나 reboot 후 실행, 장시간 화면 관찰은 이번 검증에 포함하지 않았다. commit/push와 주 checkout 변경은 하지 않았다.
