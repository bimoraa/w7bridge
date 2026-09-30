# w7bridge 개발 지침

먼저 [한국어 범용 지침](docs/agents/AGENTS.md)과 [Rust 규칙](RUST.md)을 읽는다. 사용자가 지정한 `bimoraa/AGENTS`의 `kr/` 판을 적용한다. 다음은 이 저장소의 구체적인 계약이다.

- 프로젝트가 작성하는 문서, 주석, CLI/help, 오류·메시지는 한국어다. 일반 코드 주석은 자연스러운 반말로, 공개 API 계약은 block rustdoc으로 쓴다. identifier, API, protocol, 외부 process 출력과 generated 파일의 기술 표기는 유지한다.
- 사용자가 지정한 구조를 따른다. 루트 package가 서버를 소유하고 `xtask`는 개발 도구만 소유한다. workspace 기본 build 대상은 서버다.
- `config`는 설정, `error`는 typed error, `security`는 프로젝트 registry와 정책, `execution`은 프로세스 수명과 출력, `tools`는 MCP 도구, `platform`은 OS 차이, `server`는 MCP 연결, `main`은 CLI와 종료 신호를 소유한다.
- 파일과 장기 실행 process 도구의 모듈 경계는 유지하되 아직 쓰지 않는 기능을 활성화하지 않는다. 현재 도구는 `list_projects`와 `run_command`뿐이다.
- client가 executable·args·cwd·env·registry를 수정하게 하지 않는다. stdio에는 MCP 메시지만 쓴다. Windows Job Object 실패를 우회하지 않는다. 경로 검증은 OS sandbox가 아니다.
- 네 칸 공백과 120열을 사용한다. 비어 있지 않은 multiline 중괄호 body의 시작과 끝 안쪽에 빈 줄 하나를 둔다. import와 지원하지 않는 macro token에는 padding을 넣지 않는다.
- editor와 CI의 최종 formatter는 `cargo run --locked -p xtask`다. rustfmt 다음 syn으로 body padding을 적용한다. `cargo fmt --check`만으로 최종 style을 검사하지 않는다. pipeline의 반복 실행과 check는 같은 결과를 유지해야 한다.
- 검증 command는 `cargo run --locked -p xtask -- --check`, `cargo check --locked --workspace --all-targets`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace`다.
- 테스트 본문과 fixture는 production `src/` 밖에 둔다. unit suite는 `tests/unit/`, 실제 stdio integration은 `tests/command.rs`와 `tests/protocol.rs`, subprocess fixture는 `tests/fixtures/`에 둔다. xtask 테스트는 `xtask/tests/`에 둔다.
- Windows가 실행 주 대상이고 macOS는 개발·검증에 사용한다. OS별 정적 검증, native 실행, 실제 SSH 연결을 구분해서 보고한다.
- 별도 disk quota는 없다. 무거운 build 전 실제 여유 공간을 확인한다. subagent는 명시적으로 요청받았을 때만 사용한다. 사용자 작업을 보존하고 commit/push는 승인 범위에서만 수행한다.
