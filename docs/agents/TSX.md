# 범용 TSX 및 프런트엔드 규칙

> 개인 엔지니어링 정책 · 한국어 · 2026-09-28

## X01 — 적용 조건

먼저 [AGENTS.md](AGENTS.md)와 [TYPESCRIPT.md](TYPESCRIPT.md)를 읽는다. JSX layout 규칙은 실제 TSX toolchain에 적용한다. 아래 React 전용 규칙은 React 프로젝트에서만 사용한다. 다른 JSX framework의 lifecycle과 compiler 요구사항을 유지하며 React를 새로 설치하지 않는다.

React frontend에서는 **Tailwind CSS를 프로젝트 소유 UI의 기본이자 전면적인 styling system으로 사용하고, Motion의 공식 React package를 animation system으로 사용한다.** 새 project-owned UI에 별도 component CSS, CSS module, CSS-in-JS 또는 경쟁 animation library를 임의로 추가하지 않는다. Motion은 `motion` package와 `motion/react` entry를 기준으로 하며, 실제 framework가 server/client 경계를 요구하면 공식 지원 entry를 따른다. [4][5]

구조 정리 작업에서는 기존 route, entry point, UI text, accessibility, data contract, 동작을 유지한다. tree를 깔끔하게 만들려고 framework migration, 새 state dependency, package root rename을 하지 않는다. 기존 legacy styling/animation stack 전체를 관련 없는 정리 작업에서 강제 migration하지는 않지만, 새로 작성하거나 명시적으로 재작업하는 project-owned React UI는 이 문서의 Tailwind + Motion 정책을 따른다.

## X02 — 기능 소유권

application/route composition, feature-local code, 재사용 presentation, shared infrastructure를 구분한다. 이 책임을 기존 디렉터리에 대응시킨다. `app`, `pages`, `routes`, `landing`, `features`가 항상 대체 가능한 것은 아니다. framework routing 규칙은 보기 좋은 이름보다 우선한다.

page는 실제 section을 조합한다. Hook은 실제 state/lifecycle을 소유한다. selector는 순수 함수다. feature API는 shared transport를 호출한다. reusable control은 feature별 command나 payload 대신 value와 callback을 받는다.

private card, hook, selector, type, API function은 해당 feature에 둔다. 실제 consumer 또는 분명한 경계가 있을 때만 shared code를 추출한다. 작은 page에 일곱 파일, component마다 한 폴더, 중복 store, generic `utils`를 만들지 않는다. 큰 파일은 검토 대상이지 임의 분리의 근거가 아니다.

## X03 — 프런트엔드와 테스트 tree 예시

아래는 일반적인 client package의 예시이며 필수 framework layout이나 생성할 scaffold가 아니다. 실제 lockfile, build/config file, framework entry 이름, 예시에 없는 활성 owner를 모두 유지한다. framework가 관리하는 application은 다른 root를 사용할 수 있다.

```text
frontend/
├── package.json
├── tsconfig.json
├── public/
├── src/
│   ├── main.tsx
│   ├── styles.css
│   ├── app/
│   │   ├── app.tsx
│   │   └── routes.tsx
│   ├── features/
│   │   ├── records/
│   │   │   ├── page.tsx
│   │   │   ├── record_list.tsx
│   │   │   ├── record_card.tsx
│   │   │   ├── use_records.ts
│   │   │   ├── api.ts
│   │   │   ├── selectors.ts
│   │   │   └── types.ts
│   │   └── settings/
│   │       ├── page.tsx
│   │       └── form.tsx
│   ├── components/
│   │   ├── controls/
│   │   │   ├── button.tsx
│   │   │   └── input.tsx
│   │   └── layout/
│   │       └── panel.tsx
│   ├── lib/
│   │   ├── transport/
│   │   │   └── client.ts
│   │   └── session/
│   │       └── store.ts
│   └── assets/
│       ├── icons/
│       └── images/
└── tests/
    ├── setup.ts
    ├── unit/
    │   ├── app/
    │   │   └── app.test.tsx
    │   ├── features/
    │   │   ├── records/
    │   │   │   ├── record_card.test.tsx
    │   │   │   └── selectors.test.ts
    │   │   └── settings/
    │   │       └── form.test.tsx
    │   ├── components/
    │   │   └── controls/
    │   │       └── button.test.tsx
    │   └── lib/
    │       └── transport/
    │           └── client.test.ts
    ├── integration/
    │   └── settings_flow.test.tsx
    ├── fixtures/
    │   └── records.ts
    └── mocks/
        └── transport.ts
```

실제 owner가 있을 때만 디렉터리를 만든다. mirror는 source-relative다. `src/features/records/record_card.tsx`의 test는 `tests/unit/features/records/record_card.test.tsx`에 대응한다. 기존 manual QA는 `qa/`에 남길 수 있고 자동 visual/e2e suite는 runner 설정을 따른다. 필수 baseline은 폐기용 출력이 아니다.

위 tree의 `styles.css`는 Tailwind entry, theme/token, document-level base, third-party compatibility처럼 실제 global owner가 필요한 경우를 위한 것이다. component별 visual styling을 그 파일로 다시 모으지 않는다. project-owned component의 layout, spacing, typography, color, border, responsive state는 기본적으로 JSX의 Tailwind utility로 표현한다.

## X04 — Component와 Hook naming

새 프로젝트 소유 reusable component는 `__UpperCamelCase`, frontend feature/component file은 `snake_case.tsx`를 쓴다. 필수 route filename, default export, third-party component 이름, `onClick` 같은 browser prop, 공개 component API는 유지한다. 일반 handler와 local prop/type 이름은 소문자 `snake_case`를 따른다.

React custom Hook은 React가 인식하는 `use` + 대문자로 시작하는 이름 규칙을 따라야 한다. 예를 들어 `useRecords`를 쓴다. 일반 함수 naming에 대한 명시적 호환성 예외이며 filename은 `use_records.ts`일 수 있다. import한 Hook을 기계적으로 rename하거나 검증된 tooling 지원 없이 새 Hook을 `__use_records`/`use_records`로 숨기지 않는다. 기존 custom naming은 의도적인 호환 migration으로 다루고 몰래 rename하지 않는다. [1]

exported component와 Hook은 G03의 한국어 문체 계약과 T02의 TSDoc 구조로 문서화한다. 관련 있는 controlled state, callback payload, unavailable/loading/error 동작, subscription 또는 cleanup ownership을 설명한다. prop 계약은 소유 type 가까이에 적고 parameter tag는 실제 parameter와 설정된 tooling의 destructuring 지원에 맞춘다. 이름만 반복하는 component marker나 가짜 parameter 이름은 쓰지 않는다.

## X05 — JSX, padding, alignment

읽기 쉬운 짧은 tag, component signature, callback, expression은 inline으로 둔다. 비어 있지 않은 multiline function/control-flow/object body와 multiline `return (...)` 경계에는 padding을 둔다. 모든 child나 state declaration 사이에 빈 줄을 넣지 않는다.

여러 줄 opening tag에서는 연속된 값 있는 prop의 `=`를 공백으로 정렬하고 뒤에 한 칸을 둔다. prop/spread 순서를 바꾸거나 boolean/spread prop에 가짜 대입을 추가하지 않는다. 짧은 tag를 억지로 여러 줄로 늘리지 않는다. `{label}` 같은 JSX expression brace는 간결하게 유지한다.

text node whitespace, 명시적 공백, `<pre>` 내용, template, string, key, child 순서를 보존한다. formatting으로 화면 내용이 바뀌면 안 된다. 공백에 민감한 markup에 장식용 padding을 넣지 않는다.

다음은 Tailwind + Motion style 예시일 뿐 기존 design system이나 product contract를 대체하지 않는다. static presentation은 Tailwind class로, runtime animation은 Motion prop으로 분리한다.

```tsx
import { motion } from "motion/react";

type record_card_props = {

  record: { id: string; label: string; available: boolean };
  selected: boolean;
  on_select: (record_id: string) => void;

};

/**
 * 레코드 정보랑 선택 상태를 카드로 보여줘
 *
 * @remarks
 * 사용할 수 없는 레코드도 목록에는 남겨 두고 선택만 막아
 * 클릭하면 on_select에 record.id를 넘겨
 * 선택 상태는 부모에서 관리하고, 여기서는 selected 값만 따라가
 */
export function __RecordCard({ record, selected, on_select }: record_card_props) {

  const label        = record.label || "Untitled";
  const status_label = record.available ? "Ready" : "Unavailable";

  return (

    <motion.button
      type         = "button"
      className    = "flex w-full items-center justify-between rounded-xl bg-black px-4 py-3 text-sm text-white disabled:opacity-40"
      aria-pressed = {selected}
      disabled     = {!record.available}
      onClick      = {() => on_select(record.id)}
      whileHover   = {record.available ? { y: -2, scale: 1.01 } : undefined}
      whileTap     = {record.available ? { scale: 0.985 } : undefined}
      transition   = {{ type: "spring", stiffness: 420, damping: 30 }}
    >
      <span className="truncate font-medium">{label}</span>
      <span className="text-white/60">{status_label}</span>
    </motion.button>

  );

}
```

기존 React Hook/component 안에서 관련 declaration은 함께 정렬한다. 아래는 owner가 input/type을 제공하는 fragment이며 top-level Hook 실행 코드가 아니다:

```tsx
  const generation_ref         = useRef(generation);
  const [records, set_records] = useState<readonly record_state[]>([]);
  const [error, set_error]     = useState<string | null>(null);
  const [loading, set_loading] = useState(false);

  const ready = !loading && error === null;
```

## X06 — State와 effect

실제 lifetime 안에서 한 authoritative state owner를 유지한다. 단순 파생 값은 중복 state로 저장하지 않고 계산한다. 계약에 따라 confirmed data, pending edit, error, loading, identity를 구분한다. prop을 피하려고 view별 state를 global singleton으로 바꾸지 않는다.

React Hook은 Rules of Hooks를 지켜야 한다. effect setup과 cleanup을 대응시키고 실제 dependency를 포함한다. stale closure를 숨기려고 lint를 끄지 않는다. async subscription 등록이 끝나기 전 teardown되는 경우도 처리한다. StrictMode에서 안전한 setup/cleanup과 SSR/client 경계를 유지한다. [1][2]

기존 selector/store infrastructure를 사용한다. 설계가 지원하면 subscriber는 관련 데이터만 받게 하고, 값이 바뀌지 않았다면 snapshot identity를 유지한다. external store를 읽을 때마다 새 snapshot을 만들지 않는다. [3]

순서가 중요한 변경과 save acknowledgment를 정확히 유지한다. 만료된 request/session의 response는 무시하거나 취소한다. component 이동 때문에 render마다 추가 request, subscription, worker가 생겨서는 안 된다.

## X07 — Tailwind, Motion, 성능, accessibility

Tailwind CSS를 project-owned React UI의 기본 styling layer로 사용한다. 새 JSX에서는 layout, spacing, sizing, typography, color, border, shadow, backdrop, responsive breakpoint, dark mode, hover/focus/disabled state를 가능한 한 Tailwind utility로 표현한다. static visual rule을 위해 component-scoped `.css`, CSS module, Sass/Less, styled-components, Emotion 같은 별도 styling system을 새로 만들지 않는다. [6][7]

반복되는 design value는 현재 project의 Tailwind theme/token 체계에 올리고, 한 번만 필요한 값은 필요한 범위에서 arbitrary value를 사용할 수 있다. 같은 magic value를 여러 component에 복사하지 않는다. 이미 `cn`, `clsx`, `cva`, `tailwind-merge` 같은 class composition 도구가 있다면 기존 도구를 사용하고 같은 목적의 helper를 중복 설치하지 않는다. 단순한 고정 class를 helper 뒤에 숨기지 않는다.

project-owned component의 일반 visual styling에는 `style={{ ... }}`를 사용하지 않는다. inline style은 runtime에서만 알 수 있는 값, CSS custom property 전달, canvas/WebGL bridge, 또는 Motion이 직접 소유하는 animated value처럼 Tailwind utility로 정적으로 표현할 수 없는 경우에만 쓴다. global CSS는 Tailwind entry/theme, document-level base, font declaration, browser/platform fix, third-party compatibility처럼 실제 global responsibility가 있을 때만 유지한다.

**React animation은 Motion을 사용한다.** package 이름은 `motion`이고 client React code는 기본적으로 `motion/react`에서 import한다. Next.js 같은 React Server Component 환경에서는 client component에서 `motion/react`를 사용하고, 적합한 server component 경계에서는 공식 `motion/react-client` entry를 사용할 수 있다. framework의 실제 client/server boundary를 우회하지 않는다. 새 animation을 위해 legacy `framer-motion`, GSAP, Anime.js, React Spring, AOS 또는 다른 경쟁 animation system을 추가하지 않는다. [4][5]

enter/exit, gesture, hover/tap, drag, layout transition, shared layout, presence, spring, keyframe, viewport reveal, scroll-linked animation, parallax는 Motion API로 구현한다. 상황에 맞게 `motion.*`, `AnimatePresence`, `layout`, `whileHover`, `whileTap`, `whileInView`, `useScroll`, `useTransform`, `useSpring`, `useMotionValue`, `useReducedMotion`을 사용한다. 시간 기반 visual transition을 새 Tailwind `transition-*` utility나 직접 작성한 `@keyframes`로 분산시키지 않는다. Tailwind는 static/state styling을 소유하고 Motion은 animated state와 시간 축을 소유한다.

scroll/parallax에서 매 frame React state를 갱신하지 않는다. Motion value와 transform chain을 사용하고 필요한 경우에만 DOM measurement를 한다. animation hot path에서는 `transform`과 `opacity`를 우선하고 layout-triggering property, 큰 blur/filter, 과도한 box-shadow animation은 측정 없이 남발하지 않는다. `will-change`를 상시 모든 element에 붙이지 않는다.

`AnimatePresence` child에는 안정적인 identity key를 사용한다. layout animation 때문에 semantic DOM order, focus order, pointer behavior를 깨뜨리지 않는다. animation ownership은 component lifecycle과 함께 정리하며 custom listener, observer, timer, requestAnimationFrame을 추가했다면 teardown을 명확히 한다.

`prefers-reduced-motion`을 존중한다. 의미 없는 이동, scale, parallax, auto-motion은 `useReducedMotion` 또는 동등한 project policy를 통해 축소하거나 제거한다. reduced motion에서도 정보, focus indication, loading/error feedback, navigation state는 보존한다. animation을 제거하는 것이 accessibility feedback을 제거하는 이유가 되어서는 안 된다.

memoization, virtualization, dependency 추가 전에 rendering과 data-flow 비용을 측정한다. lazy-loading 경계를 유지한다. 새 barrel이 모든 page/editor/renderer를 eager import하게 만들지 않는다. 비싼 resource는 owner를 정해 재사용하고 명시적으로 해제한다. rendering budget은 프로젝트와 platform에서 정하며 고정된 범용 주사율 목표가 아니다.

## X08 — Production tree를 어지럽히지 않는 테스트

`.test.ts`/`.test.tsx`, setup, fixture, mock, fake backend는 production `src/` 밖에 둔다. 유용한 기존 coverage를 유지한다. 이동한 component마다 suite를 만들거나 tree에 맞추려고 snapshot을 늘리지 않는다.

테스트 이동 시 runner discovery, environment, setup, coverage 경로, mock target, snapshot 경로, test typecheck를 수정한다. 동작 변경, lifecycle, accessibility, 실제 bug에 집중 회귀 테스트를 추가한다. 테스트 편의를 위해 유용한 test를 없애거나 production API를 바꾸지 않는다.

변경에 맞춰 routing, lazy fallback, asset, state persistence, empty/error state, controlled input, resource cleanup을 검증한다. Motion 변경에는 presence key, reduced-motion path, gesture/scroll interaction, cleanup을 확인한다. Tailwind 변경에는 responsive breakpoint, focus/disabled state, theme/dark mode, class composition을 확인한다. 외관에 영향이 있을 수 있다면 같은 viewport/theme/platform에서 visual을 비교한다. typecheck만으로 visual equivalence를 입증할 수 없다.

## X09 — 안전한 이동과 완료

이전 경로와 새 owner를 매핑한다. 관련 그룹을 test/import/asset과 함께 먼저 이동하고 diff 검토와 검증을 한다. 승인된 경우 그다음 실제로 섞여 있는 책임을 분리한다. server/client directive, dynamic import, worker URL, CSS 순서, framework routing을 유지한다.

주 엔지니어가 검토된 변경을 통합한다. 보조 에이전트는 [AGENTS.md](AGENTS.md)의 권한을 따르며 특정 모델 이름이나 고정 인원수는 요구하지 않는다.

실제 source ownership이 분명하고, test가 `src/` 밖에서도 발견되며, project-owned React styling이 Tailwind policy를 따르고, 새 animation이 Motion policy를 따르며, custom alignment/padding이 승인된 formatter에서 유지되고, 관련 검사가 통과하거나 정확한 blocker가 보고되었을 때 완료한다. 이 예시 tree만으로 restructure를 구현했다고 주장하지 않는다.

## 참고 자료

[1]: https://react.dev/learn/reusing-logic-with-custom-hooks
[2]: https://react.dev/reference/react/useEffect
[3]: https://react.dev/reference/react/useSyncExternalStore
[4]: https://motion.dev/docs/react-installation
[5]: https://motion.dev/docs/react
[6]: https://tailwindcss.com/docs/styling-with-utility-classes
[7]: https://tailwindcss.com/docs/responsive-design
