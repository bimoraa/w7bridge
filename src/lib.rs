/*! 등록된 Windows project의 MCP, 파일 접근과 sync를 연결해. */

mod app;
mod capture;
mod config;
mod connection;
mod error;
mod execution;
pub mod filesystem;
mod git;
mod memory;
mod platform;
mod protocol;
mod security;
mod server;
pub mod sync;
mod tools;
mod update;
pub use app::run;
pub use config::Config;
pub use error::{ConfigError, ConnectError, FileError, InstallError, PolicyError, SyncError};
pub use filesystem as files;
pub use server::{Bridge, serve_stdio};
