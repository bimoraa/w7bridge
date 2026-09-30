use super::handshake::check_transport;
use super::transport::Options;
use crate::ConnectError;
use crate::{Bridge, Config};
use rmcp::ServiceExt;
use rmcp::{
    ServerHandler,
    model::{Implementation, ServerCapabilities, ServerConfig},
};
use std::{ffi::OsString, time::Duration};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

fn options(args: &[&str]) -> Result<Options, ConnectError> {

    Options::parse(&args.iter().map(OsString::from).collect::<Vec<_>>())

}

#[test]
fn ssh_config_preserves_security_flags_alias_settings_and_argument_boundaries() {

    let parsed = options(&[
        "--host",
        "bridge@windows-host",
        "--identity",
        "/tmp/key with spaces",
        "--port",
        "2222",
        "--name",
        "windows_dev",
        "--print-config",
    ])
    .unwrap();
    let args = parsed.ssh_args();
    assert!(args.windows(2).any(|pair| pair == ["-o", "BatchMode=yes"]));
    assert!(args.windows(2).any(|pair| pair == ["-o", "StrictHostKeyChecking=yes"]));
    for setting in [
        "PreferredAuthentications=publickey",
        "PasswordAuthentication=no",
        "KbdInteractiveAuthentication=no",
        "ForwardAgent=no",
        "ForwardX11=no",
        "ClearAllForwardings=yes",
        "RequestTTY=no",
    ] {

        assert!(args.windows(2).any(|pair| pair == ["-o", setting]));

    }
    assert!(args.windows(2).any(|pair| pair == ["-i", "/tmp/key with spaces"]));
    assert!(args.windows(2).any(|pair| pair == ["-p", "2222"]));
    assert_eq!(
        &args[args.len() - 3..],
        ["--", "bridge@windows-host", "C:/w7bridge/w7bridge.exe --config C:/w7bridge/w7bridge.toml"]
    );
    let config: toml::Value = toml::from_str(&parsed.codex_config().unwrap()).unwrap();
    let entry = &config["mcp_servers"]["windows_dev"];
    assert_eq!(entry["command"].as_str(), Some("ssh"));
    assert_eq!(entry["args"].as_array().unwrap().iter().map(|value| value.as_str().unwrap()).collect::<Vec<_>>(), args);
    let alias = options(&["--host", "windows"]).unwrap().ssh_args();
    assert!(!alias.iter().any(|arg| arg == "-p" || arg == "-i"));

}

#[test]
fn unsafe_remote_shell_inputs_and_ambiguous_options_are_rejected() {

    for args in [
        vec![],
        vec!["--host", "-oProxyCommand=anything"],
        vec!["--host", "host;whoami"],
        vec!["--host", "host\nother"],
        vec!["--host", "windows", "--port", "0"],
        vec!["--host", "windows", "--port", "65536"],
        vec!["--host", "windows", "--timeout", "121"],
        vec!["--host", "windows", "--name", "a.b"],
        vec!["--host", "windows", "--host", "other"],
        vec!["--host", "windows", "--check", "--print-config"],
        vec!["--host", "windows", "--config"],
        vec!["--host", "windows", "--config", "relative.toml"],
        vec!["--host", "windows", "--config", "C:/bridge/config.toml&whoami"],
        vec!["--host", "windows", "--config", "C:/bridge/%TEMP%.toml"],
        vec!["--host", "windows", "--executable", "C:/Program Files/bridge.exe"],
        vec!["--host", "windows", "--executable", "C:/bridge/run.cmd"],
        vec!["--host", "windows", "--identity", "key\0file"],
    ] {

        assert!(matches!(options(&args), Err(ConnectError::Argument(_))), "{args:?}");

    }

}

#[tokio::test]
async fn probe_initializes_discovers_and_lists_without_running_commands() {

    let (client, server) = tokio::io::duplex(8192);
    let bridge = Bridge::new(Config::parse("version = 1\nprojects = []").unwrap(), CancellationToken::new()).unwrap();
    let server = tokio::spawn(async move { bridge.serve(server).await.unwrap().waiting().await.unwrap() });
    let (input, output) = tokio::io::split(client);
    timeout(Duration::from_secs(2), check_transport(input, output)).await.unwrap().unwrap();
    timeout(Duration::from_secs(2), server).await.unwrap().unwrap();

}

#[derive(Clone)]
struct OtherServer;

impl ServerHandler for OtherServer {

    fn get_info(&self) -> ServerConfig {

        ServerConfig::new(ServerCapabilities::default()).with_server_info(Implementation::new("other", "1"))

    }

}

#[tokio::test]
async fn probe_rejects_another_mcp_server() {

    let (client, server) = tokio::io::duplex(8192);
    let server = tokio::spawn(async move { OtherServer.serve(server).await.unwrap().waiting().await.unwrap() });
    let (input, output) = tokio::io::split(client);
    assert!(matches!(
        timeout(Duration::from_secs(2), check_transport(input, output)).await.unwrap(),
        Err(ConnectError::Server)
    ));
    timeout(Duration::from_secs(2), server).await.unwrap().unwrap();

}
