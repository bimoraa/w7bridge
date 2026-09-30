# w7bridge

Windows에 등록된 프로젝트의 명령을 다른 기기의 MCP 클라이언트에서 실행하는 Rust 서버다. 특정 프로젝트나 build 도구를 hardcode하지 않는다. 초기 MVP는 등록된 명령 실행의 입력과 수명을 명확히 하는 데 집중한다.

공식 [Rust MCP SDK `rmcp`](https://github.com/modelcontextprotocol/rust-sdk)가 protocol 초기화, version 협상, 도구 발견·호출과 취소를 처리한다. 연결은 [MCP stdio](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports)다. Mac에서 SSH로 Windows의 w7bridge를 실행하면 MCP stdin/stdout이 그대로 전달된다. 서버 자체는 network port를 열지 않는다.

```text
Mac MCP 클라이언트 → SSH → Windows w7bridge
                            → MCP 도구 → registry → executor → 등록된 명령
```

## 초기 범위와 구조

서버 package에 library와 CLI를 두고, `xtask` package는 개발 도구만 담당한다. database, plugin system, scheduler는 현재 필요하지 않다.

| 소유 모듈 | 책임 |
| --- | --- |
| `config` | version 1 TOML과 실행 한도 검증 |
| `security` | 프로젝트 registry, 명령 allowlist와 canonical 경로 검증 |
| `execution` | 직접 process 실행, 출력·시간·동시 실행 한도, 취소와 트리 정리 |
| `tools` | MCP 도구 schema, 인자 검증, 결과·오류 변환 |
| `platform` | Windows Job Object와 개발용 Unix process group |
| `server` | MCP protocol, stdio 연결과 host 종료 수명 |
| `error` | 설정·정책·실행의 typed error |
| `main` | CLI 설정 경로와 Ctrl+C 연결 |

MCP 도구는 `list_projects`, `run_command` 두 개다. build와 test도 같은 executor를 쓰며 local config에 고정된 명령으로 등록한다. 임의 shell 문자열, client가 고르는 executable·args·cwd·env, 원격 registry 수정, 파일 편집, background launch, HTTP transport와 service 설치는 현재 범위에 없다.

Windows service를 추가할 때는 SCM stop을 동일한 종료 token에 연결하는 host entry를 만들고 library를 재사용한다. HTTP 연결은 인증·TLS·Origin 정책과 함께 별도 transport로 설계한다. 장기 실행 launch에는 process 소유권, 중단 권한, 상태 조회가 필요하므로 현재 요청 수명에 flag만 붙이지 않는다.

## Windows에서 시작

Rust 1.88 이상과 MSVC toolchain의 build prerequisites가 필요하다. PowerShell에서:

```powershell
git clone https://github.com/bimoraa/w7bridge.git
Set-Location w7bridge
cargo build --locked --release
Copy-Item w7bridge.example.toml w7bridge.toml
.\target\release\w7bridge.exe --config .\w7bridge.toml
```

예제는 `projects = []`로 시작한다. 등록된 프로젝트가 없으면 아무 명령도 실행할 수 없다. `w7bridge.toml`은 git에서 제외한다. 프로젝트를 등록하려면 `projects = []`를 지우고 다음처럼 실제 절대 경로를 넣는다.

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

## Mac에서 연결

Windows의 [OpenSSH Server](https://learn.microsoft.com/en-us/windows-server/administration/openssh/openssh_install_firstuse)를 별도로 준비한다. 전용 일반 계정, SSH key 인증, 신뢰한 host key와 제한된 firewall 범위를 사용한다. SSH 계정 자체의 권한도 신뢰 경계다.

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

## 도구 계약과 한도

`list_projects`는 인자를 받지 않으며 프로젝트 ID와 명령 이름만 반환한다. root, executable, args, env는 공개하지 않는다.

```json
{ "projects": [{ "id": "sample", "commands": ["build", "test"] }] }
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

위 값이 기본값이다. 시간은 1..=3600초, 출력은 stream별 1024..=1048576 byte, 동시 실행은 1..=8개만 허용한다. slot이 없으면 즉시 거부하고 대기열을 만들지 않는다. 한도는 서버 instance별이며 SSH session 여러 개의 machine 전체 한도가 아니다.

stdout/stderr는 동시에 읽는다. 각 stream에서 한도까지의 앞부분만 보관하고 나머지는 버리면서 pipe를 계속 비운다. timeout은 process 대기와 출력 읽기를 함께 포함한다. UTF-8이 아닌 출력은 손실 변환한다. 취소·timeout 시 이미 수집한 출력은 유지하지만 pipe에 남은 모든 데이터까지 수집하지는 않는다.

## 보안과 프로세스 수명

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

CI는 Windows와 macOS에서 위 검사를 실행한다. 실제 stdio handshake, schema와 allowlist, 환경 변수 분리, 출력 제한, exit 실패, timeout·취소·연결 종료의 자식 정리를 테스트한다. ignored fixture는 일반 test가 환경을 준비해 실행하는 subprocess helper다. 실제 Windows SSH 연결과 service 설치는 별도의 운영 검증 대상이다.
