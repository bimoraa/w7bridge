use crate::InstallError;
use std::{collections::BTreeSet, ffi::OsString, path::PathBuf};
pub(crate) struct InstallOptions {

    pub directory: PathBuf,
    pub config: Option<PathBuf>,
    pub update: bool,

}

impl InstallOptions {

    pub(crate) fn parse(args: &[OsString]) -> Result<Self, InstallError> {

        let mut options = Self { directory: "C:/w7bridge".into(), config: None, update: false };
        let mut seen = BTreeSet::new();
        let mut args = args.iter();
        while let Some(flag) = args.next() {

            if !seen.insert(flag) {

                return Err(InstallError::Argument("옵션이 중복되었습니다"));

            }
            match flag.to_str() {

                Some("--update") => options.update = true,
                Some("--dir" | "--config") => {

                    let value = args
                        .next()
                        .filter(|value| !value.is_empty() && !value.to_string_lossy().starts_with('-'))
                        .ok_or(InstallError::Argument("옵션 값이 필요합니다"))?;
                    if flag == "--dir" {

                        options.directory = value.into();

                    } else {

                        options.config = Some(value.into());

                    }

                }
                _ => return Err(InstallError::Argument("알 수 없는 옵션입니다")),

            }

        }
        let directory = options.directory.to_str().ok_or(InstallError::Argument("UTF-8 설치 경로가 필요합니다"))?;
        let bytes = directory.as_bytes();
        if bytes.len() < 4
            || !bytes[0].is_ascii_alphabetic()
            || bytes[1] != b':'
            || !b"/\\".contains(&bytes[2])
            || !bytes[3..].iter().all(|byte| byte.is_ascii_alphanumeric() || b"._-/\\".contains(byte))
            || directory[3..]
                .split(['/', '\\'])
                .any(|part| part.is_empty() || part == "." || part == ".." || part.ends_with('.'))
        {

            return Err(InstallError::Argument(
                "설치 경로는 공백·shell 문자·상대 요소가 없는 절대 drive 경로여야 합니다",
            ));

        }
        Ok(options)

    }

}
