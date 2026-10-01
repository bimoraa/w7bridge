# 전체 기능 수용 기준 — 진행 중

작업 branch는 `codex/sync-safety-e2e`, 시작 commit은 `5d61bad`다. 진행 상태는 구현 완료나 운영 검증을 의미하지 않는다. 이전 [sync 검증](sync_validation.md)은 이전 snapshot의 증거이며 이번 source의 증거로 재사용하지 않는다.

| 기능 | 구현 상태 | 필수 검증 |
| --- | --- | --- |
| fresh sync 후 build/test | peer별 fresh generation gate + snapshot revision 확인 | 최신 Mac revision, conflict, offline, 취소 |
| stdout/stderr와 process handle | run_command 기본 첫 출력/100 ms 반환, progress notification, bounded ring + long-poll/cursor | 완료 전 양 stream, cursor replay, 요청별 token 분리, 연결 종료와 복구 |
| reconnect/resume | 기존 daemon backoff와 durable journal | 실제 SSH 단절, reconnect, 응답 유실 |
| 인증/암호화 | SSH key/host key, forwarding 차단 | trusted key 성공, 잘못된 host key/key 거부 |
| project file 경계 | 기존 정책 + Windows reparse point 차단 | traversal, symlink/junction, 미등록 root |
| smart ignore | 기존 공통 정책 | Git metadata, build/cache 제외, context 포함 |
| cancel/timeout/restart | Job/process group + background opt-in restart | 전체 자식 종료, build/test 재실행 금지 |
| crash/initial sync 안전 | 기존 journal/conflict snapshot | crash 후 hash 일치, 기존 양쪽 원본 보존 |
| E2E | 실제 Mac edit → Windows check/build/test → log → screenshot 통과 | 최신 revision/hash, exit code, 실제 화면 증거 |
| command preset | Cargo/Node owner opt-in 구현 | 명시 command 우선, 무권한 discovery 거부 |
| folder discovery | 제한된 root/depth scan 구현 | Cargo/Node 탐색, symlink와 artifact 제외 |
| 여러 device | pair별 journal + device/project ID + peer checkpoint 분리 | 복수 peer, 중복 remote/root 거부, 양방향 handoff |
| bandwidth control | 양방향 payload throttle 구현 | 양방향 실제 전송량/시간, 취소 |
| file priority | source/context 우선, 작은 파일 우선 구현 | 작은 source 우선, 지속 변경에도 large file 진행 |
| delta/chunk sync | 64 KiB chunk + SHA-256 구현 | 변경 chunk만 전송, 최종 SHA-256 |
| transfer resume | durable missing-chunk journal 구현 | 끊긴 offset/chunk부터 재개, peer restart |
| rename/move | 동일 hash content 재사용 구현 | 같은 hash 재사용, conflict와 stale target 거부 |
| history/rollback | bounded revision history + expected-hash restore 구현 | 이전 내용 조회, expected hash로 restore |
| health/events | project_status + bounded event stream 구현 | device/peer/sync/process/error, cursor/boot ID |
| automatic update | 서명된 HTTPS updater + helper + recovery 구현 | 신뢰한 artifact, config 보존, 실패 rollback |
| protocol version | capability 조회와 공통 version 선택 구현 | 공통 version 선택, 지원하지 않는 peer 거부 |
| Git 자동 import | bundle/staged objects + metadata backup 구현 | bundle/clone, history/HEAD/branch/origin, 대상 existing 보존 |
| Git handoff 양방향 | pair baseline + CAS/WAL 구현 | staged/unstaged/untracked, rename/delete, offline 후 resume |

## 증거 계약

- macOS gate, cross-target, Windows-native gate, live MCP/SCM, 두 기기 E2E를 별도 기록한다.
- source snapshot hash, OS, 시각, 실행 command, exit code, 전체 로그와 fixture를 보존한다. credential과 private config는 포함하지 않는다.
- screenshot은 실제 MCP 결과를 확인한다. build 결과는 exit code, 로그와 revision/hash로 별도 검증한다.
- 실제 PC reboot/장시간 전원 종료는 실행 전 사용자의 별도 조율이 필요하다. service/transport simulation을 실제 reboot나 7시간 offline 검증으로 쓰지 않는다.
- file tool 경계는 OS sandbox가 아니다. 현재 범위에서는 agent의 file access를 제한하며 build/script의 OS 격리는 구현되지 않았다.
- Git metadata를 live-sync하지 않는다. 대상 existing repo를 reset/overwrite하지 않고 credential·사용자 identity를 복사하지 않는다. 자동 commit/push하지 않는다.

## 2026-09-30 확인된 결과

- macOS formatter/check/Clippy/workspace test와 build는 통과했다. library test 65개가 통과했고 integration/xtask suite도 통과했다. subprocess fixture와 platform 전용 ignored test는 별도이며 pass로 합산하지 않는다.
- Windows-native source `0c0074ed2a4a8444b0727d9fc629ef5bdd8f0fad306c4751abb473fa0051969c`의 모든 gate와 build가 통과했다. `windows-native-result.json`의 모든 exit code가 0이고 전체 `windows-native.log`를 보존한다.
- 이 source는 headless relay의 Ctrl-C listener 오류를 종료 신호로 처리하지 않는다. 당시 manifest의 모든 source hash와 결과 exit code를 확인했다. 이후 realtime 출력 변경의 검증은 별도 snapshot으로 기록한다.
- Windows lock contention과 Git extended path 문제를 native test에서 수정했다. junction fixture는 실제 PowerShell Junction 생성으로 검증하며, install fixture는 Codex discovery를 꺼 실제 사용자 project를 읽지 않는다.
- peer별 fresh sync gate의 command forwarding regression을 실제 MCP transport로 검증한다. 다른 peer checkpoint로 통과하지 않으며 RPC error를 connection loss로 바꾸지 않는다. peer 대기 경로도 16개 한도를 적용한다.
- 임시 Windows SCM service가 실제 실행되고 MCP read_events/list_processes/project_status 응답이 돌아왔다. Windows에만 있던 fixture source는 Mac으로 import됐다.
- SSH relay가 약 0.3초 뒤 exit 0으로 닫히는 현상은 사용자가 승인한 OpenSSH service restart 이후 재현되지 않았다. 3초 Python probe가 양쪽 출력을 반환했고 MCP long-poll도 유지됐다. restart 전후 SCM 상태는 `windows-sshd-restart.json`에 기록한다. Wi-Fi 원인으로 확정하지 않는다.
- 실제 Mac 수정 `mac-edit-v2`가 Windows로 sync됐다. check/build/test 모두 exit 0, `revision_verified=true`, 동일한 시작/완료 hash `79e66fa1039431a0905f35c70de5db6a81b7789e2caaaaf6f259a674d853a0d7`를 반환했다. stdout/stderr stream과 cursor replay도 확인했다.
- hub transport를 닫고 다시 연결한 뒤 기존 check/build/test/run handle을 조회했다. 완료된 build/test를 다시 실행하지 않고 기존 결과를 읽었으며 running app의 log도 유지됐다. 실제 Wi-Fi 차단 시험과는 구분한다.
- 실제 Windows browser의 HTTP 응답과 MCP screenshot에서 `Compiled revision: mac-edit-v2`를 확인했다. `windows-verified.png`는 2560×1440 화면이며 `e2e-result.json`과 `e2e-mcp.jsonl`에 exit code, log, revision, screenshot metadata, app 취소 결과를 보존한다. screenshot만으로 build 성공을 판정하지 않는다.
- fixture는 독립 Cargo workspace를 명시한다. Python MCP reader는 screenshot JSON frame을 위해 16 MiB까지 허용하고 resume 시 기존 running app을 재사용한다. 임시 SCM service와 검증 task만 제거했으며 permanent installation과 fixture source/evidence는 보존했다.

## 2026-10-01 realtime 출력 검증

- source `1e57f0ec4d50cec70c91f7aa4380a641ffc518fc2c39747197976c4e41d94389`의 macOS formatter/check/Clippy/workspace test/build와 Windows-native의 같은 gate가 모두 통과했다. [source manifest](evidence/2026-10-01/source-manifest.json), [Mac 결과](evidence/2026-10-01/macos-result.json), [Windows 결과](evidence/2026-10-01/windows-native-result.json)와 [전체 native log](evidence/2026-10-01/windows-native.log)를 보존한다.
- 실제 Windows SCM service → named pipe relay → SSH → Mac hub에서 `run_command`가 완료 전 `running` handle을 반환했고 cursor로 stdout/stderr를 이어 읽었다. 첫 응답 4701 ms, 완료 6957 ms는 sync·연결·snapshot 준비를 포함한 전체 요청 시간이다. 100 ms 한도는 process가 시작된 뒤 적용되며 전체 요청 latency를 100 ms로 보장하지 않는다.
- `wait=true`의 newline 없는 stdout/stderr가 각각 완료 1987 ms 전에 도착했다. 별도 stream/build 요청을 동시에 실행해 progress token, project ID와 process ID가 섞이지 않는 것을 확인했다. 두 exit code가 0이며 source snapshot의 시작/완료 revision이 같고 `revision_verified=true`다. [측정 결과](evidence/2026-10-01/streaming-result.json)와 [실제 MCP frame](evidence/2026-10-01/streaming-mcp.jsonl)을 보존한다.
- SDK가 생성한 실제 request handle의 progress token을 사용하고 notification extension metadata를 보존한다. 기존 peer checkpoint regression에서 실제 MCP transport를 통한 progress forwarding도 확인한다. 첫 live 시도의 token forwarding 실패는 `before-token-fix-*` 증거에 별도로 남겼다.
- 기본 run_command가 running handle을 반환한 뒤 hub transport를 닫고 다시 열었다. 같은 handle과 cursor로 모든 출력과 exit 0 결과를 읽었으며 command를 재실행하지 않았다. [reconnect 결과](evidence/2026-10-01/streaming-reconnect-result.json)와 [MCP frame](evidence/2026-10-01/streaming-reconnect.jsonl)을 보존한다. service restart 시험과는 구분한다.
- screenshot은 이전 visual E2E 증거다. 이번 변경은 command 출력 경로이며 새 screenshot이나 실제 PC reboot 검증으로 취급하지 않는다. Codex의 notification UI 표시와 설치된 permanent binary 교체도 이번 검증 범위에 포함하지 않는다.
- 검증 종료 뒤 owned SCM service가 없는 상태(1060)를 확인하고 임시 native/fixture task를 삭제했다. permanent 설치와 project source/evidence는 보존했다.

## 2026-10-01 Fatomic 파일 권한 활성화

- 기존 `C:/w7bridge/w7bridge.toml`은 `projects = []`여서 Codex discovery에 보이는 Fatomic에도 file 권한이 없었다. 사용자가 지정한 `D:/Downloads/fatomic`만 기존 Codex ID `01a04c82-08c7-7ed3-addd-ff5358be7cf4`로 등록했다.
- native 검증된 binary SHA-256 `f7892cfd55db0ccd4d6c4d85db886627b844136786467c83442fc7e0812f40f4`를 `C:/w7bridge/w7bridge-f7892cfd.exe`에 배치하고 owner config `C:/w7bridge/fatomic.toml`을 만들었다. Mac MCP command도 이 binary/config로 변경했다. 기존 executable과 활성 세션은 종료하지 않았으며 이전 config backup을 보존했다.
- owner file 설정은 `enabled=true`, `max_file_bytes=67108864`, `exclude_dirs=["work", ".serena/cache"]`다. trace/archive와 cache는 scan에서 제외하며 원본은 삭제하지 않는다. 큰 source asset의 목록에는 `list_files`의 `protocol_version=2`를 사용한다. legacy version 1은 1 MiB를 넘는 파일을 거부한다.
- 설치된 executable을 통한 실제 SSH/MCP에서 8728개 목록, `package.json` 읽기, 신규 probe의 write/read와 expected-hash 삭제가 통과했다. [검증 결과](evidence/2026-10-01/fatomic-file-access-result.json)를 보존한다. 이 검증은 파일 접근이며 Fatomic build/test 또는 sync pairing 검증은 아니다.
- 현재 Codex chat의 기존 MCP transport는 closed 상태였다. UI 제어 도구가 Codex 접근을 차단했으므로 client의 reload/reconnect는 완료하지 못했다. 새 MCP 연결에는 변경된 Mac 설정이 적용된다.

## 2026-10-01 Fatomic branch와 permanent hub

- 최신 source `22ac45684dd9cb1e0ed0329102c54d2947686ed7c732bfe5049fc609c2cc65fa`의 macOS와 Windows-native formatter/check/Clippy/workspace test/build가 모두 exit 0이다. library test는 Mac 74개, Windows 75개이며 integration/xtask 결과와 ignored test는 전체 log에서 별도로 확인한다. [검증 기록](evidence/2026-10-01/fatomic-workflow/README.md)에 source manifest와 설치 binary hash를 연결한다.
- Mac 원본 `Documents/ChatGPT/fatomic`의 `codex/cross-machine-dev`를 독립 Windows mirror `D:/w7bridge/projects/fatomic-mac`에 인계했다. 원래 Windows `D:/Downloads/fatomic`의 `codex/bladeball`과 shared-worktree metadata는 보존한다. 별도 Mac `fatomic-mac` checkout은 건드리지 않는다.
- `.git`을 파일 공유하지 않고 검토한 initial state, Git bundle/index object, pair baseline과 CAS/WAL로 branch/history/staging을 인계한다. 기존 object는 incremental archive에서 제외하고 owner가 승인한 제외 tracked path는 index만 유지한다. approved source directory 예외도 secret, dependency와 custom ignore를 우회하지 않는다.
- permanent project ID는 `desktop-8ueaoeb__fatomic`, command는 `check/build/test/run`이다. 로그인 SID를 확인하는 console 없는 desktop host와 owner-only named pipe를 사용한다. 기존 direct-SSH Codex transport는 reload/reconnect가 필요하다. Mac config의 tool timeout은 600초이며 process 시작 후 output yield와 별도다.
- 실제 host를 중지한 동안 Mac 파일을 수정하고 같은 watcher가 다시 연결한 뒤 Windows hash가 일치했다. PC reboot 시험은 아니다. SSH reconnect의 handle을 보존하지만 host restart 뒤 process store 복원은 지원하지 않는다.
- Fatomic command는 owner의 120초 fresh-sync gate와 immutable source snapshot을 사용한다. shared read lock으로 watcher와 snapshot 읽기를 함께 허용하고 file write는 exclusive lock으로 막는다. 기존 foreground command를 자동 재실행하지 않는다.
- 최신 source의 작은 fixture E2E에서 Mac `mac-edit-v3`가 sync되고 Windows check/build/test가 모두 exit 0과 같은 verified revision을 반환했다. 실제 HTTP body와 MCP screenshot의 v3를 확인했다. realtime 첫 handle은 752.61 ms이며 양 stream은 완료 1993.58 ms 전에 도착했다. 병렬 progress token 분리와 running command의 reconnect/cursor resume도 통과했다. Fatomic 앱 검증과는 구분한다.
- Fatomic 저장 정책은 각 관련 volume의 30 GiB floor + 10 GiB headroom이다. C의 공간과 mirror의 owner-local secret/frontend dependency 때문에 실제 Fatomic app build/화면 검증은 아직 완료하지 않았다. macOS background Documents 허가는 별도로 필요하다.

## 남은 검증과 지원 경계

- 실제 네트워크 단절/장시간 offline/reboot, service crash 뒤 recovery, bandwidth의 실측 전송량과 signed HTTPS release update는 live 검증되지 않았다. 해당 안전성은 chunk/journal/history/updater 자동 test와 구분한다.
- process handle/log는 SSH reconnect 동안 유지된다. service restart 후 이전 process/log를 복원하는 persistent process store는 구현되지 않았다. foreground command를 자동 재실행하지 않는다.
- OS build sandbox, 기존 linked-worktree metadata 교체, 공개 update channel은 제공되지 않는다.

## 재현 도구

- `xtask/scripts/snapshot.py`: Cargo.lock, source, tests, formatter와 fixture input의 SHA-256 manifest/archive.
- `xtask/scripts/verify_windows.py`: 기존 owned source의 hash를 확인한 후 Windows-native gates, 전체 log, exit code와 binary SHA-256을 기록한다. Task Scheduler는 검증 중 임시 on-demand task로 사용한다.
- `xtask/scripts/fixture_windows.py`: 기존 service가 없는 경우에만 owned root에 fixture/service를 만든다. stop/reinstall도 등록된 owned root/config를 확인한다.
- `xtask/scripts/verify_e2e.py`: initial import, Mac edit, named command handle/cursor/result/revision과 MCP screenshot을 검증한다. reconnect recovery는 read-only 요청만 retry하고 결과가 불명확한 command를 자동 반복하지 않는다.
- `xtask/scripts/render_windows.py`: 실행된 app의 실제 HTTP body를 확인한 후 Windows browser를 연다.
- `xtask/scripts/verify_streaming.py`: 실제 service/SSH/hub를 통해 빠른 run_command handle, newline 없는 stdout/stderr의 완료 전 도착과 병렬 요청의 progress token 분리를 측정한다.
