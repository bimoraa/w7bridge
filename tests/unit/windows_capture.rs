use super::*;

#[tokio::test]
async fn native_cleanup_accepts_an_already_removed_task() {

    let root = tempfile::Builder::new().prefix("w7bridge-capture-").tempdir().unwrap();
    let task = literal(root.path().file_name().unwrap().to_str().unwrap());
    run_command(command(&cleanup_script(&task)).unwrap(), CancellationToken::new()).await.unwrap();

}

#[test]
fn shell_literals_cannot_change_the_script_and_worker_cleans_up_its_task() {

    let path = literal("C:/Users/test'&whoami/capture.png");
    assert_eq!(path, "'C:/Users/test''&whoami/capture.png'");
    let script = worker_script(&path, "'w7bridge-capture-fixture'", 2);
    assert!(script.contains("Move-Item -LiteralPath $partial"));
    assert!(script.contains("Unregister-ScheduledTask -TaskName 'w7bridge-capture-fixture'"));
    assert!(script.contains("SessionId -eq 0"));
    let decoded = STANDARD.decode(encode(&script)).unwrap();
    let utf16: Vec<_> = decoded.chunks_exact(2).map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]])).collect();
    assert_eq!(String::from_utf16(&utf16).unwrap(), script);

}
