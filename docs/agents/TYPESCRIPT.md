# 범용 TypeScript 규칙

> 개인 엔지니어링 정책 · 한국어 · 2026-09-28

## T01 — 범위와 소유권

먼저 [AGENTS.md](AGENTS.md)를 읽는다. 프로젝트 소유 `.ts`와 `.tsx` 안의 TypeScript에 적용한다. JSX와 프레임워크별 컴포넌트 규칙은 [TSX.md](TSX.md)를 읽는다. browser, server, CLI, shared library, build tooling 중 실제 runtime과 package 구조를 따른다.

기능 전용 API, selector, type, private helper는 해당 기능 안에 둔다. 실제로 공유되는 transport, state, contract는 한 곳에서 소유한다. React, desktop bridge, 새 store, 특정 폴더 이름을 강제하지 않는다.

## T02 — Naming과 호환성

프로젝트 소유 일반 함수, local, parameter, field, type alias, interface는 소문자 `snake_case`를 쓴다. 모듈 수준 내부 상수는 `__snake_case`, frontend feature/component 파일은 `snake_case.ts` 또는 `.tsx`를 쓴다. 다른 TypeScript source filename은 의미가 분명한 `snake_case`로 한다. 대문자 상수와 임의의 세 개·네 개 underscore는 만들지 않는다.

import한 symbol, 표준 runtime API, 외부 type, framework entry filename, generated declaration, 환경 변수 이름, 공개 계약, 직렬화 key를 유지한다. 내부 naming이 다르면 명시적 adapter를 쓴다. 정리하면서 public API 이름을 몰래 바꾸지 않는다.

component와 Hook naming의 호환성 예외는 [TSX.md](TSX.md)에 있다. `useState`, `addEventListener`, `className`, 기존 외부 callback 계약을 rename하지 않는다. 소문자 정책은 소유한 identifier에 적용하며 모든 대문자 token에 적용하지 않는다.

G03의 한국어 주석 계약을 따른다. 프로젝트 소유 exported API는 `/** ... */`와 TSDoc 호환 구조로 문서화한다. 짧은 summary, 추가 동작이나 lifecycle 상세를 위한 선택적 `@remarks`, 필요한 `@param`, `@returns`, `@throws`, `@example` block을 사용한다. tag와 identifier 철자는 정확히 유지하고 본문은 한국어 대화체 반말로 쓴다. 이름만 반복하는 `/**begin ... */` marker는 API 문서를 대신하지 못한다. [3][4]

실제 parameter에는 `@param name - description`을 사용하고 의미나 제약을 설명하되 TypeScript type을 `{string}`, `{number}`로 반복하지 않는다. 관련 있는 absence, 순서, mutation/reference 의미, side effect, async 실패/cancellation, retry 조건을 설명한다. `@throws`는 실제 exception 조건에 사용한다. 동기 throw와 promise rejection을 구분하고 후자는 `@returns` 또는 `@remarks`에 설명한다. error 계약을 지어내지 않는다. [5] 예시는 유용할 때만 추가하고 실제 export와 일치시킨다. 내부 주석은 바로 알기 어려운 결정이나 invariant를 설명하며 뻔한 statement마다 붙이지 않는다.

## T03 — Layout과 정확한 열 정렬

두 칸 공백과 초기 120열 목표를 사용한다. 짧은 signature, expression, chain, literal은 가로로 유지한다. 비어 있지 않은 여러 줄 function, arrow, control-flow, type, object body의 양 끝 안쪽에 빈 줄 하나를 둔다. 여러 줄 constant array와 `return (...)`에도 경계 padding을 둔다. argument/specifier 목록이나 모든 field/element에 padding을 넣지 않는다.

**작고 연속된 논리 그룹 안에서 대입 `=`와 import `from`을 정렬한다.** operator/keyword 앞에 정렬용 공백을 넣고 뒤에는 한 칸을 둔다. 이름이 바뀌면 그룹을 다시 맞춘다. comparison, `=>`, text 내용, 빈 줄을 넘어가는 무관한 declaration을 정렬하지 않는다. object의 colon 정렬은 필수가 아니다.

```ts
import { create_client }        from "./transport/client";
import type { request_options } from "./transport/types";
```

```ts
const __page_size          = 20;
const __request_timeout_ms = 5_000;
const __retry_limit        = 3;
```

이 예시는 layout fragment이지 반드시 만들 module이나 설정이 아니다. 긴 왼쪽 표현 때문에 너비가 넘으면 의미 있는 경계에서 그룹을 나누거나 긴 expression을 줄바꿈한다. 유용한 이름을 줄이거나 declaration마다 빈 줄을 넣거나 무관한 코드를 옮겨 억지로 정렬하지 않는다.

import와 side effect 평가 순서를 보존한다. 정렬 과정에서 import/spread 순서를 바꾸지 않는다. 필요한 attribute/directive/comment 연결과 `return` 다음 expression 같은 줄바꿈 민감 문법을 유지한다.

## T04 — 완전한 selector 예시

이 예시는 받은 데이터만 읽는다. fetch, scan, subscribe 또는 새 application state 생성을 하지 않는다.

```ts
/**
 * 레코드의 식별자, 표시 이름, 사용 가능 상태를 담아
 */
export type record_state = {

  id: string;
  label: string;
  available: boolean;

};

/**
 * 전달받은 목록에서 ID가 같은 첫 번째 레코드를 찾아줘
 *
 * @remarks
 * 원본 객체를 그대로 반환하고 목록이나 레코드를 수정하지 않아
 * 같은 ID가 여러 개면 목록에서 먼저 나온 항목을 골라
 *
 * @param records - 검색할 레코드 목록
 * @param record_id - 찾을 레코드 ID
 * @returns 일치하는 첫 번째 레코드, 없으면 undefined
 */
export function find_record(records: readonly record_state[], record_id: string): record_state | undefined {

  return records.find((record) => record.id === record_id);

}

/**
 * 사용 가능한 레코드의 이름을 원래 순서대로 모아줘
 *
 * @remarks
 * 결과 배열은 새로 만들고 원본 목록은 수정하지 않아
 * 이름이 같아도 중복을 제거하지 않아
 *
 * @param records - 이름을 가져올 레코드 목록
 * @returns 사용 가능한 레코드의 이름 목록, 없으면 빈 배열
 */
export function select_labels(records: readonly record_state[]): string[] {

  return records.filter((record) => record.available).map((record) => record.label);

}
```

## T05 — 데이터 경계와 type

작고 명시적인 input과 유용한 return type을 우선한다. 신뢰하지 않는 데이터는 검증 전까지 `unknown`을 사용한다. 미확인 계약을 숨기려고 `any`, non-null assertion, 위험한 double cast, `@ts-ignore`를 사용하지 않는다. type assertion은 network/storage input 검증이 아니다.

애플리케이션이 필요로 하면 없음, 오래된 데이터, loading, failure를 구분한다. schema, default, nullable state, persistence, localization, 공개 payload를 유지한다. 같은 기준 type을 모든 feature에 복사하지 않는다.

selector는 I/O 없이 데이터를 계산한다. API adapter는 공유 transport를 사용한다. state owner는 수명을 관리한다. 실제 책임만 분리하며 작은 operation마다 `api/types/service/helper` 파일을 만들지 않는다.

## T06 — Async, state, lifecycle

모든 request, listener, connection, worker, timer, object URL, cache에 owner를 지정한다. 적절한 teardown에서 해제한다. 해제 이후 끝나는 async 등록도 처리하고, 늦게 등록된 subscription을 그대로 누수시키지 말고 해제한다.

오래된 request, session, generation의 response를 거부한다. pending/optimistic edit와 confirmed data를 구분한다. cancellation과 stale result 거부는 관련 있지만 다른 문제를 해결하므로 실제 transport가 제공하는 기능을 사용한다.

rejected promise를 처리하지 않은 채 두거나 동기 API처럼 보이는 곳에 fire-and-forget 작업을 숨기지 않는다. concurrency, retry, queue에 한도를 둔다. 필수 command 순서를 유지하며 latest-only replacement를 save나 shutdown command에 적용하지 않는다.

## T07 — Runtime 경계와 효율

server secret과 Node 전용 filesystem/process API를 browser bundle에 넣지 않는다. 실제 SSR, client, worker, build-time 경계를 존중한다. server에서도 실행되는 module에서 import 시 browser에 접근하지 않는다.

기존 store와 공유 결과를 재사용하고 consumer마다 독립 polling을 만들지 않는다. 변하지 않은 데이터의 sorting/copy, 비싼 resource의 불필요한 재생성을 피한다. cache 증가를 제한하고 비활성 consumer만 사용하는 작업을 중단한다. memoization, virtualization, dependency 추가 전에 측정한다.

반응성 있는 feedback과 프로젝트의 실제 latency/freshness 요구사항을 유지한다. generic TypeScript 프로젝트에 고정 frame rate, browser-only 가정, animation library를 가져오지 않는다.

## T08 — 파일과 테스트

test body, setup, fake, mock, fixture는 production `src/` 밖에 둔다. `tests/unit/`은 source owner를 mirror하고 feature 간 flow는 설정된 runner에 따라 `tests/integration/`에 둔다. shared fixture/mock은 `tests/`에, feature 전용 fixture는 해당 suite 옆에 둔다.

이동 시 static/dynamic import, mock target, snapshot, fixture, worker URL, asset, export, runner의 include/setup/coverage 경로, TypeScript test-checking 범위를 수정한다. test discovery를 유지한다. source만 build한 것은 테스트도 검사했다는 증거가 아니다.

명시적 import를 사용한다. 모든 곳에 barrel이나 alias를 도입하지 않는다. alias는 runtime, bundler, test runner, typechecker 모두에서 동작해야 한다. TypeScript `paths`만으로 출력 import 경로가 바뀌지는 않는다. [1]

동작 보존 이동에서는 중복 suite 대신 기존 테스트를 조정한다. 실제 bug, lifecycle risk, 동작 변경에 집중 coverage를 추가한다. 테스트만을 위해 production type이나 visibility를 약화하지 않는다.

## T09 — 도구와 싸우지 않는 formatting

실제 formatter 설정을 확인한다. Prettier는 block 경계의 빈 줄을 제거하므로 기본 formatting이 이 custom padding/alignment를 보존한다고 가정하면 안 된다. editor와 CI에서 하나의 승인된 최종 pipeline을 사용한다. [2]

대표 파일로 먼저 시험한다. 필요하면 작은 syntax-aware 보정에 합의한다. global regex, 일괄 ignore comment, formatting 비활성화, 새 formatter framework로 해결하지 않는다. string, template, comment, directive, regex literal, JSX text, 평가 순서를 유지한다.

두 번 실행한 결과가 같아야 하며 check는 변경을 쓰지 않아야 한다. 확인 전 `format` script가 존재한다고 주장하지 않는다. 지원하지 않는 문법과 건너뛴 owner는 검증 완료에 포함하지 말고 보고한다.

## T10 — 완료

owner별 변경을 검토하고 관련 기존 test와 typecheck를 실행한 뒤 통합 시점에 package build를 한다. 실제 manifest와 설치된 package manager에서 command를 확인한다. npm, Bun, Vite, Vitest를 가정하지 않는다.

실제 결과, 계약 때문에 필요한 style 예외, 실행하지 못한 runtime 검사를 보고한다. formatting, 구조, 의미 수정, 최적화는 각각 검토 가능한 변경으로 유지한다. 비교 가능한 근거 없이 성능 향상을 주장하지 않는다.

## 참고 자료

[1]: https://www.typescriptlang.org/tsconfig/paths.html
[2]: https://prettier.io/docs/rationale
[3]: https://tsdoc.org/pages/tags/param/
[4]: https://tsdoc.org/pages/tags/remarks/
[5]: https://tsdoc.org/pages/tags/throws/
