# Frontend 준비

## 현재 범위

사용자가 전달한 annotation과 세로 tab rail 참조를 기준으로 빈 화면 shell을 구현했다.
React entry, macOS control preview, 빈 topbar, 세로 tab 선택과 빈 content panel을 제공한다.
dashboard, 모니터링 동작과 Windows 연결은 아직 구현하지 않았다.

현재 repository는 Rust 서버와 개발 도구 `xtask`로 구성되어 있다.
`frontend/`에는 Bun manifest와 lockfile, Vite/TypeScript 설정과 `src-tauri/` host가 있다.
개발 Mac에 Node.js, npm과 Bun 실행 경로가 있으며 Xcode와 Rust toolchain도 확인했다.
2026-09-30 확인 시 작업 volume의 여유 공간은 약 33 GiB다. dependency 설치 전 다시 확인한다.

## Package와 연결 경계

사용자가 macOS application을 먼저 만들고 Windows를 모니터링하도록 지정했다.
Tauri 2.12, React 19.3, TypeScript 7, Vite 8과 Bun 1.3.14로 `frontend/`를 준비한다.
Rust desktop host는 Tauri 2.12 요구사항에 따라 Rust 1.90 이상을 사용한다.
서버 root package의 Rust 1.88 계약은 변경하지 않는다.
`src-tauri/`는 독립 Cargo workspace이며 서버 workspace의 기본 build 대상에 추가하지 않는다.
Rust 서버는 root package가 계속 소유하고 `xtask`는 개발 도구만 소유한다.
`xtask`의 기존 최종 formatter는 desktop host의 `src/`와 `build.rs`도 검사한다.

기존 서버의 MCP stdio transport를 browser에서 직접 사용할 수 있다고 가정하지 않는다.
desktop host 또는 web용 연결 계층의 책임과 인증을 정한 뒤 연결한다.
frontend가 executable, args, cwd, env와 registry를 임의로 수정하는 API는 추가하지 않는다.
기존 기능 구현과 예약 모듈은 이번 준비 작업에서 변경하지 않는다.

## 임시 색상 token

기준은 사용자가 지정한 Codex의 Linear theme 분위기다.
현재 Codex 화면은 UI 접근 제한 때문에 확인하지 못했다.
아래 값은 실제 theme에서 추출한 공식 값이 아니라 design 전달 전의 제안이다.

| Token | 색상 | 용도 |
| --- | --- | --- |
| `background` | `#141414` | 기본 화면 |
| `surface` | `#1B1B1B` | 보조 surface |
| `surface_raised` | `#222222` | menu와 dialog |
| `surface_hover` | `#2A2A2A` | hover와 선택 배경 |
| `border` | `#333333` | 얇은 경계 |
| `text_primary` | `#EEEEEE` | 기본 텍스트 |
| `text_secondary` | `#A0A0A0` | 보조 텍스트 |
| `accent` | `#5E6AD2` | 주요 action과 focus |

dark neutral 배경을 기본으로 하고 accent는 필요한 곳에만 사용한다.
사용자의 수정 요청에 따라 topbar와 sidebar는 하나의 glass window frame을 공유한다.
두 영역은 투명하고 frame만 tint와 blur를 소유한다. content panel은 불투명한 `#141414`다.
browser는 평면 배경 위에서 frame의 tint와 CSS backdrop blur를 preview한다.
browser에서 desktop wallpaper 자체를 blur하지 않으며 취소된 배경 이미지는 사용하지 않는다.

기존 Mac 프로젝트에서 [Central Icons collection](central_icons.md)을 가져왔다.
`frontend/public/central-icons/`에 fill 2,037개, reversed 1,996개와 프로젝트 추가본 19개가 있다.
tab rail은 이 collection의 SVG를 CSS mask로 사용한다. 하단 avatar는 user icon placeholder다.

## 다음 checkpoint

`src/main.tsx`가 `src/app_shell.tsx`의 shell을 표시한다.
topbar 높이와 rail 너비는 각각 52px이고 panel은 남은 공간을 채운다.
panel radius는 20px, 오른쪽과 아래 여백은 6px이다.
document root는 viewport에 고정하고 두 축의 overflow와 overscroll을 차단한다.
rail은 높이가 부족할 때만 내부 세로 scroll을 허용하며 frame으로 scroll을 전파하지 않는다.
tab은 click과 위/아래 방향키, Home/End로 선택한다. 선택한 tab의 content는 현재 모두 비어 있다.
browser의 traffic light는 시각적 preview이며 실제 창 control 동작은 제공하지 않는다.

macOS 전용 `tauri.macos.conf.json`은 투명 webview와 `sidebar` native vibrancy를 설정한다.
native titlebar는 overlay로 두고 OS traffic light를 사용하므로 HTML의 점은 native Mac에서 숨긴다.
topbar drag permission은 main macOS 창에만 부여한다.
Tauri의 투명 macOS webview를 위해 `macos-private-api` feature와 config를 활성화했다.
이 설정은 App Store 제출과 호환되지 않는다. native blur의 실제 결과는 별도로 확인해야 한다.
Tauri가 요구하는 `icons/icon.png`는 단색 placeholder이며 최종 app icon이 아니다.
Windows monitoring은 기존 SSH/MCP 연결 owner를 검토한 뒤 구현하며 현재 shell에는 연결하지 않았다.

`frontend/`에서 다음 command를 사용한다.

```sh
bun install --frozen-lockfile
bun run typecheck
bun run build
bun run tauri info
bun run desktop:dev
bun run desktop:build
```

`desktop:dev`는 현재 shell을 macOS window에서 실행하는 command다. 실제 실행 여부는 아래 검증 기록과 구분한다.
`desktop:build`는 unsigned native binary만 build하며 DMG, signing과 release는 만들지 않는다.
TypeScript 설정은 root 지침의 한국어 문서 계약과 네 칸 들여쓰기를 따른다.
TS/TSX는 custom padding/alignment를 수동으로 유지했다. editor/CI formatter 연결은 아직 준비 단계다.

## 검증 기록

- Bun dependency 설치, TypeScript typecheck와 Vite build는 통과했다.
- shell 수정 뒤 `bun run build`가 통과했다. browser의 1010×799 viewport에서 overflow가 없고
  topbar와 rail의 배경이 투명하며 panel이 비어 있는 것을 확인했다.
- browser에서 tab click, 아래 방향키와 Home/End 선택을 확인했다.
  해당 확인 중 console warning/error는 없었다.
- root scroll 수정 뒤 Vite build가 통과했다. 1010×799 browser에서 상하좌우 scroll gesture 후
  window, document와 body scroll offset은 모두 0이고 frame 위치도 `(0, 0)`을 유지했다.
- `tauri dev`의 macOS binary build와 process 실행을 확인했다. 실제 native trackpad bounce 수정 결과는
  아직 직접 검증하지 않았다.
- macOS의 `cargo check --locked --manifest-path frontend/src-tauri/Cargo.toml --all-targets`와
  같은 manifest의 Clippy `--all-targets -- -D warnings`는 통과했다.
  glass 설정을 추가한 뒤 두 command를 다시 실행해 통과했다.
- `bun run tauri info`에서 Xcode/Rust 환경과 Rust/JavaScript Tauri 2.12 version 일치를 확인했다.
  `bun install --frozen-lockfile` 재실행도 변경 없이 통과했다.
- `cargo test --locked -p xtask`의 formatter 멱등성 test 1개와
  `cargo clippy --locked -p xtask --all-targets -- -D warnings`는 통과했다.
- root formatter check는 기존 서버와 test 파일들의 formatting 차이 때문에 실패했다.
  새 desktop Rust 파일과 수정한 `xtask` 파일에는 formatting diagnostic이 없었다.
  다른 작업의 source를 일괄 format하지 않았다.
- native window interaction, Windows 연결과 모니터링, Windows용 app, DMG와 signing은 아직 검증하지 않았다.

## 참고

[Tauri Vite 설정](https://v2.tauri.app/start/frontend/vite/),
[Tauri project 구조](https://v2.tauri.app/start/project-structure/),
[macOS prerequisites](https://v2.tauri.app/start/prerequisites/),
[window effects와 투명 창 설정](https://v2.tauri.app/reference/config/#windowconfig)을 참고한다.
