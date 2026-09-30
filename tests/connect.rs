use std::process::Command;

#[test]
fn config_preview_is_valid_codex_toml_and_needs_no_connection() {

    let output = Command::new(env!("CARGO_BIN_EXE_w7bridge"))
        .args(["connect", "--host", "windows", "--print-config"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let config: toml::Value = toml::from_str(std::str::from_utf8(&output.stdout).unwrap()).unwrap();
    assert_eq!(config["mcp_servers"]["w7bridge"]["command"].as_str(), Some("ssh"));
    let invalid = Command::new(env!("CARGO_BIN_EXE_w7bridge"))
        .args(["connect", "--host", "host;whoami", "--print-config"])
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    assert!(invalid.stdout.is_empty());

}

#[cfg(unix)]
mod ssh {

    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt, path::Path};
    use tempfile::{TempDir, tempdir};

    fn script(root: &Path, name: &str, body: &str) {

        let path = root.join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();

    }

    fn setup(ssh: &str) -> (TempDir, Command) {

        let root = tempdir().unwrap();
        script(root.path(), "ssh", ssh);
        script(
            root.path(),
            "codex",
            r#"
case "$1 $2" in
  "mcp list")
    if [ "$W7BRIDGE_TEST_EXISTS" = yes ]; then
      printf '[{"name":"w7bridge"}]\n'
    else
      printf '[]\n'
    fi ;;
  "mcp add") printf '%s\n' "$@" > "$W7BRIDGE_TEST_ADD" ;;
  *) exit 1 ;;
esac
"#,
        );
        let settings = root.path().join("server.toml");
        fs::write(&settings, "version = 1\nprojects = []\n").unwrap();
        let mut paths = vec![root.path().to_path_buf()];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
        let mut command = Command::new(env!("CARGO_BIN_EXE_w7bridge"));
        command
            .env("PATH", std::env::join_paths(paths).unwrap())
            .env("W7BRIDGE_TEST_SERVER", env!("CARGO_BIN_EXE_w7bridge"))
            .env("W7BRIDGE_TEST_CONFIG", settings)
            .env("W7BRIDGE_TEST_ADD", root.path().join("added"))
            .env("W7BRIDGE_TEST_SSH_ARGS", root.path().join("ssh_args"))
            .env("W7BRIDGE_TEST_PID", root.path().join("pid"))
            .env("W7BRIDGE_TEST_EXISTS", "no")
            .args(["connect", "--host", "windows", "--timeout", "2"]);
        (root, command)

    }

    #[test]
    fn verified_connection_registers_the_same_ssh_command_and_check_leaves_config_alone() {

        let (root, mut command) = setup(
            r#"
printf '%s\n' "$@" > "$W7BRIDGE_TEST_SSH_ARGS"
exec "$W7BRIDGE_TEST_SERVER" --config "$W7BRIDGE_TEST_CONFIG"
"#,
        );
        let checked = command.arg("--check").output().unwrap();
        assert!(checked.status.success(), "{}", String::from_utf8_lossy(&checked.stderr));
        assert!(!root.path().join("added").exists());

        let (root, mut command) = setup(
            r#"
printf '%s\n' "$@" > "$W7BRIDGE_TEST_SSH_ARGS"
exec "$W7BRIDGE_TEST_SERVER" --config "$W7BRIDGE_TEST_CONFIG"
"#,
        );
        let registered = command.output().unwrap();
        assert!(registered.status.success(), "{}", String::from_utf8_lossy(&registered.stderr));
        let args = fs::read_to_string(root.path().join("ssh_args")).unwrap();
        assert_eq!(
            fs::read_to_string(root.path().join("added")).unwrap(),
            format!("mcp\nadd\nw7bridge\n--\nssh\n{args}")
        );
        assert!(args.contains("StrictHostKeyChecking=yes\n"));

    }

    #[test]
    fn failed_ssh_and_existing_codex_name_do_not_register() {

        let (root, mut command) = setup("exit 255");
        let failed = command.output().unwrap();
        assert!(!failed.status.success());
        assert!(String::from_utf8_lossy(&failed.stderr).contains("MCP 연결 확인에 실패"));
        assert!(!root.path().join("added").exists());

        let (root, mut command) = setup("exit 255");
        let exists = command.env("W7BRIDGE_TEST_EXISTS", "yes").output().unwrap();
        assert!(!exists.status.success());
        assert!(String::from_utf8_lossy(&exists.stderr).contains("이미 있습니다"));
        assert!(!root.path().join("added").exists());
        assert!(!root.path().join("ssh_args").exists());

    }

    #[test]
    fn handshake_timeout_reaps_ssh_and_does_not_register() {

        let (root, mut command) = setup(
            r#"
printf '%s' "$$" > "$W7BRIDGE_TEST_PID"
exec sleep 30
"#,
        );
        let start = std::time::Instant::now();
        let output = command.output().unwrap();
        assert!(!output.status.success());
        assert!(start.elapsed() < std::time::Duration::from_secs(10));
        assert!(String::from_utf8_lossy(&output.stderr).contains("시간이 초과"));
        assert!(!root.path().join("added").exists());
        let pid = fs::read_to_string(root.path().join("pid")).unwrap();
        assert!(!Command::new("kill").args(["-0", &pid]).output().unwrap().status.success());

    }

    #[test]
    fn interrupted_handshake_reaps_ssh_and_does_not_register() {

        let (root, mut command) = setup(
            r#"
printf '%s' "$$" > "$W7BRIDGE_TEST_PID"
exec sleep 30
"#,
        );
        let child = command.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().unwrap();
        let pid_path = root.path().join("pid");
        for _ in 0..100 {

            if pid_path.exists() {

                break;

            }
            std::thread::sleep(std::time::Duration::from_millis(10));

        }
        assert!(pid_path.exists());
        // child의 stdin 연결과 signal 등록이 끝날 시간을 줘.
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(Command::new("kill").args(["-INT", &child.id().to_string()]).status().unwrap().success());
        let output = child.wait_with_output().unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("취소되었습니다"));
        assert!(!root.path().join("added").exists());
        let pid = fs::read_to_string(pid_path).unwrap();
        assert!(!Command::new("kill").args(["-0", &pid]).output().unwrap().status.success());

    }

}
