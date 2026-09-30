/*! 고정 cwd·인자와 환경 변수 allowlist를 적용해. OS sandbox는 아니야. */

use crate::config::CommandDefinition;
use std::{path::Path, process::Stdio};
pub(super) fn configure(command: &mut tokio::process::Command, root: &Path, definition: &CommandDefinition) {

    command.args(&definition.args).current_dir(root).env_clear();

    // 인증 토큰을 자동 상속하지 않고 OS와 toolchain에 필요한 항목만 넘겨.
    for key in
        ["PATH", "SystemRoot", "WINDIR", "TEMP", "TMP", "TMPDIR", "USERPROFILE", "HOME", "CARGO_HOME", "RUSTUP_HOME"]
    {

        if let Some(value) = std::env::var_os(key) {

            command.env(key, value);

        }

    }

    command.envs(&definition.env).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());

}
