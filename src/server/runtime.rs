use super::Bridge;
use rmcp::ServiceExt;
use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, ReadBuf, Stdin};
use tokio_util::sync::CancellationToken;
/**
stdio로 MCP 연결을 처리한다. stdout에는 protocol 메시지만 쓴다.

연결 종료 시 `shutdown`을 취소한다. host가 token을 취소하면 SDK 연결도 종료하고 종료 완료를 기다린다.
host는 이 future를 먼저 drop하지 말고 token 취소 후 완료를 기다려야 한다.

# Errors

초기화와 연결 처리 실패는 한국어 오류를 반환한다. 실제 자식 실행 실패는 MCP 도구 결과로 전달한다.
*/
pub async fn serve_stdio(bridge: Bridge, shutdown: CancellationToken) -> Result<(), &'static str> {

    let owner = bridge.clone();
    let input = ShutdownInput { stdin: tokio::io::stdin(), shutdown: shutdown.clone() };
    let service = tokio::select! {
        biased;
        _ = shutdown.cancelled() => return Ok(()),
        result = bridge.serve((input, tokio::io::stdout())) => result.map_err(|_| "MCP 초기화에 실패했습니다")?,
    };
    let cancellation = service.cancellation_token();
    let waiting = service.waiting();
    tokio::pin!(waiting);

    let result = tokio::select! {
        result = &mut waiting => result,
        _ = shutdown.cancelled() => {
            cancellation.cancel();
            waiting.await
        }
    };

    shutdown.cancel();
    owner.shutdown().await;
    result.map(|_| ()).map_err(|_| "MCP 연결 처리에 실패했습니다")

}

struct ShutdownInput {

    stdin: Stdin,
    shutdown: CancellationToken,

}

impl AsyncRead for ShutdownInput {

    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {

        let before = buffer.filled().len();
        let has_capacity = buffer.remaining() > 0;
        let result = Pin::new(&mut self.stdin).poll_read(context, buffer);

        // SDK의 요청 대기보다 먼저 종료를 알려서 연결이 끊긴 작업을 중단해.
        if matches!(&result, Poll::Ready(Err(_)))
            || matches!(&result, Poll::Ready(Ok(()))) && has_capacity && buffer.filled().len() == before
        {

            self.shutdown.cancel();

        }

        result

    }

}

#[cfg(windows)]
pub(crate) mod service {

    #![allow(non_upper_case_globals)]
    /*! SCM service와 SSH stdio relay를 local named pipe로 연결해. */

    use crate::{Bridge, Config};
    use rmcp::ServiceExt;
    use std::{
        ffi::OsString,
        io,
        path::{Path, PathBuf},
        sync::OnceLock,
        time::Duration,
    };
    use tokio::net::windows::named_pipe::{NamedPipeClient, NamedPipeServer, ServerOptions};
    use tokio_util::sync::CancellationToken;
    use windows_service::{
        define_windows_service,
        service::{
            ServiceAccess, ServiceAction, ServiceActionType, ServiceControl, ServiceControlAccept, ServiceErrorControl,
            ServiceExitCode, ServiceFailureActions, ServiceFailureResetPeriod, ServiceInfo, ServiceStartType,
            ServiceState, ServiceStatus, ServiceType,
        },
        service_control_handler::{self, ServiceControlHandlerResult, ServiceStatusHandle},
        service_dispatcher,
        service_manager::{ServiceManager, ServiceManagerAccess},
    };

    type Failure = Box<dyn std::error::Error + Send + Sync>;
    const service_name: &str = "w7bridge";
    const pipe_name: &str = r"\\.\pipe\w7bridge.v1";
    static config_path: OnceLock<PathBuf> = OnceLock::new();
    static failure_stage: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

    define_windows_service!(ffi_service_main, service_main);

    /** SCM에서만 호출한다. config와 pipe ACL을 적용하지 못하면 host를 시작하지 않는다. */
    pub fn dispatch(path: PathBuf) -> Result<(), Failure> {

        config_path.set(path).map_err(|_| "service config가 이미 설정되었습니다")?;
        service_dispatcher::start(service_name, ffi_service_main)?;
        Ok(())

    }

    fn status(state: ServiceState, code: u32) -> ServiceStatus {

        ServiceStatus {

            service_type: ServiceType::OWN_PROCESS,
            current_state: state,
            controls_accepted: if state == ServiceState::Running {

                ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN

            } else {

                ServiceControlAccept::empty()

            },
            exit_code: if code == 0 { ServiceExitCode::Win32(0) } else { ServiceExitCode::ServiceSpecific(code) },
            checkpoint: if matches!(state, ServiceState::StartPending | ServiceState::StopPending) { 1 } else { 0 },
            wait_hint: Duration::from_secs(15),
            process_id: None,

        }

    }

    fn service_main(_args: Vec<OsString>) {

        let shutdown = CancellationToken::new();
        let stop = shutdown.clone();
        let handle = match service_control_handler::register(service_name, move |control| match control {

            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            ServiceControl::Stop | ServiceControl::Shutdown => {

                stop.cancel();
                ServiceControlHandlerResult::NoError

            }
            _ => ServiceControlHandlerResult::NotImplemented,

        }) {

            Ok(handle) => handle,
            Err(_) => return,

        };
        let result = host(handle, shutdown);
        if let Err(error) = &result {

            eprintln!("service host를 시작하거나 유지할 수 없습니다: {error}");
            record_failure(&error.to_string());

        }
        let _ = handle.set_service_status(status(
            ServiceState::Stopped,
            if result.is_err() { failure_stage.load(std::sync::atomic::Ordering::Relaxed) } else { 0 },
        ));

    }

    fn record_failure(message: &str) {

        use std::io::Write;
        let Some(path) = config_path.get() else { return };
        let destination = path.with_extension("service-error.log");
        if std::fs::symlink_metadata(&destination).is_ok_and(|metadata| metadata.is_symlink()) {

            return;

        }
        // 최신 오류 한 건만 남겨. 읽기 전용 config 위치에서는 SCM stage code만 사용해.
        if let Ok(mut file) = std::fs::OpenOptions::new().write(true).create(true).truncate(true).open(destination) {

            let mut end = message.len().min(8192);
            while !message.is_char_boundary(end) {

                end -= 1;

            }
            let _ = writeln!(file, "service host: {}", &message[..end]);

        }

    }

    fn host(handle: ServiceStatusHandle, shutdown: CancellationToken) -> Result<(), Failure> {

        handle.set_service_status(status(ServiceState::StartPending, 0))?;
        failure_stage.store(2, std::sync::atomic::Ordering::Relaxed);
        let path = config_path.get().ok_or("service config 경로가 없습니다")?;
        failure_stage.store(3, std::sync::atomic::Ordering::Relaxed);
        let config = Config::load(path).map_err(|error| format!("설정 읽기: {error}"))?;
        if config.service.desktop {

            return Err("desktop 설정은 SCM host에서 실행하지 않습니다".into());

        }
        let sid = config.service.allowed_sid.clone().ok_or("service.allowed_sid를 설정하세요")?;
        failure_stage.store(4, std::sync::atomic::Ordering::Relaxed);
        let bridge = Bridge::new(config, shutdown.clone()).map_err(|error| format!("registry 검증: {error}"))?;
        failure_stage.store(5, std::sync::atomic::Ordering::Relaxed);
        let runtime = tokio::runtime::Runtime::new().map_err(|error| format!("runtime 시작: {error}"))?;
        runtime.block_on(async {

            failure_stage.store(6, std::sync::atomic::Ordering::Relaxed);
            let listener = pipe(&sid, true).map_err(|error| format!("pipe 생성: {error}"))?;
            handle.set_service_status(status(ServiceState::Running, 0))?;
            failure_stage.store(7, std::sync::atomic::Ordering::Relaxed);
            let result = listen(bridge.clone(), sid, listener, shutdown.clone(), false).await;
            shutdown.cancel();
            let _ = handle.set_service_status(status(ServiceState::StopPending, 0));
            bridge.shutdown().await;
            result

        })

    }

    fn pipe(sid: &str, first: bool) -> io::Result<NamedPipeServer> {

        pipe_for(sid, first, false)

    }

    fn pipe_for( sid: &str, first: bool, desktop: bool, ) -> io::Result<NamedPipeServer> {

        use windows_sys::Win32::{
            Foundation::LocalFree,
            Security::{Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW, SECURITY_ATTRIBUTES},
        };
        // SCM client에는 data 권한만 줘. desktop mode에서는 실제 host owner가 다음 listener도 만들어.
        let owner_access = if desktop { "GA" } else { "0x0012019b" };
        let sddl: Vec<u16> = format!("D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;LS)(A;;{owner_access};;;{sid})")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let mut descriptor = std::ptr::null_mut();
        // SAFETY: 문자열은 NUL 종료 UTF-16이고 출력 pointer는 유효한 local 변수야.
        if unsafe {

            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                1,
                &mut descriptor,
                std::ptr::null_mut(),
            )

        } == 0
        {

            return Err(io::Error::last_os_error());

        }
        let mut attributes = SECURITY_ATTRIBUTES {

            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,

        };
        let mut options = ServerOptions::new();
        options.first_pipe_instance(first).reject_remote_clients(true).max_instances(17);
        // SAFETY: SECURITY_ATTRIBUTES와 descriptor는 create call 동안 살아 있고 descriptor는 Windows가 생성했어.
        let result = unsafe {

            options.create_with_security_attributes_raw(pipe_name, (&mut attributes as *mut SECURITY_ATTRIBUTES).cast())

        };
        // SAFETY: Windows가 LocalAlloc으로 반환한 descriptor를 한 번만 해제해. CreateNamedPipe는 descriptor를 복사해.
        unsafe {

            LocalFree(descriptor);

        }
        result

    }

    async fn listen(
        bridge: Bridge,
        sid: String,
        mut listener: NamedPipeServer,
        shutdown: CancellationToken,
        desktop: bool,
    ) -> Result<(), Failure> {

        let slots = std::sync::Arc::new(tokio::sync::Semaphore::new(16));
        let mut tasks = tokio::task::JoinSet::new();
        let outcome: Result<(), Failure> = loop {

            let permit = tokio::select! {
                _ = shutdown.cancelled() => break Ok(()),
                result = slots.clone().acquire_owned() => match result { Ok(permit) => permit, Err(error) => break Err(error.into()) },
            };
            let result = tokio::select! {
                _ = shutdown.cancelled() => break Ok(()),
                result = listener.connect() => result,
            };
            if let Err(error) = result {

                break Err(error.into());

            }
            let next = match pipe_for(&sid, false, desktop) {

                Ok(next) => next,
                Err(error) => break Err(error.into()),

            };
            let connected = std::mem::replace(&mut listener, next);
            let handler = bridge.clone();
            let token = shutdown.clone();
            tasks.spawn(async move {

                let _permit = permit;
                let (input, output) = tokio::io::split(connected);
                let initialization = tokio::select! {
                    _ = token.cancelled() => return,
                    result = tokio::time::timeout(Duration::from_secs(30), handler.serve((input, output))) => result,
                };
                if let Ok(Ok(service)) = initialization {

                    let cancellation = service.cancellation_token();
                    let waiting = service.waiting();
                    tokio::pin!(waiting);
                    tokio::select! {
                        _ = &mut waiting => {},
                        _ = token.cancelled() => { cancellation.cancel(); let _ = waiting.await; },
                    }

                }

            });
            while tasks.try_join_next().is_some() {}

        };
        shutdown.cancel();
        while tasks.join_next().await.is_some() {}
        outcome

    }

    /** 명시적인 owner opt-in으로 로그인 session에서 같은 persistent pipe host를 실행한다. SCM으로 fallback하지 않는다. */
    pub async fn desktop_host( path: &Path, ) -> Result<(),Failure> {

        let config = Config::load(path)?;
        if !config.service.desktop {

            return Err("desktop host에는 service.desktop = true가 필요합니다".into());

        }
        let sid = crate::platform::desktop_owner()?;
        if config.service.allowed_sid.as_deref() != Some(&sid) {

            return Err("desktop host의 실제 계정과 service.allowed_sid가 다릅니다".into());

        }
        let shutdown = CancellationToken::new();
        let bridge = Bridge::new(config, shutdown.clone())?;
        let listener = pipe_for(&sid, true, true)?;
        let serving = listen(bridge.clone(), sid, listener, shutdown.clone(), true);
        tokio::pin!(serving);
        let result = tokio::select! {
            result = &mut serving => result,
            signal = tokio::signal::ctrl_c() => {
                match signal {
                    Ok(()) => { shutdown.cancel(); serving.await },
                    Err(error) => {
                        eprintln!("Ctrl-C listener를 사용할 수 없어 host 종료를 기다립니다: {error}");
                        serving.await
                    },
                }
            },
        };
        shutdown.cancel();
        bridge.shutdown().await;
        result

    }

    /** SSH 계정의 stdio를 ACL로 제한된 local service pipe에 전달한다. service 실패를 우회하지 않는다. */
    pub async fn relay() -> Result<(), Failure> {

        let mut client = tokio::time::timeout(Duration::from_secs(10), async {

            loop {

                match open_client() {

                    Ok(client) => return Ok::<_, io::Error>(client),
                    Err(error) if error.raw_os_error() == Some(231) => {

                        tokio::time::sleep(Duration::from_millis(100)).await

                    }
                    Err(error) => return Err(error),

                }

            }

        })
        .await??;
        let (mut reader, mut writer) = tokio::io::split(&mut client);
        let mut input = tokio::io::stdin();
        let mut output = tokio::io::stdout();
        let upload = tokio::io::copy(&mut input, &mut writer);
        let download = tokio::io::copy(&mut reader, &mut output);
        let interrupt = async {

            if let Err(error) = tokio::signal::ctrl_c().await {

                eprintln!("Ctrl-C listener를 사용할 수 없어 transport 종료를 기다립니다: {error}");
                std::future::pending::<()>().await;

            }

        };
        tokio::pin!(upload, download, interrupt);
        tokio::select! {
            result = upload => {
                let _bytes = result?;
                #[cfg(debug_assertions)]
                eprintln!("relay stdin 종료: {_bytes} bytes");
            },
            result = download => {
                let _bytes = result?;
                #[cfg(debug_assertions)]
                eprintln!("relay service pipe 종료: {_bytes} bytes");
            },
            _ = interrupt => {},
        }
        Ok(())

    }

    fn open_client() -> io::Result<NamedPipeClient> {

        use windows_sys::Win32::{
            Foundation::INVALID_HANDLE_VALUE,
            Storage::FileSystem::{
                CreateFileW, FILE_FLAG_OVERLAPPED, FILE_READ_DATA, FILE_WRITE_DATA, OPEN_EXISTING,
                SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT,
            },
        };
        let name: Vec<u16> = pipe_name.encode_utf16().chain(Some(0)).collect();
        // SAFETY: 문자열은 NUL 종료야. 읽기/쓰기 data만 요청하고 service가 caller를 impersonate하지 못하게 제한해.
        let raw = unsafe {

            CreateFileW(
                name.as_ptr(),
                FILE_READ_DATA | FILE_WRITE_DATA | 0x00100000,
                0,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                std::ptr::null_mut(),
            )

        };
        if raw == INVALID_HANDLE_VALUE {

            return Err(io::Error::last_os_error());

        }
        // SAFETY: raw는 이번 call에서 획득한 overlapped named pipe handle이고 다른 owner가 없어.
        unsafe { NamedPipeClient::from_raw_handle(raw.cast()) }

    }

    /** 관리자 CLI에서 자동 시작 service를 만든다. 기존 service는 덮어쓰지 않는다. */
    pub fn install(path: &Path) -> Result<(), Failure> {

        let path = path.canonicalize()?;
        let config = Config::load(&path)?;
        if config.service.desktop {

            return Err("desktop 설정은 SCM service가 아니라 로그인 task에서 실행하세요".into());

        }
        if config.service.allowed_sid.is_none() {

            return Err("service.allowed_sid를 먼저 설정하세요".into());

        }
        let _ = Bridge::new(config, CancellationToken::new())?;
        let manager = ServiceManager::local_computer(
            None::<&str>,
            ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
        )?;
        let info = ServiceInfo {

            name: service_name.into(),
            display_name: "w7bridge".into(),
            service_type: ServiceType::OWN_PROCESS,
            start_type: ServiceStartType::AutoStart,
            error_control: ServiceErrorControl::Normal,
            executable_path: std::env::current_exe()?,
            launch_arguments: vec!["service".into(), "--config".into(), path.into_os_string()],
            dependencies: vec![],
            account_name: Some("NT AUTHORITY\\LocalService".into()),
            account_password: None,

        };
        let service = manager.create_service(&info, ServiceAccess::START | ServiceAccess::CHANGE_CONFIG)?;
        service.update_failure_actions(ServiceFailureActions {

            reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(86_400)),
            reboot_msg: None,
            command: None,
            actions: Some(
                [5, 10, 30]
                    .into_iter()
                    .map(|seconds| ServiceAction {

                        action_type: ServiceActionType::Restart,
                        delay: Duration::from_secs(seconds),

                    })
                    .collect(),
            ),

        })?;
        service.set_failure_actions_on_non_crash_failures(true)?;
        service.start::<&str>(&[])?;
        Ok(())

    }

}
