/*! 등록된 명령의 실행, 출력, 프로세스 수명을 묶는 모듈이다. */

use crate::error::ExecutionError;
use std::io;

mod command;
mod output;
mod process;

pub(crate) use command::Executor;

fn failure(operation: &'static str, source: io::Error) -> ExecutionError {

    ExecutionError::Process { operation, source }

}
