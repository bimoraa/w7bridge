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

async fn incremental_handoff( source: &Repository, destination: &Repository, ) -> Result<(Value,Value),String> {

    let token = CancellationToken::new();
    let old = destination.status(token.clone()).await?;
    let state = source.status(token.clone()).await?;
    let heads: Vec<String> = old["state"]["head"].as_str().into_iter().map(str::to_owned).collect();
    let index: Vec<String> = old["state"]["index"]
        .as_array()
        .into_iter()
        .flat_map(|entries| entries.iter())
        .filter_map(|entry| entry["oid"].as_str().map(str::to_owned))
        .collect();
    let description =
        source.export_incremental(state["state_hash"].as_str().unwrap(), &heads, &index, token.clone()).await?;
    let prepared = destination.prepare_import(
        description["sha256"].as_str().unwrap(),
        description["bytes"].as_u64().unwrap() as usize,
        old["state_hash"].as_str().map(str::to_owned),
        destination.manifest()?,
    )?;
    for value in prepared["missing"].as_array().unwrap() {

        let index = value.as_u64().unwrap() as usize;
        let chunk = source.read_export(description["sha256"].as_str().unwrap(), index)?;
        destination.put_import(
            prepared["transfer_id"].as_str().unwrap(),
            index,
            &STANDARD.decode(chunk["content_base64"].as_str().unwrap()).unwrap(),
            chunk["sha256"].as_str().unwrap(),
        )?;

    }
    Ok((description, destination.apply_import(prepared["transfer_id"].as_str().unwrap(), token).await?))

}

#[tokio::test]
async fn incremental_branch_and_commit_handoff_reuses_history_and_preserves_local_index_flags( ) {

    let root = tempdir().unwrap();
    let peer = tempdir().unwrap();
    let source = repository(root.path());
    let destination = repository(peer.path());
    git(root.path(), &["init", "--quiet", "-b", "main"]);
    fs::write(root.path().join("source.rs"), "initial").unwrap();
    git(root.path(), &["add", "source.rs"]);
    git(root.path(), &["commit", "--quiet", "-m", "fixture"]);
    copy_worktree(&source, &destination);
    handoff(&source, &destination, None).await.unwrap();
    git(peer.path(), &["update-index", "--skip-worktree", "source.rs"]);
    git(peer.path(), &["update-index", "--assume-unchanged", "source.rs"]);
    git(root.path(), &["switch", "-c", "codex/mac"]);
    let (description, result) = incremental_handoff(&source, &destination).await.unwrap();
    assert!(description["bytes"].as_u64().unwrap() < 4096);
    let archive: Archive = serde_json::from_slice(
        &source
            .files
            .load_metadata(&format!("git-export-{}.bin", description["sha256"].as_str().unwrap()))
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert!(archive.bundle_base64.is_none());
    assert!(archive.index_pack_base64.is_none());
    assert!(result["backup"].is_string());
    assert_eq!(git(peer.path(), &["branch", "--show-current"]), b"codex/mac\n");
    assert_eq!(git(peer.path(), &["ls-files", "-v"]), b"s source.rs\n");
    fs::write(root.path().join("new.rs"), "new commit").unwrap();
    git(root.path(), &["add", "new.rs"]);
    git(root.path(), &["commit", "--quiet", "-m", "second"]);
    fs::write(root.path().join("staged.rs"), "staged only").unwrap();
    git(root.path(), &["add", "staged.rs"]);
    copy_worktree(&source, &destination);
    incremental_handoff(&source, &destination).await.unwrap();
    assert_eq!(git(peer.path(), &["rev-parse", "HEAD"]), git(root.path(), &["rev-parse", "HEAD"]));
    assert_eq!(git(peer.path(), &["show", ":staged.rs"]), b"staged only");
    assert_eq!(git(peer.path(), &["ls-files", "-v", "source.rs"]), b"s source.rs\n");
    let object = text(git(root.path(), &["hash-object", "-w", "--stdin"])).unwrap();
    git(root.path(), &["update-ref", "refs/snapshots/blob", &object]);
    incremental_handoff(&source, &destination).await.unwrap();
    assert_eq!(git(peer.path(), &["cat-file", "-t", "refs/snapshots/blob"]), b"blob\n");
    let description = source
        .export(
            source.status(CancellationToken::new()).await.unwrap()["state_hash"].as_str().unwrap(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    let path = source
        .files
        .metadata_dir()
        .unwrap()
        .join(format!("git-export-{}.bin", description["sha256"].as_str().unwrap()));
    let mut bytes = fs::read(&path).unwrap();
    bytes[0] ^= 1;
    fs::write(path, bytes).unwrap();
    assert!(source.read_export(description["sha256"].as_str().unwrap(), 0).is_err());

}

#[tokio::test]
async fn explicit_local_only_index_paths_preserve_working_files_and_reject_secrets( ) {

    let root = tempdir().unwrap();
    let peer = tempdir().unwrap();
    let source = repository(root.path());
    let destination = repository(peer.path());
    git(root.path(), &["init", "--quiet", "-b", "main"]);
    fs::create_dir(root.path().join("build")).unwrap();
    fs::write(root.path().join("build/output.txt"), "old tracked artifact").unwrap();
    git(root.path(), &["add", "build/output.txt"]);
    git(root.path(), &["commit", "--quiet", "-m", "fixture"]);
    assert!(source.status(CancellationToken::new()).await.is_err());
    assert!(source.clone().with_local_only_paths(vec![".env".into()]).is_err());
    assert!(source.clone().with_local_only_paths(vec![".git/config".into()]).is_err());
    assert!(source.clone().with_local_only_paths(vec!["../escape".into()]).is_err());
    assert!(source.clone().with_local_only_paths(vec!["source.rs".into()]).is_err());
    let source = source.with_local_only_paths(vec!["build/output.txt".into()]).unwrap();
    let destination = destination.with_local_only_paths(vec!["build/output.txt".into()]).unwrap();
    fs::create_dir(peer.path().join("build")).unwrap();
    fs::write(peer.path().join("build/output.txt"), "keep Windows artifact").unwrap();
    handoff(&source, &destination, None).await.unwrap();
    assert_eq!(git(peer.path(), &["show", ":build/output.txt"]), b"old tracked artifact");
    assert_eq!(fs::read(peer.path().join("build/output.txt")).unwrap(), b"keep Windows artifact");
    assert!(destination.files.list().unwrap().is_empty());

}

#[tokio::test]
async fn git_handoff_never_replaces_metadata_shared_with_another_worktree( ) {

    let root = tempdir().unwrap();
    let peer = tempdir().unwrap();
    let extra = tempdir().unwrap();
    let source = repository(root.path());
    let destination = repository(peer.path());
    git(root.path(), &["init", "--quiet", "-b", "main"]);
    fs::write(root.path().join("source.rs"), "initial").unwrap();
    git(root.path(), &["add", "source.rs"]);
    git(root.path(), &["commit", "--quiet", "-m", "fixture"]);
    copy_worktree(&source, &destination);
    handoff(&source, &destination, None).await.unwrap();
    let path = git_path(&extra.path().join("linked")).unwrap();
    git(peer.path(), &["worktree", "add", "--quiet", "--detach", &path]);
    let before = git(peer.path(), &["rev-parse", "HEAD"]);
    let expected =
        destination.status(CancellationToken::new()).await.unwrap()["state_hash"].as_str().unwrap().to_owned();
    git(root.path(), &["switch", "-c", "codex/mac"]);
    let error = handoff(&source, &destination, Some(expected)).await.unwrap_err();
    assert!(error.contains("worktree"), "{error}");
    assert_eq!(git(peer.path(), &["rev-parse", "HEAD"]), before);
    assert_eq!(git(peer.path(), &["branch", "--show-current"]), b"main\n");
    assert_eq!(git(extra.path().join("linked").as_path(), &["rev-parse", "HEAD"]), before);

}

#[tokio::test]
async fn git_exports_are_peer_independent_and_missing_known_objects_never_apply( ) {

    let root = tempdir().unwrap();
    let peer = tempdir().unwrap();
    let source = repository(root.path());
    let destination = repository(peer.path());
    git(root.path(), &["init", "--quiet", "-b", "main"]);
    fs::write(root.path().join("source.rs"), "initial").unwrap();
    git(root.path(), &["add", "source.rs"]);
    git(root.path(), &["commit", "--quiet", "-m", "fixture"]);
    copy_worktree(&source, &destination);
    handoff(&source, &destination, None).await.unwrap();
    let token = CancellationToken::new();
    let old = destination.status(token.clone()).await.unwrap();
    fs::write(root.path().join("staged.rs"), "new blob").unwrap();
    git(root.path(), &["add", "staged.rs"]);
    copy_worktree(&source, &destination);
    let state = source.status(token.clone()).await.unwrap();
    let expected = state["state_hash"].as_str().unwrap();
    let full = source.export(expected, token.clone()).await.unwrap();
    let heads = vec![old["state"]["head"].as_str().unwrap().to_owned()];
    let false_inventory = state["state"]["index"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["oid"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    let incomplete = source.export_incremental(expected, &heads, &false_inventory, token.clone()).await.unwrap();
    assert_ne!(full["sha256"], incomplete["sha256"]);
    assert!(source.read_export(full["sha256"].as_str().unwrap(), 0).is_ok());
    let sha = incomplete["sha256"].as_str().unwrap();
    let prepared = destination
        .prepare_import(
            sha,
            incomplete["bytes"].as_u64().unwrap() as usize,
            old["state_hash"].as_str().map(str::to_owned),
            destination.manifest().unwrap(),
        )
        .unwrap();
    for value in prepared["missing"].as_array().unwrap() {

        let index = value.as_u64().unwrap() as usize;
        let chunk = source.read_export(sha, index).unwrap();
        destination
            .put_import(
                prepared["transfer_id"].as_str().unwrap(),
                index,
                &STANDARD.decode(chunk["content_base64"].as_str().unwrap()).unwrap(),
                chunk["sha256"].as_str().unwrap(),
            )
            .unwrap();

    }
    assert!(destination.apply_import(prepared["transfer_id"].as_str().unwrap(), token.clone()).await.is_err());
    assert_eq!(destination.status(token.clone()).await.unwrap()["state_hash"], old["state_hash"]);
    for i in 0..5 {

        git(root.path(), &["branch", &format!("cache-{i}")]);
        let state = source.status(token.clone()).await.unwrap();
        source
            .export_incremental(state["state_hash"].as_str().unwrap(), &heads, &false_inventory, token.clone())
            .await
            .unwrap();

    }
    let count = fs::read_dir(source.files.metadata_dir().unwrap())
        .unwrap()
        .filter(|entry| {

            entry.as_ref().unwrap().file_name().to_string_lossy().starts_with("git-export-")
                && entry.as_ref().unwrap().file_name().to_string_lossy().ends_with(".bin")

        })
        .count();
    assert_eq!(count, 4);

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
