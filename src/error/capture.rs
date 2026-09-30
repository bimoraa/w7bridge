use thiserror::Error;

/** screenshot 권한, 한도와 OS 실행 실패를 구분한다. native stderr와 개인 경로는 응답에 노출하지 않는다. */
#[derive(Debug, Error)]
pub(crate) enum CaptureError {

    #[error("screenshot 도구가 비활성화되어 있습니다. 서버 소유자가 screenshots.enabled를 설정하세요")]
    Disabled,
    #[error("screenshot 작업이 이미 실행 중입니다")]
    Busy,
    #[error("screenshot 요청이 취소되었거나 서버가 종료 중입니다")]
    Cancelled,
    #[error("screenshot 실행 한도 25초를 초과했습니다")]
    Timeout,
    #[error("screenshot을 생성할 수 없습니다. 로그인한 desktop과 OS screen recording 권한을 확인하세요")]
    Unavailable,
    #[cfg(not(any(target_os = "macos", windows)))]
    #[error("macOS와 Windows에서만 screenshot을 지원합니다")]
    Unsupported,
    #[error("screenshot 한도를 초과했습니다 (PNG 8 MiB, 2000만 pixel)")]
    Limit,
    #[error("screenshot PNG 형식이 올바르지 않습니다")]
    Data,
    #[error("screenshot process 또는 임시 task 종료를 확인할 수 없습니다")]
    Cleanup {

        #[source]
        cause: Option<Box<CaptureError>>,

    },

}
