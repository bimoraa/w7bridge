# Central Icons 수집

2026-09-30에 사용자가 Mac의 기존 프로젝트들에서 Central Icons를 모두 가져오도록 요청했다.
새 icon을 생성하거나 원본 프로젝트의 asset을 수정하지 않고 SVG를 복사했다.

## 결과

| 구분 | SVG 파일 수 | 위치 |
| --- | --- | --- |
| fill | 2,037 | `frontend/public/central-icons/fill/` |
| reversed | 1,996 | `frontend/public/central-icons/reversed/` |
| 프로젝트 추가본과 다른 버전 | 19 | `frontend/public/central-icons/project-variants/` |
| 전체 | 4,052 | `frontend/public/central-icons/` |

서로 다른 SVG 내용은 SHA-256 기준 4,040개다.
기본 collection의 파일 이름은 그대로 유지했다. 이름이 다른 동일 내용의 icon도 기존 이름으로 사용할 수 있다.
다른 프로젝트에서 발견한 추가본과 수정 버전은 `project-variants/`에 이름과 hash를 붙여 보관했다.
추가본의 Central Icons 공식 출처 여부는 확인하지 않았으며 기본 collection과 구분한다.

## 출처와 범위

`Documents/`, `Downloads/`, `.codex/worktrees/`와 home의 다른 개발 directory 21곳을 검색했다.
이름에 `central-icons`, `central_icons` 또는 `centralicons`가 포함된 경로에서 SVG를 수집했다.
`.git`, `node_modules`, `target`, `.next`, `dist`, `build`와 `.venv` 내부는 검색에서 제외했다.
Library와 개인 media directory는 프로젝트 검색 범위에 넣지 않았다.
프로젝트 README, license 파일과 package manifest에서 Central Icons 표기도 검색했다.
검색된 코드 wrapper와 test source는 asset collection에 복사하지 않았다.

41개 asset directory에서 SVG 사본 71,003개를 확인했다.
내용이 같은 프로젝트 사본을 중복 복사하지 않고 모든 다른 SVG 내용을 보존했다.
기본 collection은 현재 `my-codex/public/central-icons-fill/`과
`my-codex/public/central-icons-reversed/`에서 가져왔다.
추가 내용 19개는 `fatomic`의 현재 frontend와 이전 UI archive에서 발견했다.

[수집 manifest](central_icons_manifest.json)에 모든 원본 directory, destination path,
내용 hash와 원본 파일 이름을 기록했다. 같은 내용의 여러 원본 위치도 유지한다.
manifest는 `docs/`에 있어서 frontend의 public asset과 함께 배포되지 않는다.

## 사용 경로

Vite가 public directory를 그대로 복사하므로 다음 URL로 기존 SVG를 사용할 수 있다.

```text
/central-icons/fill/settings-gear-1.svg
/central-icons/reversed/settings-gear-1.svg
```

React component, icon gallery와 화면은 아직 구현하지 않았다.
SVG의 `currentColor`, stroke, viewBox와 원본 byte를 변경하지 않았다.

## 검증

- 복사한 모든 SVG를 XML로 parse하고 SVG root를 확인했다.
- 원본과 destination의 SHA-256이 같은지 확인했다.
- 검색된 모든 SVG 내용이 destination에 포함되는지 확인했다.
- manifest의 원본 항목 총수가 검색된 사본 71,003개와 같은지 확인했다.
- `bun run build`의 TypeScript typecheck와 Vite build는 통과했다.
- build output의 SVG 4,052개도 원본 hash와 같고 asset 누락이 없는지 확인했다.
