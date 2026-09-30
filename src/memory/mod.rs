/*! 공유 context 문서를 project별로 읽고 조건부 갱신해. 파일이 유일한 원본이야. */

mod context;
mod project;
mod state;
mod storage;

pub(crate) use project::ProjectMemory;

#[cfg(test)]
#[path = "../../tests/unit/memory.rs"]
mod tests;
