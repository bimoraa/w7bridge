/*! desktop host가 실제 실행 계정과 session을 확인해. 환경 변수의 계정 이름은 신뢰하지 않아. */

use std::{io, mem::size_of};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, LocalFree},
    Security::{Authorization::ConvertSidToStringSidW, GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser},
    System::{
        RemoteDesktop::ProcessIdToSessionId,
        Threading::{GetCurrentProcess, GetCurrentProcessId, OpenProcessToken},
    },
};

struct Token(HANDLE);
impl Drop for Token {

    fn drop( &mut self, ) {

        // SAFETY: OpenProcessToken으로 얻은 handle을 이 guard가 한 번만 닫아.
        unsafe {

            CloseHandle(self.0);

        }

    }

}

#[cfg(test)]
#[path = "../../../tests/unit/windows_identity.rs"]
mod tests;

struct SidText(*mut u16);
impl Drop for SidText {

    fn drop( &mut self, ) {

        // SAFETY: ConvertSidToStringSidW가 할당한 메모리를 한 번만 반환해.
        unsafe {

            LocalFree(self.0.cast());

        }

    }

}

pub(crate) fn desktop_owner( ) -> io::Result<String> {

    let mut session = 0;
    // SAFETY: 현재 PID와 유효한 출력 pointer만 전달해.
    if unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) } == 0 {

        return Err(io::Error::last_os_error());

    }
    if session == 0 {

        return Err(io::Error::other("desktop host는 로그인한 사용자 session에서만 실행하세요"));

    }
    current_sid()

}

fn current_sid( ) -> io::Result<String> {

    let mut raw = std::ptr::null_mut();
    // SAFETY: 현재 process의 token을 조회 권한으로만 열어.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) } == 0 {

        return Err(io::Error::last_os_error());

    }
    let token = Token(raw);
    let mut length = 0;
    // SAFETY: null buffer와 0 크기로 필요한 byte 수만 조회해.
    unsafe {

        GetTokenInformation(token.0, TokenUser, std::ptr::null_mut(), 0, &mut length);

    }
    if !(size_of::<TOKEN_USER>() as u32..=65536).contains(&length) {

        return Err(io::Error::other("계정 token 크기가 올바르지 않습니다"));

    }
    let mut buffer = vec![0usize; (length as usize).div_ceil(size_of::<usize>())];
    // SAFETY: TOKEN_USER 정렬을 만족하며 조회된 길이 이상의 쓰기 buffer야.
    if unsafe { GetTokenInformation(token.0, TokenUser, buffer.as_mut_ptr().cast(), length, &mut length) } == 0 {

        return Err(io::Error::last_os_error());

    }
    // SAFETY: 성공한 TokenUser 응답이며 buffer가 SID 변환 완료까지 살아 있어.
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    let mut sid = std::ptr::null_mut();
    // SAFETY: token의 유효한 SID와 출력 pointer를 전달해.
    if unsafe { ConvertSidToStringSidW(user.User.Sid, &mut sid) } == 0 {

        return Err(io::Error::last_os_error());

    }
    let sid = SidText(sid);
    let mut length = 0;
    // SAFETY: API가 NUL 종료 문자열을 반환했으며 guard가 아직 메모리를 소유해.
    unsafe {

        while *sid.0.add(length) != 0 {

            length += 1;

        }
        String::from_utf16(std::slice::from_raw_parts(sid.0, length)).map_err(io::Error::other)

    }

}
