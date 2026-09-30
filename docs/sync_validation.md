# 양방향 sync와 service 검증 — 2026-09-30

## 구현과 소유권

새 구현은 기존 registry와 Executor를 재사용한다. Mac daemon 한 개가 pairing별 baseline, journal과 SSH 연결을 소유하고 Windows service 한 개가 MCP 상태, concurrency slot과 process handle을 공유한다. client는 executable, args, cwd, env와 registry를 변경하지 않는다.

| 요청 | 현재 동작 |
| --- | --- |
| 항상 실행하는 양방향 sync | 기본 2초 polling, Mac LaunchAgent와 Windows SCM host |
| project pairing | Mac 절대 root ↔ Windows registry ID, 서로 다른 실제 경로 |
| sync status | synced / syncing / conflict / offline, lease와 CLI owner 확인 |
| conflict | 원본 양쪽 + Mac snapshot 보존, 충돌 상태의 build 거부 |
| smart excludes | Git metadata, dependencies, build/cache/binary artifact 제외 |
| initial sync | 없는 Mac root 생성, Windows 파일 import, 다른 기존 내용은 conflict |
| reconnect | SSH backoff, persisted baseline/journal, root/policy drift 거부 |
| remote build/test/run | 고정된 command allowlist, 선택적인 fresh sync gate |
| live output | managed process의 stdout/stderr event와 cursor |
| process control | start/stop/restart/list, Job Object 전체 종료 |
| context files | AGENTS/MEMORY/PLANS/custom 파일, Git tracking과 무관 |
| Windows service | LocalService, automatic SCM start와 crash recovery |

운영 설정과 설치 command는 [README](../README.md)에 있다. 파일 전송은 파일당 1 MiB, 10,000개, manifest 전체 256 MiB로 제한한다. 빈 디렉터리와 permission bit는 복제하지 않는다. editor가 bridge advisory lock을 따르지 않는 짧은 check/replace race는 OS sandbox나 원자적인 두 기기 transaction으로 보장하지 않는다.

## 최종 source와 검사

- source/fixture/manifest/formatter 입력 105개 파일의 snapshot SHA-256: `77ddcd4b73b2bc9f1e493446d8e38ea04ca56ff43f87abc4a879b12407f45793`. Mac 파일별 hash와 Windows native build 입력의 hash가 전부 일치했다.
- Windows MSVC debug binary SHA-256: `d3257ba231f28472d9ac805d911903a867b1d0df92738f89e5107c3a77f3516b`. 로컬 검토용 복사본은 `target/native-windows/w7bridge.exe`에 있다. release artifact는 아니다.
- macOS: formatter와 반복 check, workspace check, Clippy -D warnings, workspace test 통과. 49 passed / 5 ignored. ignored 항목은 subprocess helper 4개와 native desktop capture 1개다. capture는 별도 --include-ignored 실행으로 실제 desktop의 2개 integration test를 통과했다.
- Windows MSVC native: 같은 formatter check, workspace check, Clippy -D warnings, workspace test와 binary build 통과. 46 passed / 5 ignored. capture integration --include-ignored의 2개 test도 별도로 통과했다.
- Windows GNU target의 workspace/all-targets Clippy -D warnings 통과. 이 결과는 cross-target 정적 검증이며 위 MSVC native 실행과 구분한다.
- 기존 unit suite에 새 sync coverage를 더해 lost write reply, 불확실한 결과 후 양쪽 새 수정, 재시작, identity/policy drift, corrupt journal, case/file-directory 충돌과 edit/delete 충돌을 검사했다.
- tests/와 Cargo.lock의 기존 Git 제외 정책은 유지했다. 이 입력을 포함한 실제 snapshot에서 검증했으며 새 checkout/CI에서 ignored 입력의 재현성은 아직 확인하지 않았다. commit/push하지 않았다.

## 실제 Mac ↔ Windows SSH 흐름

Windows DESKTOP-8UEAOEB의 격리된 임시 Rust project와 service를 사용했다. 프로젝트의 빌드 자체가 Windows에서 실행됐고 source, context, output과 process 수명은 Mac에서 MCP로 확인했다. Windows에만 파일이 있는 상태에서 시작했다. 아래 시간은 interval_seconds=1인 작은 fixture 한 번의 관찰값이며 대형 project 성능이나 SLA가 아니다.

- `windows_initial_import_context_and_excludes`: 통과, 0.63초.
- `mac_to_windows_save`: 통과, 1.065초.
- `windows_to_mac_save`: 통과, 0.629초.
- `windows_build`: 통과, 2.441초, exit 0.
- `windows_test`: 통과, 1.127초, exit 0.
- `run_live_stdout_stderr_before_completion`: 통과.
- `process_reconnect_restart_stop`: 통과.
- `simultaneous_edit_preserves_both_and_blocks_build`: 통과.
- `conflict_resolved_by_equal_content`: 통과.
- `scm_stop_job_tree_cleanup_zero_exit`: 통과.
- `service_stopped_sync_offline`: 통과.
- `service_restart_auto_reconnect_and_resume`: 통과.
- `mac_launchagent_install_stop_start_uninstall`: 통과.
- `mac_launchagent_recovers_daemon_crash`: 통과, 5.596초.
- `scm_recovers_service_crash_reaps_job_and_sync_resumes`: 통과, 35.909초.
- `connect_service_ssh_handshake_and_registry`: 통과.
- `final_binary_all_context_files_bidirectional_cli_status_and_build`: 통과, exit 0.

MEMORY.md가 .gitignore에 있어도 전달됐고 AGENTS.md, PLANS.md와 custom context의 양쪽 editor 변경도 실제로 전달됐다. 제외된 marker와 Windows target output은 Mac에 생기지 않았다. 동시에 편집한 MEMORY.md는 두 원본과 snapshot을 유지하며 build를 차단했다. 원본 내용을 같게 만든 뒤 다음 round에서 충돌을 해제했다.

live output은 Windows 앱이 아직 running일 때 stdout/stderr를 받았다. SSH client를 닫은 뒤에도 service handle로 다시 조회하고 restart/stop했다. SCM stop과 강제 service 종료 모두 기존 앱이 남지 않았고, 정상 SCM stop의 Win32 exit code는 0이었다. recovery 설정은 5/10/30초 restart와 86400초 reset으로 실제 조회했다. 이번 crash 시나리오는 앞선 설정 실패의 recovery count가 남아 약 35.9초 뒤 sync/build까지 복구됐다.

처음 LocalService에서 사용자 전용 cargo 경로의 ACL과 MSVC linker 환경이 없어서 fixture 시작/build가 실패했다. 기존 사용자 .cargo/.rustup의 권한을 넓히지 않고 임시 전용 toolchain/cache를 준비했으며 linker/SDK library 경로를 local command env에 명시한 뒤 같은 흐름을 통과했다. service는 이 권한이나 toolchain 환경을 자동 설정하지 않는다.

## 마지막 screenshot와 운영 경계

최종 snapshot에서 Mac 3024×1964 PNG 1,500,495 byte와 Windows 2560×1440 PNG 2,391,527 byte를 실제 MCP로 받았고 desktop 내용도 확인했다. capture 요청은 별도 opt-in 사용자 stdio endpoint다. LocalService가 로그인 desktop을 대신 캡처하지 않는다. capture의 임시 task/folder/worker는 모두 0개로 확인했다. 세부 계약은 [PLANS](../PLANS.md)의 capture 기록에 있다.

- 임시 Windows service와 fixture, Mac LaunchAgent를 검증 후 제거했다. service/fixture/process와 Mac job/plist/binary가 없는 것도 확인했다. 기존 C:/w7bridge binary/config와 기본 Codex MCP 등록은 교체하지 않았다.
- 실제 fatomic pairing은 아직 활성화하지 않았다. 지정 예시 /Users/hillaryabigail/Developer/fatomic와 C:/dev/fatomic는 검증 시 존재하지 않았다. 실제 root와 명령/toolchain 설정을 정해야 운영 설치를 연결할 수 있다.
- 전체 PC reboot, 전원 종료 후 시작, Mac logout/login, 실제 fatomic build/test 및 새 Codex 채팅의 최신 service 도구 호출은 아직 확인하지 않았다. 이번 증거는 native daemon/service lifecycle와 실제 SSH MCP다.
- Windows service의 GUI 앱은 Session 0에서 실행한다. process 제어가 로그인 desktop에 창을 표시하는 기능을 의미하지 않는다.
