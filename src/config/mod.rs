/*! 서버 registry와 로컬 pairing 설정을 소유해. */

mod loader;
mod model;
pub(crate) mod paths;
pub(crate) use model::{CodexSettings, CommandDefinition, ExecutionLimits, Pair, ProjectDefinition, SyncSettings};
pub use model::{Config, FileSettings};

#[cfg(test)]
#[path = "../../tests/unit/config.rs"]
mod tests;
