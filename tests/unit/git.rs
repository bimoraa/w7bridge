use super::*;
use crate::filesystem::FileSettings;
use tempfile::tempdir;

fn git( root: &Path, args: &[&str], ) -> Vec<u8> {

    let output = std::process::Command::new(executable())
        .current_dir(root)
        .args(["-c", "user.name=fixture", "-c", "user.email=fixture@example.invalid", "-c", "core.autocrlf=false"])
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    output.stdout

}

fn executable( ) -> PathBuf {

    std::env::var_os("W7BRIDGE_TEST_GIT").map(PathBuf::from).unwrap_or_else(|| {

        if cfg!(windows) { "C:/Program Files/Git/cmd/git.exe".into() } else { "/usr/bin/git".into() }

    })

}

fn repository( root: &Path, ) -> Repository {

    Repository::new(FileStore::new(root, FileSettings { enabled: true, ..Default::default() }).unwrap(), executable())

}

async fn handoff( source: &Repository, destination: &Repository, expected: Option<String>, ) -> Result<Value,String> {

    let token = CancellationToken::new();
    let state = source.status(token.clone()).await.unwrap();
    let description = source.export(state["state_hash"].as_str().unwrap(), token.clone()).await.unwrap();
    let sha = description["sha256"].as_str().unwrap();
    let prepared = destination
        .prepare_import(sha, description["bytes"].as_u64().unwrap() as usize, expected, destination.manifest().unwrap())
        .unwrap();
    let id = prepared["transfer_id"].as_str().unwrap();
    for index in prepared["missing"].as_array().unwrap() {

        let index = index.as_u64().unwrap() as usize;
        let chunk = source.read_export(sha, index).unwrap();
        destination
            .put_import(
                id,
                index,
                &STANDARD.decode(chunk["content_base64"].as_str().unwrap()).unwrap(),
                chunk["sha256"].as_str().unwrap(),
            )
            .unwrap();

    }
    destination.apply_import(id, token).await

}

fn copy_worktree( source: &Repository, destination: &Repository, ) {

    let left = source.files.list().unwrap();
    let right = destination.files.list().unwrap();
    for entry in &left {

        let expected = right.iter().find(|other| other.path == entry.path).map(|entry| entry.sha256.as_str());
        destination.files.write(&entry.path, Some(&source.files.read(&entry.path).unwrap()), expected).unwrap();

    }
    for entry in right {

        if !left.iter().any(|other| other.path == entry.path) {

            destination.files.write(&entry.path, None, Some(&entry.sha256)).unwrap();

        }

    }

}

#[tokio::test]
async fn bundle_index_and_worktree_handoff_both_directions_preserve_new_destination_changes( ) {

    let source_root = tempdir().unwrap();
    let target_root = tempdir().unwrap();
    let source = repository(source_root.path());
    let destination = repository(target_root.path());
    git(source_root.path(), &["init", "--quiet", "-b", "main"]);
    fs::write(source_root.path().join(".gitignore"), ".w7bridge/\n.env\n").unwrap();
    fs::write(source_root.path().join("tracked.txt"), "initial").unwrap();
    fs::write(source_root.path().join("deleted.txt"), "delete me").unwrap();
    git(source_root.path(), &["add", "."]);
    git(source_root.path(), &["commit", "--quiet", "-m", "fixture"]);
    git(source_root.path(), &["remote", "add", "origin", "https://token:secret@example.invalid/repo.git"]);
    fs::write(source_root.path().join("tracked.txt"), "staged").unwrap();
    git(source_root.path(), &["add", "tracked.txt"]);
    fs::write(source_root.path().join("tracked.txt"), "unstaged").unwrap();
    git(source_root.path(), &["mv", "deleted.txt", "renamed.txt"]);
    fs::remove_file(source_root.path().join("renamed.txt")).unwrap();
    fs::write(source_root.path().join("new.txt"), "untracked").unwrap();
    fs::write(source_root.path().join(".env"), "secret must stay local").unwrap();
    copy_worktree(&source, &destination);
    handoff(&source, &destination, None).await.unwrap();
    assert_eq!(
        git(target_root.path(), &["status", "--porcelain=v1", "--untracked-files=all"]),
        git(source_root.path(), &["status", "--porcelain=v1", "--untracked-files=all"])
    );
    assert_eq!(git(target_root.path(), &["rev-parse", "HEAD"]), git(source_root.path(), &["rev-parse", "HEAD"]));
    assert_eq!(git(target_root.path(), &["show", ":tracked.txt"]), b"staged");
    assert_eq!(fs::read(target_root.path().join("tracked.txt")).unwrap(), b"unstaged");
    assert_eq!(
        git(target_root.path(), &["config", "--get", "remote.origin.url"]),
        b"https://example.invalid/repo.git\n"
    );
    assert!(!target_root.path().join(".env").exists());
    assert!(git(target_root.path(), &["config", "--local", "--list"]).windows(6).all(|bytes| bytes != b"secret"));
    let previous_source =
        source.status(CancellationToken::new()).await.unwrap()["state_hash"].as_str().unwrap().to_owned();
    fs::write(target_root.path().join("new.txt"), "windows staged").unwrap();
    git(target_root.path(), &["add", "new.txt"]);
    fs::write(target_root.path().join("new.txt"), "windows offline edit").unwrap();
    copy_worktree(&destination, &source);
    handoff(&destination, &source, Some(previous_source.clone())).await.unwrap();
    assert_eq!(
        git(target_root.path(), &["status", "--porcelain=v1", "--untracked-files=all"]),
        git(source_root.path(), &["status", "--porcelain=v1", "--untracked-files=all"])
    );
    git(source_root.path(), &["branch", "local-change"]);
    assert!(handoff(&destination, &source, Some(previous_source)).await.is_err());
    assert!(String::from_utf8(git(source_root.path(), &["branch", "--list"])).unwrap().contains("local-change"));

}

#[tokio::test]
async fn incomplete_corrupt_and_stale_git_import_never_replaces_repository( ) {

    let root = tempdir().unwrap();
    let files = repository(root.path());
    git(root.path(), &["init", "--quiet", "-b", "main"]);
    fs::write(root.path().join("data.rs"), "before").unwrap();
    git(root.path(), &["add", "data.rs"]);
    git(root.path(), &["commit", "--quiet", "-m", "fixture"]);
    let original = git(root.path(), &["rev-parse", "HEAD"]);
    let description = files
        .export(
            files.status(CancellationToken::new()).await.unwrap()["state_hash"].as_str().unwrap(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    let prepared = files
        .prepare_import(
            description["sha256"].as_str().unwrap(),
            description["bytes"].as_u64().unwrap() as usize,
            None,
            files.manifest().unwrap(),
        )
        .unwrap();
    assert!(files.apply_import(prepared["transfer_id"].as_str().unwrap(), CancellationToken::new()).await.is_err());
    assert_eq!(git(root.path(), &["rev-parse", "HEAD"]), original);
    assert!(files.put_import(prepared["transfer_id"].as_str().unwrap(), 0, b"corrupt", &digest(b"corrupt")).is_err());
    assert_eq!(git(root.path(), &["rev-parse", "HEAD"]), original);
    assert_eq!(
        public_origin("https://token:secret@github.com/owner/repo.git?auth=secret"),
        Some("https://github.com/owner/repo.git".into())
    );
    assert!(!valid_ref("refs/heads/../../escape"));

}
