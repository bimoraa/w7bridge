/*! Windows Job Object를 통해 자식 프로세스 수명을 묶는다. */

use process_wrap::tokio::{CommandWrap, JobObject};

pub(crate) fn configure(command: &mut CommandWrap) {

    // SDK wrapper가 정지 상태로 생성하고 Job에 연결한 뒤 재개해. 실패를 우회하지 않아.
    command.wrap(JobObject);

}
