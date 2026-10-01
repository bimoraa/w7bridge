# Fatomic workflow 검증 기록

Mac 원본은 `/Users/hillaryabigail/Documents/ChatGPT/fatomic`, Windows 대상은 독립 mirror `D:/w7bridge/projects/fatomic-mac`다. Mac의 별도 `fatomic-mac` checkout과 Windows 원본 `D:/Downloads/fatomic`은 대상이 아니다. Windows 원본의 shared-worktree `.git`은 교체하지 않는다.

## 설치와 계약

- project ID: `desktop-8ueaoeb__fatomic`, owner command: `check/build/test/run`.
- Mac MCP는 private pairing config를 사용하는 local hub다. daemon은 `com.w7bridge.sync` LaunchAgent다.
- Windows host는 로그인한 같은 SID의 Limited task `w7bridge-fatomic-host`다. task action은 `w7bridge-desktop.exe`를 직접 실행한다. SSH는 console binary의 relay를 사용한다.
- Git과 파일 journal은 pair별로 보존한다. 첫 branch 인계는 검토한 양쪽 state hash와 같은 HEAD/index를 확인했다. 초기 승인 hash는 permanent pairing에서 제거했다.
- `source_dirs`는 실제 source build/target fixture만 승인한다. owner가 검토한 제외된 tracked 파일 192개는 index/object만 인계한다. raw `.git`, `.w7bridge`, `.w7bridge-update`, secret과 output은 파일 공유 대상이 아니다.
- Fatomic command는 owner의 `sync_timeout_seconds = 120`과 immutable check/build/test snapshot을 사용한다. run의 자동 restart는 꺼져 있다.

## source와 증거 경계

`source-manifest.json`의 최신 source ID는 `22ac45684dd9cb1e0ed0329102c54d2947686ed7c732bfe5049fc609c2cc65fa`다. Mac/Windows result와 전체 log의 모든 gate가 exit 0이며 library test는 각각 74개와 75개다. 이 결과는 해당 manifest의 source만 검증한다. 설치 binary hash는 별도 `installed-binaries.json`에 기록한다. CLI와 console 없는 desktop binary를 함께 확인한다.

첫 fixture E2E/streaming은 source `a2b334da104ae2d805f2e22c1527d514a54d46b5d7151ad22eafac35aebfa474`에서 실행했다. `fixture-e2e-result.json`, `streaming-result.json`, `streaming-mcp.jsonl`, `streaming-reconnect-result.json`은 이 source의 원래 결과다. newline 없는 stdout/stderr는 완료 1927 ms 전에 도착했고 병렬 progress token이 분리됐다. 최신 source의 추가 실행은 별도 final fixture 폴더에 기록한다.

첫 browser screenshot은 화면의 app을 확인하지 못했으므로 visual pass로 쓰지 않는다. `fixture-windows-rendered.png`는 실행된 fixture의 실제 HTTP body를 Windows WebBrowser에 표시한 뒤 MCP로 캡처한 화면이다. `Compiled revision: mac-edit-v2`를 pixel로 확인했다. Fatomic 앱 화면의 증거가 아니다.

`source-probe.json`은 실제 Mac 신규 파일이 Windows mirror에 도착한 SHA-256이다. `desktop-stop-result.json`은 direct task stop 후 owned host가 0개인 것을 확인한다. `restart-probe-local.json`의 Mac offline edit가 새 host에서 같은 hash로 도착한 결과는 `desktop-resume-result.json`이다. 이 시험은 owned desktop host restart이며 PC reboot나 Wi-Fi 강제 차단 시험이 아니다.

## 최신 source의 fixture E2E와 realtime

`final-e2e/`는 위 최신 bridge source의 실제 Mac edit `mac-edit-v3` → sync → Windows check/build/test → log → MCP screenshot 결과다. 세 command 모두 exit 0, `revision_verified=true`, source revision `269d3a7695cfd0b716f38be2860372ce6124cde010c874df3e4045dc85d6617c`를 반환했다. HTTP body를 표시한 `windows-verified.png`의 `Compiled revision: mac-edit-v3`를 pixel로 확인했다. 작은 검증 fixture이며 Fatomic 앱 screenshot이 아니다.

`final-streaming/`에서 첫 running handle은 752.61 ms, 완료는 3344.16 ms였다. sync/연결/snapshot을 포함한 request 측정이다. newline 없는 stdout/stderr는 각각 완료 1993.58 ms 전에 도착했다. 병렬 stream/build의 progress notification은 각 6개이며 token과 process ID가 섞이지 않았다. `reconnect-result.json`은 running handle을 받은 뒤 Mac hub/SSH를 닫고 새 연결에서 같은 handle/cursor로 모든 marker와 exit 0을 읽은 기록이다. command를 재실행하지 않았다.

첫 fixture 재실행은 실제 내용이 v3인데 test 기대값이 이전 v2라서 exit 101이었다. 기대값을 v3로 맞춘 뒤 owned host의 새 process store에서 전체 command를 다시 검증했다. bridge source 변경 없이 fixture만 수정했으며 정확한 fixture input hash는 `final-e2e/fixture-manifest.json`에 기록하며 실제 UTF-8 source는 `fixture-inputs.json`으로 보존한다.

## 실제 Fatomic command와 reconnect

`fatomic-acceptance-result.json`과 실제 MCP frame은 Mac hub → fresh sync → Windows immutable snapshot → `check` handle/log/result를 기록한다. 준비 시간은 132.48초이며 100 ms process yield를 전체 request 시간으로 오해하지 않는다. revision `31df90ebc1392c4c55d8e37763a771023069378fe8972a078538ae58b0db7831`의 시작/완료 hash가 같고 `revision_verified=true`다.

native helper가 `C:\ free=16.67 GiB, required=40 GiB`를 stderr로 반환했고 exit code는 1이다. Fatomic Cargo를 실제 실행하기 전에 저장 guard가 거부한 결과이며 build 성공으로 취급하지 않는다. `fatomic-command-reconnect.json`은 새 SSH 연결에서 같은 process ID와 완료 결과를 읽고 cursor 뒤 중복 event가 없는 것을 확인한다. command는 재실행하지 않았다.

150초 제한의 첫 client는 snapshot 준비 중 timeout으로 닫혔다. `fatomic-timeout-recovered.json`은 그 이전 요청의 handle을 조회해 cancelled, 출력 없음, 검증된 revision을 확인한 기록이다. 그 상태 확인 후 별도의 600초 client로 위 시험을 실행했다.

`final-owner-restored.json`은 fixture를 제거한 permanent Fatomic-only config의 SHA-256과 임시 task 삭제를 기록한다. `final-desktop-state.json`에서 최신 binary hash, Session 1의 실행 host, mirror의 `codex/cross-machine-dev`, 원래 Windows의 `codex/bladeball`과 검증 probe 삭제를 확인한다.

## 필요한 사용자 환경

macOS DocumentsFolder TCC 로그에서 background binary의 허가 prompt를 확인했다. Codex의 foreground 권한과 LaunchAgent 권한은 다르다. prompt에서 Documents 접근을 허용해야 로그인 후 자동 sync를 검증할 수 있다. binary의 code identity가 바뀌면 다시 허가가 필요할 수 있다. TCC database를 수정하거나 다른 앱의 권한으로 우회하지 않는다.

현재 Codex MCP transport는 이전 direct-SSH registry를 유지한다. config는 local hub와 `tool_timeout_sec = 600`으로 바뀌었지만 새 MCP 연결/reconnect가 필요하다. 이 timeout은 fresh sync와 source snapshot 준비를 포함하며 process가 시작된 뒤의 100 ms output yield와 다르다. 이전 transport의 project ID와 commands가 새 hub의 ID로 자동 바뀌었다고 보고하지 않는다.

Fatomic 저장 정책은 volume별 30 GiB floor와 10 GiB headroom이다. C가 약 16–17 GiB이므로 실제 Fatomic Cargo/app 실행은 시작하지 않는다. mirror의 owner-local secret config와 frontend dependency 설치도 필요하다. 이 blocker를 fixture build 성공이나 screenshot으로 대신하지 않는다.

## 지원하지 않거나 별도 검증이 필요한 부분

- file tool 경계는 OS build sandbox가 아니다.
- SSH reconnect의 process handle/log는 유지되지만 host crash/restart 후 복원하지 않는다. foreground build/test를 자동 반복하지 않는다.
- signed update engine과 protocol negotiation은 구현돼 있지만 public signed release channel과 desktop-task binary update 배포는 준비되지 않았다.
- 실제 PC reboot, 장시간 offline, Wi-Fi fault와 bandwidth wire-byte 실측은 이번 기록의 live 증거가 아니다.
