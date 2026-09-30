use super::*;

#[tokio::test]
async fn screenshot_is_disabled_by_default_and_cancellation_prevents_capture() {

    let disabled = Capture::new(false, CancellationToken::new());
    assert!(matches!(disabled.take(1, CancellationToken::new()).await, Err(CaptureError::Disabled)));
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    let enabled = Capture::new(true, cancelled);
    assert!(matches!(enabled.take(1, CancellationToken::new()).await, Err(CaptureError::Cancelled)));

}

#[test]
fn corrupt_oversized_and_symlink_images_are_rejected() {

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("capture.png");
    std::fs::write(&path, b"not an image").unwrap();
    assert!(matches!(read_image(&path), Err(CaptureError::Data)));
    let file = File::create(&path).unwrap();
    file.set_len(8 * 1024 * 1024 + 1).unwrap();
    assert!(matches!(read_image(&path), Err(CaptureError::Limit)));
    #[cfg(unix)]
    {

        let link = directory.path().join("link.png");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(matches!(read_image(&link), Err(CaptureError::Data)));

    }

}
