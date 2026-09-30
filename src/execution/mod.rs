/*! 등록된 명령의 실행과 process 수명을 소유해. */

use crate::error::ExecutionError;
use std::io;
mod command;
pub(crate) mod process;
mod runner;
mod sandbox;
pub(crate) use process::Processes;
pub(crate) use runner::Executor;
fn failure(operation: &'static str, source: io::Error) -> ExecutionError {

    ExecutionError::Process { operation, source }

}
