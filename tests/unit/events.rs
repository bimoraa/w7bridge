use super::*;

#[tokio::test]
async fn event_scope_replay_truncation_long_poll_and_cancellation_are_explicit( ) {

    let events = Events::new();
    events.publish("a", "file_synced", json!({"path":"src.rs"}));
    events.publish("b", "secret", json!({"path":"other.rs"}));
    let value = events.read("a", 0, 0, CancellationToken::new()).await.unwrap();
    assert_eq!(value["events"].as_array().unwrap().len(), 1);
    assert_eq!(value["next_cursor"], 2);
    let cursor = value["next_cursor"].as_u64().unwrap();
    let wait = events.read("a", cursor, 2, CancellationToken::new());
    let publish = async {

        tokio::time::sleep(Duration::from_millis(50)).await;
        events.publish("a", "build_done", json!({"exit_code":0}));

    };
    let (result, ()) = tokio::join!(wait, publish);
    assert_eq!(result.unwrap()["events"][0]["kind"], "build_done");
    assert!(events.read("a", 999, 0, CancellationToken::new()).await.is_err());
    let token = CancellationToken::new();
    token.cancel();
    assert!(events.read("a", 0, 0, token).await.is_err());
    for index in 0..1030 {

        events.publish("a", "file_synced", json!({"index":index}));

    }
    let value = events.read("a", 0, 0, CancellationToken::new()).await.unwrap();
    assert_eq!(value["truncated"], true);
    assert!(value["events"].as_array().unwrap().len() <= 1024);
    assert!(events.read("a", 0, 31, CancellationToken::new()).await.is_err());

}
