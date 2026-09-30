/*! CLI host 경계를 넘는 오류의 공통 타입이야. */

pub(crate) type Failure = Box<dyn std::error::Error + Send + Sync>;
