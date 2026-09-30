use super::*;
use ed25519_dalek::{Signer, SigningKey};
use tempfile::tempdir;

fn settings( installed: PathBuf, key: &SigningKey, ) -> Settings {

    Settings {

        version: 1,
        manifest_url: "https://example.invalid/manifest.json".into(),
        public_key_base64: STANDARD.encode(key.verifying_key().to_bytes()),
        installed_executable: installed,
        service: false,
        interval_seconds: 60,

    }

}

fn manifest( key: &SigningKey, sequence: u64, target: String, bytes: &[u8], ) -> Vec<u8> {

    let release = Release {

        sequence,
        version: "0.1.0".into(),
        target,
        artifact_url: "https://example.invalid/binary".into(),
        sha256: digest(bytes),
        bytes: bytes.len(),
        protocol_min: 1,
        protocol_max: 2,

    };
    let payload = serde_json::to_vec(&release).unwrap();
    serde_json::to_vec(&Signed {

        payload_base64: STANDARD.encode(&payload),
        signature_base64: STANDARD.encode(key.sign(&payload).to_bytes()),

    })
    .unwrap()

}

#[test]
fn signed_manifest_rejects_tampering_wrong_key_downgrade_and_wrong_platform( ) {

    let key = SigningKey::from_bytes(&[7; 32]);
    let other = SigningKey::from_bytes(&[8; 32]);
    let settings = settings("/unused/w7bridge".into(), &key);
    let target = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
    let bytes = manifest(&key, 2, target.clone(), b"candidate");
    assert_eq!(verify(&bytes, &settings, 1).unwrap().sequence, 2);
    assert!(verify(&bytes, &settings, 3).is_err());
    assert!(verify(&manifest(&other, 2, target, b"candidate"), &settings, 1).is_err());
    assert!(verify(&manifest(&key, 2, "another-platform".into(), b"candidate"), &settings, 1).is_err());
    let mut altered: Signed = serde_json::from_slice(&bytes).unwrap();
    altered.payload_base64 = STANDARD.encode(b"altered");
    assert!(verify(&serde_json::to_vec(&altered).unwrap(), &settings, 1).is_err());
    assert!(https("http://example.invalid/binary").is_err());
    assert!(https("https://secret@example.invalid/binary").is_err());

}

#[tokio::test]
async fn interrupted_replace_recovers_binary_and_preserves_config_and_newer_changes( ) {

    let root = tempdir().unwrap();
    let installed = root.path().join(if cfg!(windows) { "w7bridge.exe" } else { "w7bridge" });
    fs::write(&installed, b"old binary").unwrap();
    fs::write(root.path().join("w7bridge.toml"), b"owner config").unwrap();
    let settings = settings(installed.clone(), &SigningKey::from_bytes(&[7; 32]));
    let directory = root.path().join(".w7bridge-update");
    fs::create_dir(&directory).unwrap();
    let backup = directory.join(if cfg!(windows) { "previous.exe" } else { "previous" });
    fs::write(&backup, b"old binary").unwrap();
    let mut state = State {

        pending: Some(Pending {

            previous_hash: digest(b"old binary"),
            next_hash: digest(b"new binary"),
            sequence: 2,
            phase: "replaced".into(),
            was_running: false,

        }),
        ..Default::default()

    };
    fs::write(&installed, b"new binary").unwrap();
    save_state(&directory, &state).unwrap();
    state = read_state(&directory).unwrap();
    recover(&settings, &directory, &mut state).await.unwrap();
    assert_eq!(fs::read(&installed).unwrap(), b"old binary");
    assert!(state.pending.is_none());
    assert_eq!(fs::read(root.path().join("w7bridge.toml")).unwrap(), b"owner config");
    state.pending = Some(Pending {

        previous_hash: digest(b"old binary"),
        next_hash: digest(b"new binary"),
        sequence: 2,
        phase: "replaced".into(),
        was_running: false,

    });
    fs::write(&installed, b"newer owner binary").unwrap();
    assert!(recover(&settings, &directory, &mut state).await.is_err());
    assert_eq!(fs::read(&installed).unwrap(), b"newer owner binary");

}
