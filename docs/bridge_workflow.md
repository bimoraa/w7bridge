# device/project 기반 작업 흐름

Agent는 hub의 project ID와 owner가 등록한 command 이름만 선택한다. 실행 파일, 인자, 환경, root, SSH 계정과 update trust key는 owner 설정이다. `project_status`로 online, 선택한 peer의 sync, 다른 paired peer의 conflict, process handle과 마지막 오류를 함께 확인한다.

## Windows owner 설정

```toml
version = 1
device_id = "windows-dev"

[codex]
enabled = false

[execution]
timeout_seconds = 300
output_bytes = 262144
concurrency = 2

[service]
allowed_sid = "S-1-5-21-실제-계정-SID"

[discovery]
roots = ['D:\Projects']
max_depth = 3

[[projects]]
id = "sample"
root = 'D:\Projects\sample'
requires_sync = true
sync_timeout_seconds = 120

[projects.files]
enabled = true
max_file_bytes = 67108864
exclude_dirs = ["artifacts"]

[projects.presets]
kind = "cargo"
executable = 'D:\Toolchains\rust\bin\cargo.exe'

[projects.git]
executable = 'C:\Program Files\Git\cmd\git.exe'

[projects.commands.run]
executable = 'D:\Toolchains\rust\bin\cargo.exe'
args = ["run", "--locked"]
background = true
restart_on_sync = true
```

실제 SID와 toolchain 경로를 지정해야 한다. Cargo preset은 `check/build/test/run --locked`를 생성하며 명시 command가 우선한다. Cargo check/build/test preset은 source snapshot을 사용한다. Node preset은 설치된 node_modules를 사용하는 원래 root에서 실행하며 snapshot 여부를 허위로 보장하지 않는다. `kind = "node"`는 package.json의 check/build/test/run script를 읽고 run이 없으면 dev를 사용한다. owner가 승인한 `bun.exe` 또는 `node.exe`와 npm CLI 경로를 `prefix_args`로 지정한다. `.cmd` launcher를 허용하지 않는다. discovery는 목록만 제공하며 발견한 root에 command나 파일 권한을 추가하지 않는다.

Source snapshot은 공유 파일 정책으로 복사한 별도 root에서 명령을 실행한다. 결과에 `artifact_root`, 시작/완료 revision과 `revision_verified`가 포함된다. 다음 sync가 진행 중 build의 source를 바꾸지 않는다. snapshot에는 `.git`, node_modules와 cache가 없다. 그런 입력이 필요한 command는 owner가 명시 command를 등록하고 `source_snapshot = false`로 설정한다. 이 경우 시작/완료 hash가 같아도 실행 중 source가 고정됐다는 보장은 없다. build script가 OS 계정으로 할 수 있는 작업을 제한하는 OS sandbox는 별도다.

## Mac hub 설정

```toml
version = 1
interval_seconds = 2

[[pairs]]
local_root = "/Users/me/Developer/sample"
remote_project = "sample"
host = "windows-dev"
service = true
expected_device_id = "windows-dev"
# 전용 설치 경로를 사용하는 경우 지정한다.
# executable = "D:/Tools/w7bridge/w7bridge.exe"
# config = "D:/Tools/w7bridge/owner.toml"
git_executable = "/usr/bin/git"
bandwidth_bytes_per_second = 524288
# desktop 사용자로 실행하는 별도 opt-in config다.
# screenshot_config = "D:/Tools/w7bridge/capture.toml"
```

```sh
w7bridge hub --config /absolute/path/pairing.toml
```

Codex의 MCP command를 Mac의 w7bridge executable, args를 `hub --config /absolute/path/pairing.toml`로 등록한다. hub는 pairing watcher와 SSH 연결을 유지한다. SSH 실패 후 다시 연결하며 offline 상태도 로컬 MCP에서 조회할 수 있다. watcher가 이미 실행 중이면 동일 pair journal을 중복으로 쓰지 않고 기존 daemon을 유지한다. service pairing에서만 명령을 실행하므로 연결을 다시 열어도 process handle은 같은 Windows service에서 찾을 수 있다.

여러 Windows는 `[[pairs]]`를 추가한다. 같은 Mac root도 서로 다른 Windows에 연결할 수 있으며 baseline과 Git handoff state는 pair별로 분리한다. 여러 Mac이 한 Windows project를 사용하는 경우 새 peer protocol이 각 Mac의 checkpoint와 build 요청을 구분한다. 다른 Mac의 checkpoint는 선택한 Mac의 fresh 요청을 확인하지 못한다. 활성 paired peer의 conflict는 build를 막는다. peer ID를 선택하지 않은 직접 client는 여러 peer일 때 명령을 거부하므로 hub의 device/project ID를 사용한다.

큰 repository는 owner가 project의 `sync_timeout_seconds`를 1–120초로 지정할 수 있다. 기본값은 기존 30초다. process 실행 timeout과 구분하며 MCP client가 이 owner 한도를 변경하지 않는다. reconnect의 identity 조회는 현재 generation을 읽기 전에 heartbeat를 보내지 않는다. file 요청 실패는 tool 이름과 제한된 peer 오류를 함께 기록한다.

파일 목록·읽기와 build snapshot은 shared access lock을 사용한다. 같은 source를 읽는 watcher와 snapshot이 서로 Busy를 만들지 않는다. snapshot 복사 동안 bridge의 파일 교체는 exclusive lock으로 막고 복사 전·후 manifest도 비교한다. 외부 editor는 bridge lock을 따르지 않으므로 hash가 달라지면 명령을 시작하지 않는다.

## MCP 호출 순서

```json
{"name":"list_projects","arguments":{}}
{"name":"project_status","arguments":{"project_id":"windows-dev__sample"}}
{"name":"run_command","arguments":{"project_id":"windows-dev__sample","command":"build"}}
{"name":"read_process_output","arguments":{"project_id":"windows-dev__sample","process_id":"응답의 ID","cursor":"응답의 next_cursor","wait_seconds":30}}
```

`start_process`와 `run_command` 모두 실행 전에 fresh sync round를 요청한다. conflict면 즉시 실패하고 offline/미완료 상태면 대기 한도 후 실패한다. `start_process`는 handle을 빨리 돌려준다. `run_command`도 기본적으로 첫 출력 또는 process 시작 후 100 ms에 handle과 현재 출력을 돌려준다. `yield_time_ms`로 시작 후 대기 한도를 0–30000 ms 안에서 지정한다. sync와 snapshot 준비 시간은 이 한도에 포함하지 않는다. `status = "running"`은 build 성공이 아니며 최종 exit code와 revision 검증은 완료 뒤 확인한다. 완료 전에는 `list_processes`에서도 handle을 찾을 수 있다. 로그의 `next_cursor` 숫자를 다음 호출의 `cursor`에 넣는다. `read_process_output`은 새 출력이 생기면 바로 응답하므로 30초 long poll이 출력 전달을 30초 늦추지 않는다. 오래된 로그가 ring 한도 밖으로 나갔으면 `truncated = true`다. 연결이 끊겨 응답 결과를 모르면 명령을 재실행하지 말고 handle과 event를 조회한다.

최종 결과까지 한 요청으로 기다리려면 `run_command`에 `wait = true`를 지정한다. 이때 `yield_time_ms`는 함께 지정하지 않는다. client가 요청 `_meta.progressToken`을 제공하면 실행 중 `notifications/progress`로 stdout/stderr chunk를 보내며 hub가 SSH peer의 token을 요청별로 분리해 전달한다. newline을 기다리지 않지만 child가 아직 flush하지 않은 출력은 읽을 수 없다. notification의 `_meta["io.w7bridge/output"]`에는 project/process ID, events와 cursor가 있다. 전송 간격은 최소 16 ms이며 bounded queue와 느린 client 제한을 사용한다. relay에서 notification이 유실되면 `truncated`, `missed_notifications`, `resume_cursor`를 보고하고 process cursor로 보충한다. client가 notification을 표시하지 않아도 기본 handle과 `read_process_output`으로 실행 중 출력을 읽을 수 있다. MCP notification이 Codex 대화 UI에 자동 표시된다고 보장하지 않는다.

`stop_process`는 sync 실패 중에도 전체 process tree를 종료한다. timeout은 foreground command에 적용한다. background command는 명시 stop 또는 host shutdown까지 계속 실행한다. `restart_on_sync = true`는 background + requires_sync command만 허용한다. 같은 revision checkpoint는 restart하지 않으며 build/test preset에 자동 restart를 붙이지 않는다.

`read_events`는 project event cursor와 service `boot_id`를 반환한다. service restart 뒤 boot ID가 바뀌면 cursor를 초기화한다. process handle, live log와 event ring은 service 메모리 상태다. SSH reconnect 후 유지되지만 service crash/restart 뒤 process를 자동 재실행하거나 이전 handle을 복원하지 않는다. 파일 journal과 history는 project의 `.w7bridge`에 보존한다.

## 안전한 sync와 recovery

최초 sync에서 한쪽에만 있는 파일은 반대쪽에 전달한다. 같은 경로의 내용이 다르면 양쪽 원본과 conflict snapshot을 보존한다. 파일 교체와 restore는 예상 SHA-256과 현재 파일을 비교한다. `.git`, target, node_modules, cache와 build output은 기본 제외이며 추가 output은 owner가 `exclude_dirs`에 지정한다. `.gitignore`는 공유 제외 규칙이 아니며 context 파일도 전달한다.

큰 파일은 64 KiB 고정 chunk로 나눠 SHA-256을 확인한다. 변경 chunk만 전송하고 staging manifest를 통해 중단 후 missing chunk부터 이어간다. 같은 hash의 파일을 이동했으면 기존 content를 재사용한다. source/context를 먼저, 같은 우선순위에서는 작은 파일을 먼저 전달하고 삭제는 마지막이다. 한 round의 파일 목록은 유한하다. 대역폭은 양방향 chunk의 base64 payload와 메시지 overhead 추정값을 기준으로 제한한다. 정확한 TLS/IP wire byte 한도는 아니다. legacy peer에서 제한을 보장할 수 없으면 pairing을 거부한다.

`sync_history`는 bridge가 관찰한 파일 교체/삭제의 이전·결과 hash를 제공한다. 임의 editor의 모든 중간 저장을 기록하지 않는다. 최대 64개 record와 256 MiB content를 보관하며 진행 중 recovery record는 제거하지 않는다. `restore_file`에는 revision ID, previous/result와 필수 `expected_hash`가 필요하다. 파일이 이후 변경됐으면 restore를 거부한다. blob이 corrupt하면 hash 검증에서 중단한다. `abort_transfer`는 지정한 staging transfer만 정리하며 현재 파일을 변경하지 않는다.

## Git handoff

양쪽 owner가 Git executable을 승인하면 file sync 완료 뒤 Git state도 비교한다. raw `.git` 파일은 sync하지 않는다. bundle과 staged object pack으로 commit history, HEAD/branch, refs와 index를 인계하고 실제 source는 동일한 파일 sync 정책을 사용한다. source의 hooks, credential, user config와 사용자 계정 설정을 복사하지 않는다. origin은 지원하는 공개 HTTPS/SSH 주소만 남기고 credential/query를 제거한다. 자동 commit/push하지 않는다.

한쪽에만 repository가 있으면 빈 쪽에 import한다. 이후 baseline에서 한쪽만 변경됐으면 반대쪽에 적용한다. 양쪽 Git state가 달라졌으면 conflict로 build를 중단한다. apply 직전 대상 Git state와 source manifest를 다시 확인하고 기존 metadata를 recovery backup으로 보존한다. Git backup은 자동 삭제하지 않는다. staged/unstaged/untracked와 rename/delete는 file 상태와 index를 함께 전달한다.

기존 linked worktree나 다른 worktree를 소유한 main repository의 metadata 교체, submodule/symlink index, 승인하지 않은 제외된 tracked 파일과 활성 merge/rebase는 fail closed다. 기존 linked worktree를 임의로 독립 repo로 바꾸지 않는다. Git archive 한도는 64 MiB, 로컬 metadata recovery 복사는 256 MiB/10000 entry/depth 32다. 지원 범위 밖 project는 원본을 보존하고 오류를 해결하기 전 build를 중단한다.

## 서명된 automatic update

```toml
version = 1
manifest_url = "https://owner.example.invalid/w7bridge/windows.json"
public_key_base64 = "owner가 pin한 Ed25519 public key"
installed_executable = 'D:\Tools\w7bridge\w7bridge.exe'
service = true
interval_seconds = 3600
```

```sh
w7bridge update --config /absolute/path/update.toml --status
w7bridge update --config /absolute/path/update.toml --once
w7bridge update install --config /absolute/path/update.toml
```

자동 helper는 별도 executable에서 실행하며 기존 owner registry/config를 보존한다. manifest는 Ed25519 서명, 증가하는 sequence, 지원 OS/architecture/protocol, HTTPS artifact의 크기/SHA-256과 candidate version을 확인한다. 실패와 interrupted replacement는 이전 binary backup으로 recovery한다. update 설정과 private signing key는 MCP client 입력으로 받지 않는다.

Windows helper는 현재 사용자의 Limited/로그인 task다. service binary를 갱신하려면 해당 계정이 설치 디렉터리 쓰기와 SCM start/stop 권한을 가져야 한다. 권한이 없으면 자동 update를 실패로 보고하며 더 높은 권한으로 우회하지 않는다. service는 기존 SCM 자동 시작을 사용하지만 updater는 로그인 전 실행을 보장하지 않는다. macOS helper는 사용자 LaunchAgent다. 설치 후 같은 trust key와 URL을 계속 사용한다. release hosting과 signing key 배포는 owner가 준비해야 하며 기본 public update channel은 없다.

검증 source와 현재 evidence 경계는 [기능 수용 기준](feature_acceptance.md)에 기록한다.

## source 이름과 output 이름이 겹치는 경우

`source_dirs = ["apps/desktop/build", "apps/desktop/tests/unit/engine/target"]`는 owner가 검토한 source 디렉터리만 공통 제외 규칙에서 다시 연다. 목록·읽기·쓰기·삭제·snapshot과 sync가 같은 정책을 사용한다. `.git`, node_modules, secret 파일과 실행 output 확장자는 계속 차단한다. target은 정확한 승인 디렉터리만 열며 그 안의 또 다른 target은 열지 않는다. custom `exclude_dirs`를 우회하지 않는다.

tracked output의 index가 필요한 경우 `[projects.git]`의 `local_only_paths`에 정확한 상대 파일 경로를 지정한다. 최대 256개며 secret/bridge metadata는 거부한다. Git object와 index만 유지하고 working file은 공유하지 않는다. 양쪽 장비의 device-local index flag도 보존한다. raw `.git`와 `.w7bridge`는 Syncthing 등 다른 sync 엔진에서 제외해야 한다. 같은 working root의 파일을 두 엔진이 동시에 쓰게 하지 않는다.

## 검토한 초기 Git 인계와 큰 repository

기존 양쪽 repo가 다른 branch를 가진 경우 기본 initial sync는 conflict다. owner가 두 state를 검토한 경우에만 pairing의 `[pairs.initial_git_handoff]`에 `source = "local"`, `expected_local_state`, `expected_remote_state`를 설정한다. 두 hash는 현재 `git_status`의 SHA-256과 같아야 하고 HEAD와 index도 같아야 한다. 이후 변경은 정상 baseline/CAS 규칙으로 처리한다. 검토 이후 상태가 바뀌면 초기 승인을 다시 사용하지 않는다.

큰 history는 owner가 독립 mirror에 검증한 local bundle을 먼저 seed할 수 있다. 기존 shared-worktree repo를 clone --shared나 metadata 복사로 연결하지 않는다. mirror의 object와 index를 준비한 뒤 새 export는 대상이 가진 commit/index OID를 제외한다. 신규 object와 branch/refs/index만 archive로 보내고 대상은 bundle, blob, fsck와 전체 hash를 검증한 후 apply한다. archive 한도는 여전히 64 MiB다. export cache는 내용 hash별로 분리하고 최신 4개만 보관한다. 오래된 export가 제거됐으면 source에서 재준비하며 대상에 도착한 chunk는 기존 transfer 규칙으로 재사용한다.

## 로그인 desktop host

GUI command를 bridge의 Job Object 아래에서 관리하려면 Windows owner 설정에 `[service] desktop = true`와 실제 `allowed_sid`를 지정하고 `w7bridge-desktop.exe --config <owner TOML>`을 로그인 계정의 Limited Task Scheduler task에서 직접 실행한다. 별도 console 없는 진입점이 기존 host runtime을 호출한다. PowerShell/Python wrapper를 task action으로 쓰면 task 중단 후 host가 남을 수 있으므로 직접 binary를 등록한다. console 진단은 `w7bridge desktop-host --config <owner TOML>`로 실행한다. SSH stdio와 relay는 기존 `w7bridge.exe`를 사용한다. host는 실제 token SID와 nonzero session을 확인한다. owner SID가 다르거나 session 0이면 실행을 거부한다. SCM mode와 혼용하지 않으며 동일 named pipe의 기존 host가 있으면 시작을 거부한다.

로그인 trigger, 실패 restart와 무제한 host 실행 시간을 owner task에 설정한다. 로그인 전 실행은 보장하지 않는다. SSH relay가 종료돼도 같은 host의 process handle과 log는 유지되며 cancel은 관리 중인 process tree를 종료한다. host가 crash/restart하면 이전 process/log는 복원하지 않는다. desktop capture는 별도 opt-in이며 실제 실행 revision과 exit code의 증거를 대체하지 않는다.

`xtask/scripts/native_cargo_windows.ps1`은 owner가 고정한 Cargo/manifest/Bun 경로로 Fatomic의 named command를 실행하는 helper다. 필요한 volume의 free space를 repo floor와 headroom에 비교하고 부족하면 Cargo를 시작하지 않는다. run은 owner가 배치한 로컬 secret config와 dependency 설치가 필요하며 이 파일은 source sync로 배포하지 않는다.

pair별로 검증한 파일 정책을 `.w7bridge/policy-<pair hash>.json`에 원자적으로 저장한다. watcher 재시작도 같은 source_dirs/exclude_dirs로 기존 journal을 검사하며 baseline을 초기화하지 않는다. 손상된 cache나 변경된 peer identity/policy는 오류로 중단한다. 정책 cache는 owner 승인이나 peer 검증을 대체하지 않는다.
