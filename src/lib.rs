/*! 등록된 Windows 프로젝트 명령을 MCP 도구로 제공하는 초기 기반이다. */

mod config;
mod error;
mod execution;
mod platform;
mod security;
mod server;
mod tools;

pub use config::Config;
pub use error::{ConfigError, PolicyError};
pub use server::{Bridge, serve_stdio};
