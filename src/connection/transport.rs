/*! 검증한 SSH 인자와 Codex 설정을 구성해. */

use crate::ConnectError;
use serde::Serialize;
use std::{collections::BTreeSet, ffi::OsString};
#[derive(Debug)]
pub(crate) struct Options {

    pub(super) host: String,
    executable: String,
    config: String,
    pub(super) name: String,
    port: Option<u16>,
    identity: Option<String>,
    pub(super) timeout_seconds: u64,
    pub(super) check: bool,
    pub(super) print_config: bool,
    service: bool,

}

impl Options {

    pub(crate) fn parse(args: &[OsString]) -> Result<Self, ConnectError> {

        let mut options = Self {

            host: String::new(),
            executable: "C:/w7bridge/w7bridge.exe".into(),
            config: "C:/w7bridge/w7bridge.toml".into(),
            name: "w7bridge".into(),
            port: None,
            identity: None,
            timeout_seconds: 30,
            check: false,
            print_config: false,
            service: false,

        };
        let mut seen = BTreeSet::new();
        let mut args = args.iter();
        while let Some(flag) = args.next() {

            let flag = flag.to_str().ok_or(ConnectError::Argument("UTF-8 옵션이 필요합니다"))?;
            if !seen.insert(flag) {

                return Err(ConnectError::Argument("옵션이 중복되었습니다"));

            }
            match flag {

                "--check" => options.check = true,
                "--service" => options.service = true,
                "--print-config" => options.print_config = true,
                "--host" | "--executable" | "--config" | "--name" | "--port" | "--identity" | "--timeout" => {

                    let value = args
                        .next()
                        .and_then(|value| value.to_str())
                        .filter(|value| !value.is_empty() && !value.starts_with("--"))
                        .ok_or(ConnectError::Argument("옵션 값이 필요합니다"))?;
                    match flag {

                        "--host" => options.host = value.into(),
                        "--executable" => options.executable = value.into(),
                        "--config" => options.config = value.into(),
                        "--name" => options.name = value.into(),
                        "--identity" => options.identity = Some(value.into()),
                        "--port" => {

                            options.port = Some(
                                value
                                    .parse()
                                    .ok()
                                    .filter(|port| *port > 0)
                                    .ok_or(ConnectError::Argument("port는 1..=65535여야 합니다"))?,
                            )

                        }
                        "--timeout" => {

                            options.timeout_seconds = value
                                .parse()
                                .ok()
                                .filter(|seconds| (1..=120).contains(seconds))
                                .ok_or(ConnectError::Argument("timeout은 1..=120초여야 합니다"))?

                        }
                        _ => unreachable!(),

                    }

                }
                _ => return Err(ConnectError::Argument("알 수 없는 옵션입니다")),

            }

        }
        if options.host.is_empty()
            || options.host.starts_with('-')
            || !options.host.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._-@:[]".contains(&byte))
            || options.host.matches('@').count() > 1
        {

            return Err(ConnectError::Argument("SSH alias 또는 user@host가 필요합니다"));

        }
        if options.name.is_empty()
            || options.name.len() > 64
            || !options
                .name
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_-".contains(&byte))
        {

            return Err(ConnectError::Argument("name은 1..=64자의 영문 소문자, 숫자, 밑줄, 하이픈이어야 합니다"));

        }
        for path in [&options.executable, &options.config] {

            let bytes = path.as_bytes();
            if bytes.len() < 4
                || !bytes[0].is_ascii_alphabetic()
                || bytes[1] != b':'
                || !b"/\\".contains(&bytes[2])
                || !bytes.iter().all(|byte| byte.is_ascii_alphanumeric() || b"._-:/\\".contains(byte))
            {

                return Err(ConnectError::Argument(
                    "Windows 경로는 공백과 shell 문자가 없는 절대 drive 경로여야 합니다",
                ));

            }

        }
        if !options.executable.to_ascii_lowercase().ends_with(".exe") {

            return Err(ConnectError::Argument("Windows 실행 파일은 .exe여야 합니다"));

        }
        if options.identity.as_ref().is_some_and(|identity| identity.chars().any(char::is_control)) {

            return Err(ConnectError::Argument("identity 경로에 제어 문자를 쓸 수 없습니다"));

        }
        if options.check && options.print_config {

            return Err(ConnectError::Argument("--check와 --print-config는 함께 쓸 수 없습니다"));

        }
        Ok(options)

    }

    pub(crate) fn ssh_args(&self) -> Vec<String> {

        let mut args: Vec<String> = [
            "-T",
            "-o",
            "BatchMode=yes",
            "-o",
            "StrictHostKeyChecking=yes",
            "-o",
            "PreferredAuthentications=publickey",
            "-o",
            "PasswordAuthentication=no",
            "-o",
            "KbdInteractiveAuthentication=no",
            "-o",
            "ForwardAgent=no",
            "-o",
            "ForwardX11=no",
            "-o",
            "ClearAllForwardings=yes",
            "-o",
            "RequestTTY=no",
            "-o",
            "ConnectTimeout=10",
            "-o",
            "ServerAliveInterval=15",
            "-o",
            "ServerAliveCountMax=2",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        if let Some(port) = self.port {

            args.extend(["-p".into(), port.to_string()]);

        }
        if let Some(identity) = &self.identity {

            args.extend(["-i".into(), identity.clone()]);

        }
        args.extend([
            "--".into(),
            self.host.clone(),
            if self.service {

                format!("{} relay", self.executable)

            } else {

                format!("{} --config {}", self.executable, self.config)

            },
        ]);
        args

    }

    pub(crate) fn codex_config(&self) -> Result<String, ConnectError> {

        #[derive(Serialize)]
        struct Entry {

            command: &'static str,
            args: Vec<String>,

        }
        let entry = Entry { command: "ssh", args: self.ssh_args() };
        Ok(format!("[mcp_servers.{}]\n{}", self.name, toml::to_string(&entry)?))

    }

}
