use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[test]
#[ignore = "MCP 테스트가 환경을 준비한 뒤 실행하는 subprocess fixture"]
fn process() {

    let mode = std::env::var("W7BRIDGE_FIXTURE_MODE").expect("fixture 모드가 필요해");
    let root = PathBuf::from(std::env::var_os("W7BRIDGE_FIXTURE_ROOT").expect("fixture root가 필요해"));

    match mode.as_str() {

        "output" => {

            println!("명령 출력 확인");
            println!(
                "{}",
                serde_json::json!({
                    "cwd": std::env::current_dir().unwrap(),
                    "configured_env": mode,
                    "secret_inherited": std::env::var_os("W7BRIDGE_TEST_SECRET").is_some()
                })
            );
            eprintln!("오류 출력 확인");

        }
        "stream" => {

            io::stdout().write_all(b"OUT_1").unwrap();
            io::stdout().flush().unwrap();
            io::stderr().write_all(b"ERR_1").unwrap();
            io::stderr().flush().unwrap();
            thread::sleep(Duration::from_millis(1500));
            io::stdout().write_all(b"OUT_2").unwrap();
            io::stdout().flush().unwrap();
            io::stderr().write_all(b"ERR_2").unwrap();
            io::stderr().flush().unwrap();
            thread::sleep(Duration::from_millis(500));
            fs::write(root.join("stream_done"), "종료").unwrap();

        }
        "failure" => std::process::exit(7),
        "flood" => {

            io::stdout().write_all(&vec![b'x'; 20_000]).unwrap();
            io::stderr().write_all(&vec![b'y'; 20_000]).unwrap();

        }
        "leaf" => {

            fs::write(root.join("leaf_ready"), "준비됨").unwrap();
            thread::sleep(Duration::from_secs(2));
            fs::write(root.join("survived"), "프로세스가 살아 있어").unwrap();

        }
        "tree" => {

            let mut child = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "fixture::process", "--ignored", "--nocapture"])
                .env("W7BRIDGE_FIXTURE_MODE", "leaf")
                .env("W7BRIDGE_FIXTURE_ROOT", &root)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);

            while !root.join("leaf_ready").exists() {

                assert!(Instant::now() < deadline, "자식 fixture가 시작되지 않았어");
                thread::sleep(Duration::from_millis(10));

            }

            fs::write(root.join("ready"), "준비됨").unwrap();
            thread::sleep(Duration::from_secs(30));
            child.wait().unwrap();

        }
        _ => panic!("알 수 없는 fixture 모드야"),

    }

}
