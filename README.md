# w7bridge

Windows에 등록된 프로젝트의 명령을 다른 기기의 MCP 클라이언트에서 실행하는 Rust 서버다. 특정 프로젝트나 build 도구를 hardcode하지 않는다. 명령 실행, 양방향 project sync와 process 수명을 제공한다.

공식 [Rust MCP SDK `rmcp`](https://github.com/modelcontextprotocol/rust-sdk)가 protocol 초기화, version 협상, 도구 발견·호출과 취소를 처리한다. 연결은 [MCP stdio](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports)다. Mac에서 SSH로 Windows의 w7bridge를 실행하면 MCP stdin/stdout이 그대로 전달된다. 서버 자체는 network port를 열지 않는다.

```text
Mac MCP 클라이언트 → SSH → Windows w7bridge
                            → MCP 도구 → registry → executor → 등록된 명령
```

## 초기 범위와 구조

서버 package에 library와 CLI를 두고, `xtask` package는 개발 도구만 담당한다. 별도 database, plugin system, scheduler는 현재 필요하지 않다. Codex의 기존 database는 project metadata를 읽는 용도로만 사용한다.

| 소유 모듈 | 책임 |
| --- | --- |
| `config` | version 1 TOML과 실행 한도 검증 |
| `security` | 프로젝트 registry, 명령 allowlist와 canonical 경로 검증 |
| `execution` | 직접 process 실행, 출력·시간·동시 실행 한도, 취소와 트리 정리 |
| `tools` | MCP 도구 schema, 인자 검증, 결과·오류 변환 |
| `platform` | Windows Job Object와 개발용 Unix process group |
| `server` | MCP protocol, stdio 연결과 host 종료 수명 |
| `error` | 설정·정책·실행의 typed error |
| `app` | CLI dispatch, 설치 진입과 Ctrl+C 종료 수명 |
| `main` | binary entry와 종료 코드 |
| `filesystem` | 공유 파일·설치 파일 배치와 pairing watcher |
| `connection` | Mac의 SSH 연결 검증, 지속 peer 연결과 Codex MCP 등록 |
| `protocol` | 도구 입력과 실행 결과의 wire 타입 |
| `sync` | baseline, journal, 충돌 보존과 전송 계획 |
| `memory` | context 문서 읽기·조건부 갱신과 파일 revision; 기존 FileStore 재사용 |

사용자가 지정한 source tree와 이전 경로의 이동 계약은 [source 구조](docs/source_structure.md)에 있다. 미구현 모듈은 문서만 두고 도구를 등록하지 않는다.

현재 source에는 파일 목록·읽기·쓰기, context 읽기·갱신, sync 상태·대기, process 제어와 Windows service가 구현되어 있다. 2026-09-30에 `C:\w7bridge`의 영구 binary를 갱신하고 기존 config를 보존했다. 새 binary와 연결하려면 종료된 MCP transport를 재연결한다. 기능별 검증 범위는 [PLANS.md](PLANS.md)에 기록했다. client가 고르는 executable·args·cwd·env와 원격 registry 수정은 허용하지 않는다.

## 프로젝트 파일과 context 인계 계획

Mac과 Windows 사이의 작업 context는 프로젝트와 함께 공유하는 `AGENTS.md`, `MEMORY.md`, `PLANS.md`와 다른 context 파일에 남긴다. Windows에서 파일을 수정하면 sync를 통해 Mac에도 전달하고, 이어서 작업하는 Codex가 같은 파일을 읽는다. 중요한 결정, 진행 상태와 다음 작업을 한 채팅에만 보관하지 않는다.

파일 sharing과 양방향 sync daemon이 이 흐름을 수행한다. context 파일은 sync 대상에 포함하고 `.git`, `target`, `node_modules`, build artifact와 cache는 제외해야 한다. `.gitignore`와 sync 정책은 별도 계약이다. 파일 범위, 충돌 처리와 완료 기준은 [PLANS.md](PLANS.md)에 있다.

## project memory MCP

`read_memory`, `update_memory`는 project 안의 context 파일을 읽고 갱신한다. 기본 경로는 `MEMORY.md`이며 `AGENTS.md`, `PLANS.md`와 로컬 config의 `projects.files.context_files`에 등록한 경로도 허용한다. 해당 project는 registry에 명시적으로 등록하고 `projects.files.enabled = true`로 설정해야 한다. Codex 자동 발견만으로는 파일 권한을 주지 않는다.

`read_memory` 인자 예시:

```json
{"project_id":"sample","path":"MEMORY.md"}
```

응답에는 `project_id`, `path`, 허용된 `context_files`, `exists`, UTF-8 `content`, 원본 byte의 `sha256`, `bytes`가 있다. 파일이 없으면 `exists=false`, `content=null`, `sha256=null`, `bytes=0`이며 읽기만으로 context 파일을 만들지 않는다. 매 요청에 실제 파일을 읽으므로 editor와 sync의 변경도 다음 읽기에 보인다. 문서 내용이나 줄바꿈을 자동 정규화하지 않는다.

`update_memory`는 문서 전체와 필수 `expected_hash`를 받는다. 처음 생성할 때만 `null`을 사용한다:

```json
{"project_id":"sample","content":"이어갈 작업 context","expected_hash":null}
```

기존 파일은 `read_memory`에서 받은 64자리 소문자 SHA-256을 `expected_hash`에 넣는다. 읽은 뒤 editor나 sync가 수정하면 `conflict`로 실패하며 새 내용을 보존한다. 다시 읽고 검토한 뒤 갱신한다. 응답이 유실되어도 무조건 재시도하지 않는다. 빈 문자열은 빈 파일로 저장하며 삭제나 append 인자는 제공하지 않는다.

파일 access와 동일한 lock, symlink·제외 경로 정책, 1 MiB 한도와 동시 작업 슬롯을 사용한다. UTF-8이 아닌 파일은 `data` 오류다. 저장 성공은 해당 project 파일에 반영됐다는 의미이며 다른 기기까지 sync됐다는 의미는 아니다. pairing daemon을 실행하고 `wait_for_sync`로 새 round 완료를 확인한 뒤 상대 기기에서 같은 파일을 읽는다. `.gitignore`와 관계없이 기존 context sync 대상이다. 채팅 자동 추출이나 별도 memory database는 없다.

## Windows에서 시작

Rust 1.88 이상과 MSVC toolchain의 build prerequisites가 필요하다. PowerShell에서:

```powershell
git clone https://github.com/bimoraa/w7bridge.git
Set-Location w7bridge
cargo build --locked --release
Copy-Item w7bridge.example.toml w7bridge.toml
.\target\release\w7bridge.exe --config .\w7bridge.toml
```

예제는 `projects = []`로 시작하지만 `list_projects`는 Codex에 있는 로컬 project 목록을 자동으로 보여준다. 이름, Codex ID, root 목록과 현재 디렉터리 존재 여부를 매 요청에 다시 읽는다. Codex에 project를 추가하거나 이름을 바꾸거나 삭제하면 다음 조회에 반영된다. 별도 registry 생성이나 Codex 설정 변경은 하지 않는다.

현재 Codex의 `state_<version>.sqlite`에서 `projects`와 `project_roots`만 read-only로 조회한다. project table이 없는 구버전에서는 `.codex-global-state.json`의 `local-projects`만 읽는다. 현재 database가 비었으면 오래된 JSON 목록을 되살리지 않는다. remote project, 채팅, memory, 인증 정보는 반환하지 않는다. 내부 저장 형식이 바뀌거나 읽기 실패가 있으면 `codex.status = "error"`로 보고하며 명시적으로 등록한 명령 목록은 유지한다.

Codex home은 `[codex].home`, `CODEX_HOME`, 현재 계정의 `.codex` 순서로 찾는다. 자동 조회는 기본 활성화이며 `[codex]`의 `enabled = false`로 끌 수 있다. Windows service에서 사용자 project를 조회하려면 실제 사용자 Codex home과 읽기 권한을 로컬 설정에서 지정한다. home은 절대 경로이며 client가 변경할 수 없다.

자동으로 발견한 project는 목록만 제공한다. command 실행이나 file 공유 권한을 자동으로 추가하지 않는다. `w7bridge.toml`은 git에서 제외한다. 명령을 허용하려면 `projects = []`를 지우고 다음처럼 실제 절대 경로와 command를 넣는다.

```toml
version = 1

[[projects]]
id = "sample"
root = 'C:\Projects\sample'

[projects.commands.build]
executable = 'C:\Users\bridge\.cargo\bin\cargo.exe'
args = ["build", "--locked"]

[projects.commands.test]
executable = 'C:\Users\bridge\.cargo\bin\cargo.exe'
args = ["test", "--locked"]

[projects.commands.test.env]
RUST_BACKTRACE = "1"
```

root는 존재하는 절대 디렉터리, Windows executable은 존재하는 절대 `.exe` 파일이어야 한다. `.cmd`와 `.bat`는 거부한다. 경로에 들어 있는 `~`나 환경 변수를 자동 확장하지 않는다. ID와 명령 이름은 1..=64자의 영문 소문자, 숫자, `_`, `-`만 허용한다. 알 수 없는 설정 항목과 중복 ID는 오류다. 설정 변경은 서버 재시작으로 적용한다.

서버는 stdin에서 MCP 초기화를 기다린다. 서버 모드 stdout은 MCP 메시지 전용이고 진단은 stderr로 간다. `--help`, `--version`은 일반 CLI 출력이다.

### Windows 영구 설치

Windows에서 build한 실행 파일의 `install`을 사용하면 repository와 별개인 `C:\w7bridge`에 실행 파일과 설정을 배치한다. 파일은 logout과 재부팅 후에도 남는다. Mac의 MCP 클라이언트가 SSH로 연결할 때마다 서버가 실행되고 연결이 끝나면 종료된다. 이 설치만으로 자동 시작 service를 만들지는 않는다. 공유 상태와 항상 실행되는 host가 필요하면 아래 `service install`을 추가한다. SSH로 다시 연결하려면 Windows가 켜져 있고 OpenSSH Server가 실행 중이어야 한다.

```powershell
cargo build --locked --release

# 최초 설치: 기본적으로 명령을 실행할 수 없는 빈 registry 생성
.\target\release\w7bridge.exe install

# 또는 처음부터 검증된 로컬 설정을 복사
.\target\release\w7bridge.exe install --config .\w7bridge.toml

# 설치된 설정을 편집한 뒤 Mac에서 연결
notepad C:\w7bridge\w7bridge.toml
```

위 두 설치 command는 대안이다. 기본 경로는 `connect`의 executable·config 기본값과 같다. 다른 drive나 디렉터리는 `install --dir D:/tools/w7bridge`로 지정하고, Mac의 `connect`에도 `--executable D:/tools/w7bridge/w7bridge.exe --config D:/tools/w7bridge/w7bridge.toml`을 전달한다. 설치 경로는 공백·shell 문자·`.`·`..` 요소·끝의 점이 없는 절대 drive 경로여야 한다. 설정 원본에는 공백이나 상대 경로를 사용할 수 있다.

installer는 TOML과 registry 경로·명령을 검증한 뒤 파일을 준비한다. 프로젝트 명령은 실행하지 않는다. 기존 executable은 `--update` 없이 교체하지 않고 기존 설정은 byte 단위로 보존한다. 설정이 이미 있을 때 `--config`를 지정하면 오류다. 기존 설정이 잘못되었으면 먼저 수정해야 갱신할 수 있다.

```powershell
# 기존 MCP 연결을 닫고 새 build의 executable에서 갱신
cargo build --locked --release
.\target\release\w7bridge.exe install --update
```

설치된 executable 자체에서 갱신하지 않는다. Windows에서 기존 서버가 실행 중이거나 파일이 잠겨 있으면 교체가 실패할 수 있으므로 연결을 닫고 다시 시도한다. 준비한 executable을 rename으로 교체하므로 복사 실패는 기존 executable을 훼손하지 않는다. 최초 설치에서 설정 배치 후 executable 배치가 실패하면 설정은 남고 다음 설치에서 그대로 사용한다.

설치 volume은 hard link를 지원해야 한다 (예: NTFS). 완성된 파일을 hard link로 최초 배치하여 기존 파일을 덮어쓰지 않는다. 작업 중에는 `.w7bridge-install` 디렉터리를 독점한다. 일반 실패에는 이번 작업의 임시 파일을 정리하지만 강제 종료 흔적은 남을 수 있다. 이 디렉터리가 이미 있으면 설치를 중단한다. 다른 설치 작업이 실행 중인지 확인하고 이전 작업의 잔여 파일만 별도로 정리한다.

설치 계정은 대상 디렉터리에 쓰기 권한이 있어야 한다. `C:\w7bridge`를 만들 권한이 없으면 관리자가 디렉터리와 전용 계정 권한을 먼저 준비하거나 쓰기 가능한 다른 drive 경로를 지정한다. SSH 계정에는 executable 읽기·실행, config 읽기와 실제 프로젝트 작업에 필요한 권한을 부여한다. 설치 경로와 부모 경로를 신뢰하고 다른 일반 계정의 쓰기를 제한한다. installer는 계정·ACL·PATH·OpenSSH·firewall·자동 시작 설정을 변경하지 않는다. 제거할 때는 연결을 닫고 설치한 executable만 삭제한다. config는 명령 registry이므로 필요한 경우 보관한다.

## Mac에서 연결

Windows의 [OpenSSH Server](https://learn.microsoft.com/en-us/windows-server/administration/openssh/openssh_install_firstuse)를 별도로 준비한다. 전용 일반 계정, SSH key 인증, 신뢰한 host key와 제한된 firewall 범위를 사용한다. SSH 계정 자체의 권한도 신뢰 경계다.

### Connect MCP

Mac에서도 w7bridge를 build한 뒤 `connect`를 실행한다. Windows 서버 실행 파일과 설정의 기본 위치는 `C:/w7bridge/w7bridge.exe`, `C:/w7bridge/w7bridge.toml`이다. SSH alias를 쓰면 해당 alias의 user, port와 identity 설정을 그대로 사용한다.

```sh
cargo build --locked --release
./target/release/w7bridge connect --host bridge@windows-host
```

기본 모드는 `codex` CLI가 PATH에 있어야 한다. 기존 MCP 이름을 먼저 확인하고, SSH를 통해 MCP 초기화·서버 이름·필수 도구를 검증한 뒤 `list_projects`만 호출한다. 확인에 성공하면 `codex mcp add w7bridge -- ssh ...`로 등록한다. 프로젝트 명령은 실행하지 않으며 Windows registry도 변경하지 않는다. 등록 후 Codex의 새 채팅에서 MCP를 사용한다. 이미 열린 연결은 자동으로 교체하지 않는다.

```sh
# Codex 등록 없이 연결만 확인
./target/release/w7bridge connect --host windows --check

# 다른 설치 경로, port, 로컬 key와 MCP 이름 지정
./target/release/w7bridge connect --host bridge@windows-host \
  --executable C:/tools/w7bridge.exe --config C:/config/w7bridge.toml \
  --port 2222 --identity /Users/me/.ssh/windows_bridge --name windows_dev

# network와 Codex CLI 호출 없이 ~/.codex/config.toml에 넣을 TOML 출력
./target/release/w7bridge connect --host windows --print-config
```

`--timeout`은 연결 확인에만 적용하며 기본 30초, 범위는 1..=120초다. timeout·실패·Ctrl+C에는 로컬 SSH process를 종료하고 회수한다. 등록된 연결의 startup timeout은 Codex 설정을 따른다. 같은 MCP 이름은 덮어쓰지 않으므로 다른 `--name`을 사용하거나 기존 항목을 직접 관리한다. 목록 검사와 등록은 서로 다른 CLI 호출이므로 같은 이름의 병렬 등록은 피한다.

현재 생성하는 remote command는 Windows OpenSSH의 기본 `cmd.exe` shell과 공백 없는 절대 drive 경로를 전제로 한다. 공백, 환경 변수 치환과 shell 문자가 있는 경로는 인자 검증에서 거부한다. 공백 없는 전용 설치 경로를 준비한다. SSH key와 host key는 미리 준비해야 하며, 연결 command는 `BatchMode=yes`, `StrictHostKeyChecking=yes`, PTY 비활성화를 유지한다. 인증·host key 검증을 우회하거나 Windows OpenSSH를 자동 설치하지 않는다.

CLI의 `connect` 모듈은 Mac 쪽 연결과 client 등록만 소유한다. Windows 서버 transport와 registry 도구 계약은 유지하며 `--service`는 공유 service relay를 선택한다.

### 다른 stdio 클라이언트

stdio command를 지원하는 MCP 클라이언트의 설정 예시다. 바깥쪽 형식은 client에 따라 다를 수 있다. Windows의 기본 OpenSSH shell이 `cmd.exe`이고 remote 경로에 공백이 없는 구성을 기준으로 한다. 실행 파일과 설정 파일을 실제 위치에 배치해야 한다. 공백이 있는 경로나 다른 기본 shell에는 해당 shell의 quoting을 적용한다.

```json
{
  "mcpServers": {
    "w7bridge": {
      "command": "ssh",
      "args": [
        "-T",
        "-o", "BatchMode=yes",
        "-o", "StrictHostKeyChecking=yes",
        "bridge@windows-host",
        "C:/w7bridge/w7bridge.exe --config C:/w7bridge/w7bridge.toml"
      ]
    }
  }
}
```

먼저 같은 remote command로 SSH 연결을 확인하고 host key를 등록한다. `-T`로 PTY를 만들지 않는다. shell 시작 스크립트나 login banner가 stdout에 텍스트를 출력하면 protocol framing이 깨질 수 있다. URL 연결만 받는 MCP client는 현재 지원하지 않는다.

## 양방향 project sync와 자동 실행

Mac의 pairing daemon이 SSH를 유지하며 기본 2초 간격으로 양쪽 파일을 비교한다. 각 project는 Windows registry ID 하나와 Mac 절대 root 하나를 연결한다. Mac root가 없으면 만들고 Windows에만 있는 파일을 가져온다. 양쪽에 다른 내용의 같은 파일이 이미 있으면 최초 sync도 충돌로 남긴다. Windows root는 먼저 존재해야 한다.

Windows registry 예시:

```toml
version = 1

[service]
allowed_sid = "S-1-5-21-실제-SSH-계정-SID"

[[projects]]
id = "fatomic"
root = 'C:\dev\fatomic'
requires_sync = true

[projects.files]
enabled = true
exclude_dirs = ["artifacts", "vendor/cache"]
context_files = ["docs/work_context.md"]

[projects.commands.build]
executable = 'C:\Users\bridge\.cargo\bin\cargo.exe'
args = ["build", "--locked"]

[projects.commands.test]
executable = 'C:\Users\bridge\.cargo\bin\cargo.exe'
args = ["test", "--locked"]

[projects.commands.run]
executable = 'C:\Users\bridge\.cargo\bin\cargo.exe'
args = ["run", "--locked"]
background = true
```

`allowed_sid`는 예시 문자열 대신 `whoami /user`의 숫자 SID를 넣는다. 서비스가 실행하는 toolchain과 필요한 환경은 config 소유자가 준비한다. `requires_sync = true`는 `run_command`, `start_process`, `restart_process`마다 새 sync round를 기다린다. 30초 안에 새 round가 확인되지 않거나 충돌이면 명령을 실행하지 않는다. 이전 `synced` 기록만 보고 build하지 않는다. `stop_process`는 sync 실패에도 사용할 수 있다.

Mac의 `pairing.toml`은 Windows registry와 다른 설정 파일이다:

```toml
version = 1
interval_seconds = 2

[[pairs]]
local_root = "/Users/me/Developer/fatomic"
remote_project = "fatomic"
host = "codex-pc-lan"
service = true
# 기본 설치 경로가 아니면 아래 값을 지정한다.
# executable = "C:/tools/w7bridge/w7bridge.exe"
```

`~`와 환경 변수는 확장하지 않는다. `service = true`와 Codex의 `connect --service`는 같은 Windows service에 연결한다. sync 상태와 process handle은 그 service가 소유하므로 SSH 연결이 끊겨도 유지된다. 별도 stdio 서버 두 개에 연결하면 상태가 공유되지 않아 `requires_sync` gate가 완료되지 않는다. registry 변경 후 service를 재시작한다.

```sh
# 최초 전송을 한 번 확인하거나 foreground로 계속 실행
w7bridge sync --config /absolute/path/pairing.toml --once
w7bridge sync --config /absolute/path/pairing.toml

# pairing 상태: synced / syncing / conflict / offline
w7bridge sync --config /absolute/path/pairing.toml --status

# Mac 로그인 후 자동 시작하며 daemon이 종료되면 다시 시작
w7bridge sync install --config /absolute/path/pairing.toml
w7bridge sync background-status
w7bridge sync stop
w7bridge sync start
w7bridge sync uninstall
```

Mac installer는 현재 binary를 `~/Library/Application Support/w7bridge/w7bridge`에 복사하고 `~/Library/LaunchAgents/com.w7bridge.sync.plist`를 만든다. 기존 binary/job은 덮어쓰지 않는다. binary 또는 plist 배치 뒤 실패하면 이미 배치된 파일은 남을 수 있으며 오류를 확인하고 설치 상태를 점검한다. `uninstall`은 이 형식의 job과 binary만 제거하고 pairing 설정과 `sync.log`를 보존한다. SSH/key는 daemon 사용자에게 non-interactive로 접근 가능해야 한다. Mac logout 동안 sync는 멈추고 다음 로그인에 다시 시작한다.

연결 실패는 `offline`으로 기록하고 최대 30초 간격의 backoff로 재연결한다. baseline과 전송 journal은 Mac project의 `.w7bridge/sync.json`에 보관한다. 응답을 잃은 write는 다음 round에서 실제 hash를 확인한다. 같은 경로라도 remote root identity나 제외 정책이 달라지면 기존 baseline을 재사용하지 않고 중단한다. root 변경 또는 새 pairing은 양쪽 파일을 먼저 보존하고 상태를 검토해야 한다. baseline 파일을 임의로 지우면 삭제와 수정의 과거 기준도 사라진다.

충돌은 원본 양쪽을 그대로 두고 Mac `.w7bridge/conflict-<hash>-local.bin`, `-remote.bin`, `.json`에 보존한다. 삭제된 쪽에는 `.bin`이 없다. 두 원본을 같은 원하는 내용으로 합치거나 양쪽에서 같은 삭제를 수행하면 다음 round에서 충돌이 해제된다. snapshot은 자동 삭제하지 않는다. 이름의 hash는 경로와 양쪽 content hash로 결정된다.

공통 제외는 `.git`, `target`, `node_modules`, `dist`, `build`, `out`, cache와 binary artifact, 실제 `.env`다. `.gitignore`는 sync 제외 정책이 아니며 `AGENTS.md`, `MEMORY.md`, `PLANS.md`는 Git tracking과 관계없이 전달된다. 추가 build 경로는 `exclude_dirs`에 명시한다. 목록·읽기·쓰기가 같은 정책을 쓰며 symlink/special file과 Windows에서 충돌하는 대소문자·file/directory 경로는 거부한다. 현재 전송 한도는 파일당 1 MiB, 10,000개, manifest 전체 256 MiB다. 빈 디렉터리와 POSIX permission bit는 동기화하지 않는다.

쓰기 직전 hash를 확인하고 완성된 임시 파일을 rename한다. 새로운 파일 생성은 기존 파일을 덮어쓰지 않는다. 로컬 editor는 bridge의 advisory lock을 따르지 않으므로 hash 검사와 기존 파일 교체 사이의 아주 짧은 동시 쓰기를 OS 수준으로 막는 것은 아니다. 지속적으로 변경되는 파일은 재검사하며 `synced`는 마지막 round의 관찰 결과다.

MCP의 `sync_status`는 daemon lease가 만료되면 `offline`을 반환한다. `wait_for_sync`는 새 generation을 요청하고 다음 round의 완료를 기다린다. `sync_checkpoint`는 daemon이 사용하는 도구로 local registry나 executable을 수정하지 않는다.

## Windows service와 process 제어

관리자 PowerShell에서 service 계정과 프로젝트 권한을 먼저 준비한다. `service install`은 `LocalService` 계정의 자동 시작 SCM service를 만들고 시작한다. 이미 존재하는 `w7bridge` service는 교체하지 않는다.

```powershell
C:\w7bridge\w7bridge.exe service install --config C:\w7bridge\w7bridge.toml
Get-Service w7bridge
Stop-Service w7bridge
Start-Service w7bridge

# 제거는 SCM에서 수행한다. executable/config는 자동 삭제하지 않는다.
sc.exe delete w7bridge
```

LocalService에는 executable/toolchain/config 읽기·실행과 project 수정 권한이 필요하다. 사용자 전용 `.cargo`/`.rustup`의 접근 권한을 그대로 쓸 수 있다고 가정하지 않는다. 전용 toolchain과 writable cache를 준비하며 MSVC linker·SDK library 경로가 필요한 환경은 등록된 명령의 `env`에 명시한다. SSH 사용자에게도 config를 수정할 권한을 자동 부여하지 않는다. 일반 `install`과 `service install`은 다른 작업이다. service 생성 후 recovery 설정이나 start가 실패하면 SCM 등록은 남을 수 있으므로 `Get-Service`와 config/ACL을 확인한다. 장애 recovery는 5, 10, 30초 뒤 재시작으로 설정한다. 정상 stop은 SCM exit code 0이다. 시작/host 실패의 service-specific code는 2=config 경로, 3=config 읽기·SID, 4=registry, 5=runtime, 6=pipe 생성, 7=연결 loop 단계다. config 옆의 `<이름>.service-error.log`에는 최신 오류 한 건만 최대 8 KiB로 기록한다. config 위치가 읽기 전용이면 log를 만들지 않고 SCM code를 사용한다. log를 위해 registry 디렉터리 쓰기 권한을 넓히지 않는다. SCM stop/shutdown은 MCP 연결과 process 트리를 함께 종료한다. service 시작은 로그온과 독립적이며 실제 reboot 검증은 별도다.

Mac에서 `w7bridge connect --host codex-pc-lan --service`로 등록한다. SSH remote command는 `w7bridge.exe relay`이며 이 process는 local named pipe에 stdio를 전달한다. pipe는 remote 접속을 거부하고 SYSTEM/관리자/LocalService/지정한 SSH SID만 허용한다. service가 없거나 pipe ACL에 실패하면 일반 stdio 서버로 우회하지 않는다. 최대 16개의 동시 MCP 연결을 허용한다.

`start_process`는 등록된 명령을 시작하고 `process_id`를 반환한다. `read_process_output`으로 완료 전 stdout/stderr를 읽고 `next_cursor`를 다음 요청의 `cursor`로 보낸다. `list_processes`는 응답을 잃거나 다시 연결한 client가 handle을 찾는 데 쓴다. `stop_process`, `restart_process`는 project와 handle을 함께 확인하고 Windows Job Object 전체를 종료한다. 임의 PID를 받지 않는다.

```json
{"project_id":"fatomic","command":"run"}
```

위 인자는 `start_process`에 전달한다. 조회/종료/재시작은 `{"project_id":"fatomic","process_id":"반환된-handle"}`를 쓴다. `read_process_output`은 선택적으로 `cursor`를 받는다. ring은 128 KiB/256 event로 제한하며 오래된 출력이 사라지면 `truncated`를 표시한다. 서버는 최대 32개의 handle을 보관하고 완료된 오래된 항목부터 회수한다. service 재시작은 handle을 복구하지 않으며 실행 중인 앱도 종료한다.

일반 `run_command`는 완료 후 결과를 반환한다. 실시간 output이 필요한 build/test도 `start_process`로 실행할 수 있다. `background = true`는 managed process에서 실행 timeout을 해제하며 normal `run_command`의 timeout은 유지한다. 두 경로는 같은 concurrency slot과 출력/환경/Job Object 정책을 쓴다. Windows service의 Session 0에는 사용자 화면이 없으므로 GUI 앱이 로그인 desktop에 나타나지는 않는다.

## 도구 계약과 한도

`list_projects`는 인자를 받지 않는다. 기존 registry의 ID·명령 목록과 Codex 로컬 project metadata를 함께 반환한다. 같은 canonical root에 등록된 권한이 있으면 기존 ID와 명령을 유지하고 Codex metadata를 합친다. executable, args, env는 공개하지 않는다. 자동으로 발견한 project의 `commands`는 빈 목록이다. 다중 root는 `roots`로 반환하며 현재 존재하지 않는 root가 있으면 `available = false`다.

```json
{
  "projects": [
    { "id": "sample", "commands": ["build", "test"] },
    { "id": "codex-project-id", "name": "fatomic", "roots": ["D:/Downloads/fatomic"],
      "available": true, "commands": [], "source": "codex" }
  ],
  "codex": { "status": "ready", "source": "codex_database" }
}
```

`run_command`는 다음 두 문자열만 받는다. 추가 인자는 거부한다.

```json
{ "project_id": "sample", "command": "build" }
```

결과에는 `status` (`completed`, `timed_out`, `cancelled`), `success`, `exit_code`, `stdout`, `stderr`, `stdout_truncated`, `stderr_truncated`가 있다. timeout과 취소의 exit code는 `null`이다. 실패한 exit, timeout, 실행 실패와 반환된 취소 결과는 도구 오류(`isError: true`)다. MCP 취소 notification을 받으면 SDK가 해당 응답을 버릴 수 있으므로 취소 응답 수신을 보장하지 않는다. 잘못된 인자와 알 수 없는 도구는 protocol 오류다. 자동 retry나 파일 변경 rollback은 하지 않는다.

```toml
[execution]
timeout_seconds = 60
output_bytes = 65536
concurrency = 1
```

위 값이 기본값이다. 시간은 1..=3600초, 출력은 stream별 1024..=1048576 byte, 동시 실행은 1..=8개만 허용한다. slot이 없으면 즉시 거부하고 대기열을 만들지 않는다. 한도는 Bridge instance별이다. Windows service의 relay client는 같은 Bridge와 한도를 공유하고 별도 stdio 서버는 공유하지 않는다.

stdout/stderr는 동시에 읽는다. 각 stream에서 한도까지의 앞부분만 보관하고 나머지는 버리면서 pipe를 계속 비운다. timeout은 process 대기와 출력 읽기를 함께 포함한다. UTF-8이 아닌 출력은 손실 변환한다. 취소·timeout 시 이미 수집한 출력은 유지하지만 pipe에 남은 모든 데이터까지 수집하지는 않는다.

## 보안과 프로세스 수명

### 요청할 때만 화면 캡처

화면 캡처는 기본적으로 꺼져 있다. 로컬 설정 소유자가 아래 설정을 추가하고 서버를 다시 시작하면 `capture_screenshot` 도구를 등록한다. Mac과 Windows 모두 별도 frontend 없이 MCP 이미지 응답으로 현재 화면을 반환한다.

```toml
[screenshots]
enabled = true
```

```json
{ "display": 1 }
```

`display`는 생략하면 1이며 1..=16만 받는다. 1은 기본 화면이고 나머지는 OS에서 찾은 다른 화면이다. client가 저장 경로, executable, 계정이나 임의 script를 지정할 수 없다. 한 번에 한 요청만 처리하고 PNG는 8 MiB 및 2천만 pixel로 제한한다. 결과의 `structuredContent`에는 `display`, `width`, `height`, `machine_os`가 있고 `content`에는 `image/png` 이미지가 있다. 주기적으로 화면을 수집하거나 프로젝트에 이미지를 저장하지 않는다.

macOS는 `/usr/sbin/screencapture`를 실행한다. 실행 계정의 화면 녹화 권한이 필요하며 GUI login과 권한이 없는 실행 환경에서는 오류를 반환한다. Windows SSH는 Session 0에서 실행될 수 있으므로 같은 Windows 사용자 SID의 Interactive/Limited 일회성 작업으로 login desktop에서 캡처한다. 작업과 임시 파일은 요청이 끝나면 정리한다. login된 같은 사용자의 desktop이 없으면 오류를 반환한다. 다른 사용자를 선택하거나 비밀번호를 저장하지 않는다.

Windows의 LocalService·LocalSystem·NetworkService 계정에서는 캡처를 거부한다. Windows service로 sync와 command를 운영할 때 화면이 필요하면 해당 desktop 사용자의 SSH stdio endpoint에만 캡처를 켠다. 권한을 바꾸거나 service를 interactive 계정으로 우회하지 않는다. MCP 실패는 `disabled`, `busy`, `cancelled`, `timeout`, `unavailable`, `limit`, `data`, `cleanup` 등 structured code로 반환한다.

실제 GUI에서 PNG를 받는 integration test는 화면 권한이 필요하므로 일반 suite에서 제외한다. 해당 환경에서 `cargo test --locked --test capture -- --include-ignored`를 따로 실행하면 binary의 실제 stdio 초기화, 도구 발견과 캡처 응답을 검사한다.

설정 소유자가 명령을 등록하고 client는 이름만 고른다. 자식 환경은 비운 뒤 `PATH`, `SystemRoot`, `WINDIR`, `TEMP`, `TMP`, `TMPDIR`, `USERPROFILE`, `HOME`, `CARGO_HOME`, `RUSTUP_HOME` 중 존재하는 값만 상속한다. 명령의 로컬 `env`는 이를 덮어쓰거나 값을 추가할 수 있다. 인증 토큰 환경 변수를 자동 전달하지 않는다.

등록된 명령은 서버 계정 권한으로 실행된다. **OS sandbox가 아니다.** build script는 코드를 실행하고, 명령은 프로젝트 밖의 파일·network·계정 credential에 접근할 수 있다. config, 프로젝트 source, executable, toolchain, 상속되는 PATH를 신뢰해야 한다. config는 등록된 프로젝트 밖에 두고 전용 계정의 쓰기 권한을 제한한다. 명령이 출력한 secret을 자동 지우지는 않는다.

registry는 시작 시 canonical 경로를 고정하고 실행 직전에 다시 검사한다. 경로 검사와 process 생성 사이의 파일 교체를 원자적으로 막지는 않는다. 로컬 파일 쓰기 권한이 신뢰 경계다.

Windows는 [`process-wrap`](https://docs.rs/process-wrap/10.0.1/process_wrap/)의 Job Object로 process 트리를 묶는다. process를 일시 정지 상태로 생성하고 Job 연결 뒤 재개하며 연결 실패 시 실행을 우회하지 않는다. timeout·취소 시 트리를 종료하고 최대 5초 동안 종료를 확인한다. future가 drop되면 guard가 종료를 요청하고 Job handle도 닫힌다. 서버 비정상 종료는 Job의 kill-on-close 정책에 맡긴다. macOS/Linux는 개발과 테스트용 process group을 사용하며, 의도적으로 group을 벗어나는 process를 sandbox로 막지는 않는다.

## 개발과 검증

[AGENTS.md](AGENTS.md)와 [RUST.md](RUST.md)는 사용자가 지정한 [bimoraa/AGENTS](https://github.com/bimoraa/AGENTS)의 한국어판을 따른다. 일반 주석은 자연스러운 반말 한국어로, 공개 계약은 block rustdoc으로 작성한다. body multiline의 안쪽 시작과 끝에는 빈 줄 하나를 둔다.

```sh
cargo run --locked -p xtask
cargo run --locked -p xtask -- --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
```

`xtask`의 최종 formatter는 rustfmt 다음 syn으로 body padding을 적용한다. editor와 CI에서 동일한 command를 사용한다. 일반 `cargo fmt --check`는 최종 padding 검사 command가 아니다. AST parser가 이해하지 않는 macro token과 import에는 padding을 넣지 않는다. 문자열 보존과 pipeline 멱등성은 별도 작은 test로 검사한다.

CI는 Windows와 macOS에서 위 검사를 실행한다. 실제 stdio handshake, schema와 allowlist, 환경 변수 분리, 출력 제한, exit 실패, timeout·취소·연결 종료의 자식 정리를 테스트한다. 설치 test는 설정 보존·실패 시 임시 파일 정리·중복 설치 거부를 검사한다. Windows native test는 임시 경로에 실제 binary를 설치하고 MCP 초기화·list_projects·갱신을 검사한다. ignored fixture는 일반 test가 환경을 준비해 실행하는 subprocess helper다. 실제 Windows SSH 연결과 대상 PC의 계정·재부팅 후 연결은 별도의 운영 검증 대상이다.
