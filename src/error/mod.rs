/*! 작업 경계별 typed error를 모아. */

mod app;
mod capture;
pub(crate) use capture::CaptureError;
mod execution;
pub(crate) mod filesystem;
pub use app::{ConfigError, ConnectError, InstallError, PolicyError};
pub(crate) use execution::ExecutionError;
pub use filesystem::{FileError, SyncError};
