use std::{path::Path, process::Stdio, time::Duration};

use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    time::timeout,
};

pub struct Client {

    process: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,

}

impl Client {

    pub async fn start(root: &Path, mode: &str, execution: Value) -> Self {

        let command = json!({
            "executable": std::env::current_exe().unwrap(),
            "args": ["--exact", "fixture::process", "--ignored", "--nocapture"],
            "env": { "W7BRIDGE_FIXTURE_MODE": mode, "W7BRIDGE_FIXTURE_ROOT": root }
        });
        let mut probe = command.clone();
        probe["env"]["W7BRIDGE_FIXTURE_MODE"] = json!("output");
        let config = json!({
            "version": 1,
            "execution": execution,
            "projects": [{
                "id": "sample", "root": root,
                "commands": { "test": command, "probe": probe }
            }]
        });
        let path = root.join("settings.toml");
        std::fs::write(&path, toml::to_string(&config).unwrap()).unwrap();
        Self::start_config(&path).await

    }

    pub async fn start_config( path: &Path, ) -> Self {

        let mut process = Command::new(env!("CARGO_BIN_EXE_w7bridge"))
            .arg("--config")
            .arg(path)
            .env("W7BRIDGE_TEST_SECRET", "상속 금지")
            .env("CODEX_HOME", path.parent().unwrap().join("codex"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let input = process.stdin.take();
        let output = BufReader::new(process.stdout.take().unwrap());
        let mut client = Self { process, input, output };
        client
            .send(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-11-25", "capabilities": {},
            "clientInfo": { "name": "w7bridge-test", "version": "1" }
        } }))
            .await;
        let initialized = client.response(1).await;
        assert_eq!(initialized["result"]["serverInfo"]["name"], "w7bridge");
        assert_eq!(initialized["result"]["capabilities"]["tools"], json!({}));
        client.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).await;
        client

    }

    pub async fn send(&mut self, value: Value) {

        let bytes = format!("{value}\n");
        let input = self.input.as_mut().unwrap();
        input.write_all(bytes.as_bytes()).await.unwrap();
        input.flush().await.unwrap();

    }

    pub async fn call(&mut self, id: u64, name: &str, arguments: Value) -> Value {

        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {
            "name": name, "arguments": arguments
        } }))
        .await;
        self.response(id).await

    }

    pub async fn response(&mut self, id: u64) -> Value {

        timeout(Duration::from_secs(10), async {

            loop {

                let value = self.message().await;
                if value["id"] == id {

                    return value;

                }

            }

        })
        .await
        .expect("MCP 응답 시간이 초과됐어")

    }

    pub async fn message(&mut self) -> Value {

        let mut line = String::new();
        assert!(self.output.read_line(&mut line).await.unwrap() > 0, "MCP 연결이 닫혔어");
        serde_json::from_str(&line).expect("stdout에 protocol 이외의 텍스트가 있어")

    }

    pub async fn close(mut self) {

        drop(self.input.take());
        let exit = timeout(Duration::from_secs(10), self.process.wait()).await.unwrap().unwrap();
        assert!(exit.success(), "서버가 정상 종료하지 않았어: {exit}");

    }

}
