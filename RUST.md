# 범용 Rust 규칙

> 개인 엔지니어링 정책 · 한국어 · 2026-09-28

## R01 — 범위

먼저 [AGENTS.md](AGENTS.md)를 읽는다. 이 지침은 승인된 범위의 프로젝트 소유 Rust 코드에 적용하며 library, CLI, service, desktop application 등 대상 종류를 가정하지 않는다. 특정 프레임워크나 운영체제를 전제하지 않는다.

본문 padding과 소문자 상수는 의도적인 custom style이다. 기본적인 관용 naming이나 rustfmt의 보장 사항이 아니다. 별도 마이그레이션 승인 전에는 upstream/generated 코드, 외부 계약, macro에 민감한 문법, 공개 호환성을 유지한다.

## R02 — Naming

| 항목 | 필요한 형식 |
| --- | --- |
| 소스 파일, 폴더, 모듈, 함수, local, parameter, field | `snake_case` |
| 프로젝트 소유 `const`, `static`, associated constant, thread-local static | 소문자 `snake_case` |
| struct, enum, variant, trait, type alias | `UpperCamelCase` |
| generic type parameter와 필수 external/trait/generated 이름 | 해당 언어·계약의 관례 유지 |

프로젝트 소유 이름으로 새 `MAX_ITEMS`, `DEFAULT_TIMEOUT`, `__ACTIVE_STATE`를 만들면 스타일 위반이다. `max_items`, `default_timeout`, `active_state`를 쓴다. Rust 소스에 `__` 접두사는 필수가 아니다. import한 플랫폼 상수나 공개 wire key를 소문자로 바꾸지 않는다.

적절한 crate/module 범위에 필요한 `non_upper_case_globals` 예외만 허용한다. `allow(warnings)`나 전체 Clippy 억제는 금지한다. 이 예외는 소문자를 허용할 뿐 강제하지 않으므로 선언과 사용을 검토한다. rename은 import, re-export, test, macro, consumer까지 포함한다. rename을 끝내지 않으려고 대문자 alias를 남기지 않는다. [1]

## R03 — 가로로 간결하게, block 안쪽에는 padding

- 네 칸 공백을 사용하고 tab은 쓰지 않는다. 초기 목표는 120열이다. 프로젝트에서 승인한 140열은 예외이며 무제한 너비가 아니다.
- 읽기 쉬운 signature, 단순 call, 작은 literal, 짧은 chain은 한 줄에 둔다. parameter 개수만으로 세로 확장을 정당화하지 않는다.
- 비어 있지 않은 모든 여러 줄 중괄호 본문 시작·끝 안쪽에 빈 줄을 정확히 하나 둔다. function, method, `impl`, trait/module, control flow, block closure, `unsafe`, 여러 줄 struct/enum 정의와 literal에 적용한다.
- 간결한 한 줄 단위, 빈 `{}`, expression closure, 짧은 match arm은 유지한다. padding을 넣으려고 여러 줄로 늘리지 않는다.
- 관련 statement와 state 변경은 붙여 둔다. 논리 단계 사이에 빈 줄 하나를 두며 모든 statement, field, argument, arm 사이에 넣지 않는다. padding이 겹치면 한 줄을 공유한다.
- import 목록, generic parameter, call argument 목록을 실행 block처럼 padding하지 않는다. `} else {`, `});`, attribute/doc comment의 연결을 유지한다.
- 긴 flags 표현식에 이름을 붙이는 것은 call을 명확히 할 때만 한다. 줄에 맞추려고 무의미한 wrapper를 만들거나 이름을 줄이지 않는다. Rust의 수동 `=` 열 정렬은 필수가 아니다.
- string, raw string, shader text, generated 내용, 지원하지 않는 macro token tree는 건드리지 않는다. 문법을 이해하는 구조화된 macro에서만 의미별 field 그룹을 허용한다.

## R04 — 스타일 예시

다음 독립 예시는 layout과 naming을 설명한다. record model을 도입하거나 기존 limit을 바꾸라는 지시가 아니다. 예시 코드는 대상 저장소의 빌드가 검증되었다는 뜻이 아니다.

G03의 한국어 주석 계약을 따른다. item 문서는 block rustdoc `/** ... */`, crate/module 문서는 `/*! ... */`를 우선한다. 매 줄의 `///`, `//!`, 장식용 `*` prefix는 필수가 아니다. item 문서는 선언과 그 attribute 바로 앞에 두고 inner docs는 crate/module의 시작에 둔다. 기존 line docs도 유효하며 승인된 범위 밖에서 변환할 필요는 없다. 구현 주석은 일반 `//` 또는 `/* ... */`를 사용한다. [4]

summary는 짧게 쓰고 실제 계약을 이어서 설명한다. 선택적인 `# Behavior`, `# Ownership` heading은 의미 있는 상세 내용을 정리하는 custom heading이며 rustdoc 필수 section이 아니다. 관련 error 조건과 부분 실패의 영향은 `# Errors`, caller에게 관련 있는 알려진 panic 조건은 `# Panics`, caller가 지켜야 하는 unsafe API invariant는 `# Safety`에 적는다. 사용법을 명확히 할 때 유용한 `# Examples`를 추가하되 실행 가능한 doctest와 실제 crate path를 유지한다. 각 unsafe block 가까이에서 안전한 이유를 설명하고 `SAFETY:` 주석에는 실제로 충족된 invariant를 적는다. 빈 section이나 근거 없는 보장은 만들지 않는다. [5]

```rust
#![allow(non_upper_case_globals)]

/**
표시할 레코드 수의 상한이야
*/
pub const default_limit: usize = 20;

/**
레코드의 식별자, 표시 이름, 사용 가능 상태를 담아
*/
#[derive(Clone, Debug)]
pub struct Record {

    pub id: u64,
    pub label: String,
    pub available: bool,

}

/**
ID가 같은 첫 번째 레코드가 사용 가능하면 참조를 돌려줘

# Behavior

- ID가 일치하는 레코드가 없으면 `None`이야
- 첫 번째 일치 항목이 사용 불가 상태여도 `None`을 반환해
- 같은 ID가 뒤에 더 있어도 추가로 찾지는 않아

# Ownership

반환값은 `records`에서 빌린 참조야
원본 목록을 수정하거나 레코드를 복사하지 않아
*/
pub fn find_available_record(records: &[Record], record_id: u64) -> Option<&Record> {

    let record = records.iter().find(|record| record.id == record_id)?;

    if !record.available {

        return None;

    }

    Some(record)

}

/**
사용 가능한 레코드를 원래 순서대로 골라 참조 목록을 만들어줘

요청한 `limit`은 `default_limit`까지만 허용해
`limit`이 0이면 빈 목록을 돌려줘

결과를 담을 벡터는 새로 만들지만 레코드 자체는 복사하거나 수정하지 않아
*/
pub fn visible_records(records: &[Record], limit: usize) -> Vec<&Record> {

    let limit = limit.min(default_limit);

    records.iter().filter(|record| record.available).take(limit).collect()

}
```

## R05 — Error와 재사용 경계

집중된 operation, 명시적 input, borrowing, 좁은 visibility를 우선한다. 재사용 함수는 실제 중복을 제거하거나 계약을 명확히 해야 한다. 함수별 파일, forwarding chain, generic manager, 목적이 입증되지 않은 trait/newtype은 피한다.

실패를 먼저 분류한다. 필수 작업은 의미 있는 error를 반환하고 선택 작업은 관찰 가능한 fallback을 사용한다. cleanup은 panic하거나 원래 error를 없애면 안 된다. formatting 중 모든 무시된 결과를 `?`로 바꾸거나 모든 실패를 default로 덮지 않는다. 이는 동작 변경이다.

특정 error library를 강제하지 않고 유용한 경계에서 `Result`와 typed error를 사용한다. 실패 가능한 production input에 무검증 `unwrap()`/`expect()`를 쓰지 않는다. invariant를 근거로 삼으려면 증거가 필요하다. test assertion과 의도적으로 문서화된 fatal invariant는 다른 요구사항을 가진다.

## R06 — Ownership, unsafe, concurrency

resource의 소유권과 수명을 명시한다. 반복 cleanup을 신뢰성 있게 대체할 때 작은 typed guard를 사용한다. acquisition과 올바른 release operation을 짝지으며 handoff가 성공한 뒤에만 소유권을 넘긴다. partial initialization과 early return 경로도 처리한다.

필요한 경우에만 unsafe를 사용하고 실제 pointer, lifetime, aliasing, thread, ABI invariant를 문서화한다. 외부 API의 return 규칙, 초기화, payload 크기, encoding, platform 요구사항을 확인한다. 감사된 FFI가 필요한 프로젝트에 일괄 unsafe 금지를 적용하거나 서로 다른 handle에 하나의 generic destructor를 만들지 않는다.

검증된 설계 없이 blocking 작업, `.await`, reentrant 가능 callback을 넘어서 shared lock을 유지하지 않는다. cancellation과 completion/join 소유자를 명확히 한다. self-join, UI thread 정지, 무한 queue, entity별 thread 생성, 근거 없는 `unsafe impl Send/Sync`를 피한다. 기존 synchronization 자체가 결함인 것은 아니다.

## R07 — 모듈과 테스트

현재 crate/workspace 구조를 따른다. 여러 파일 owner는 `owner.rs`와 `owner/` 또는 `owner/mod.rs`를 쓸 수 있으며 둘 다 유효하다. 기본 entry가 충돌하게 두거나 무관한 모듈에 한 형태를 강요하지 않는다. 모든 feature를 crate로 만들지 않는다. 일반 module 선언을 우선하되 기존 textual include scope를 보존한다.

unit test의 body/fake/fixture는 production `src/` 밖에 둔다. private test는 작은 test-gated path 선언으로 원래 owner의 논리적 child를 유지할 수 있다. 공개 integration test는 실제 Cargo test target에 둔다. private 내부에 접근하려고 API를 넓히지 않는다. [2]

```text
package/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── cache.rs
│   └── cache/
│       ├── entry.rs
│       └── eviction.rs
└── tests/
    ├── public_api.rs
    ├── unit/
    │   └── cache.rs
    └── fixtures/
```

모듈 파일이 실제로 `src/cache.rs`에 있을 때 다음 연결로 예시의 private suite를 읽을 수 있다. owner가 이동하면 경로를 다시 계산하고 다른 디렉터리에 이 상대 경로를 그대로 복사하지 않는다.

```rust
#[cfg(test)]
#[path = "../tests/unit/cache.rs"]
mod tests;
```

tree는 소유권만 보여 준다. 필요 없이 모든 파일이나 새 테스트를 만들지 않는다. Cargo는 임의의 integration 파일을 재귀적으로 발견하지 않으므로 필요한 root target이나 명시적 등록을 유지한다. doctest는 의미 있는 실행형 API 문서다. `src/`에서 테스트를 없애 보이게 하려고 제거하지 않는다. [2]

## R08 — 데이터 재사용과 성능

discovery/loading, identity 검증, 최신 값 sampling, 순수 snapshot 접근을 분리한다. 실제 수명 범위에서 한 소유자가 공유 데이터를 제공해야 한다. parameter를 없애려고 global singleton을 만들지 않는다.

identity, generation, input, ownership이 유효한 동안만 cache한다. 교체·제거 이후의 오래된 결과를 거부한다. reader가 유지하는 snapshot을 뒤에서 변경하지 않는다. 값이 같아도 freshness를 보존한다. 새 generation을 표시하기 위해 항상 새 allocation이 필요한 것은 아니다.

변경 전에 비싼 copy, string/map clone, sorting, 반복 I/O, allocation, cache miss, lock contention을 측정한다. 소유 데이터 복사와 shared handle 복제를 구분한다. 한도가 있는 buffer를 재사용하고 eviction/disposal을 정의한다. 모든 getter를 cache하거나 곳곳에 `inline(always)`를 붙이지 않는다.

latest-only replacement는 교체 가능한 state에만 사용하며 필수 command, 순서가 있는 write, release, shutdown에는 적용하지 않는다. 실제 throughput, latency, freshness 요구사항을 유지한다. 임의로 sleep을 제거하거나 busy loop로 대체하지 않고 다른 프로젝트의 고정 rendering 목표를 가져오지 않는다.

## R09 — 포매터 계약

설치된 rustfmt의 버전과 설정을 확인한다. 다음은 시작용 너비 설정이며 줄바꿈을 제어할 뿐 정확한 custom padding을 보장하지 않는다. whitespace를 위해 edition이나 compiler channel을 바꾸지 않는다. [3]

```toml
max_width = 120
fn_call_width = 120
tab_spaces = 4
hard_tabs = false
```

대표 owner 하나에서 목표 출력을 시험한다. 기본 포매터가 유지하지 못하면 이미 승인된 syntax-aware pipeline을 사용한다. 없다면 전체 적용 전에 정확한 충돌을 보고한다. formatting을 끄거나 `rustfmt::skip`을 퍼뜨리거나 전역 중괄호·줄바꿈 regex를 쓰지 않는다.

편집기와 CI는 같은 최종 pipeline을 사용해야 한다. 두 번 포매팅한 결과가 같아야 하며 check는 worktree를 변경하지 않는다. comment, macro 의미, string, include 해석을 유지한다. 소스 위치에 민감한 macro와 source 검사 테스트도 고려한다. 실제로 그렇지 않다면 일반 `cargo fmt --check`가 custom post-pass를 검증한다고 주장하지 않는다.

## R10 — 검증

diff, semantic reference, visibility, feature/cfg 경로, include, error handling, cleanup, test discovery를 검토한다. 일관된 변경 묶음 뒤 가장 작은 유효 검사를 실행하고 이후 넓은 통합 검사를 한다. 입증된 빈틈이나 위험에만 집중 테스트를 추가한다.

다음은 crate root에서의 예시이며 모든 workspace에 강제되는 명령은 아니다:

```sh
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test
```

실제 package/feature/target과 기존 lockfile 정책을 사용하고 필요하면 `--locked`를 붙인다. 모든 feature를 무작정 활성화하거나 실패를 몰래 건너뛰지 않는다. ignored/live/destructive test는 전제 조건 없이 실행하지 않는다. `cargo test --lib`만으로 integration target을 검증할 수 없다. 불가능한 platform/runtime 검증을 보고한다. [2]

## 참고 자료

[1]: https://doc.rust-lang.org/rustc/lints/listing/warn-by-default.html#non-upper-case-globals
[2]: https://doc.rust-lang.org/cargo/reference/cargo-targets.html
[3]: https://github.com/rust-lang/rustfmt/blob/main/Configurations.md
[4]: https://doc.rust-lang.org/reference/comments.html#doc-comments
[5]: https://rust-lang.github.io/api-guidelines/documentation.html
