/*! 콘솔 창 없이 자식을 실행하고 Windows Job Object로 수명을 묶어. */

use process_wrap::tokio::{CommandWrap, CreationFlags, JobObject};
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

pub(crate) fn configure(command: &mut CommandWrap) {

    // JobObject가 생성 flags를 다시 설정하므로 wrapper를 통해 콘솔 억제를 유지해.
    let mut flags = command.get_wrap::<CreationFlags>().map_or(Default::default(), |flags| flags.0);
    flags.0 |= CREATE_NO_WINDOW;
    command.wrap(CreationFlags(flags));
    // SDK wrapper가 정지 상태로 생성하고 Job에 연결한 뒤 재개해. 실패를 우회하지 않아.
    command.wrap(JobObject);

}

#[cfg(test)]
#[path = "../../../tests/unit/windows_process.rs"]
mod tests;
