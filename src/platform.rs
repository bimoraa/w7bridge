/*! 프로세스 트리의 OS별 생성 정책을 선택한다. */

#[cfg(unix)]
use process_wrap::tokio::ProcessGroup;
use process_wrap::tokio::{CommandWrap, KillOnDrop};

#[cfg(windows)]
mod windows;

pub(crate) fn configure(command: &mut CommandWrap) {

    command.wrap(KillOnDrop);
    #[cfg(windows)]
    windows::configure(command);
    #[cfg(unix)]
    command.wrap(ProcessGroup::leader());

}
