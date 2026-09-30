/*! Git object bundle과 index 상태만 인계해. source의 config, hooks와 credential은 복사하지 않아. */

use crate::{
    execution::process::{ChildGuard, stop},
    filesystem::{FileStore, digest, hashes, validate_paths},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use process_wrap::tokio::CommandWrap;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Entry {

    mode: String,
    oid: String,
    path: String,

}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct State {

    head: Option<String>,
    branch: Option<String>,
    refs: BTreeMap<String, String>,
    index: Vec<Entry>,
    origin: Option<String>,

}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Archive {

    version: u32,
    state: State,
    bundle_base64: Option<String>,
    index_pack_base64: Option<String>,

}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Import {

    sha256: String,
    bytes: usize,
    expected_state: Option<String>,
    expected_manifest: String,

}

#[derive(Clone)]
pub(crate) struct Repository {

    files: FileStore,
    executable: PathBuf,

}

impl Repository {

    pub fn new( files: FileStore, executable: PathBuf, ) -> Self {

        Self { files, executable }

    }

    fn root( &self, ) -> Result<PathBuf,String> {

        Ok(self.files.metadata_dir().map_err(message)?.parent().ok_or("Git root가 없습니다")?.to_owned())

    }

    async fn command( &self, root: &Path, args: &[&str], input: Option<Vec<u8>>, cancellation: CancellationToken, ) -> Result<Vec<u8>,String> {

        let hooks = self.files.metadata_dir().map_err(message)?.join("no-hooks");
        if !hooks.exists() {

            fs::create_dir(&hooks).map_err(message)?;

        }
        if fs::symlink_metadata(&hooks)
            .map_err(message)
            .is_ok_and(|metadata| crate::filesystem::paths::redirected(&metadata))
        {

            return Err("Git hook 경로가 변경되었습니다".into());

        }
        let hooks = git_path(&hooks)?;
        let safe_directory = git_path(root)?;
        let mut command = CommandWrap::with_new(&self.executable, |command| {

            command
                .env_clear()
                .current_dir(root)
                .args([
                    "-c",
                    "core.fsmonitor=false",
                    "-c",
                    "core.autocrlf=false",
                    "-c",
                    "maintenance.auto=false",
                    "-c",
                    "gc.auto=0",
                    "-c",
                ])
                .arg(format!("core.hooksPath={}", hooks))
                .arg("-c")
                .arg(format!("safe.directory={safe_directory}"))
                .args(args)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", if cfg!(windows) { "NUL" } else { "/dev/null" })
                .env("GIT_TERMINAL_PROMPT", "0")
                .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            for key in ["PATH", "SystemRoot", "WINDIR", "TEMP", "TMP", "TMPDIR"] {

                if let Some(value) = std::env::var_os(key) {

                    command.env(key, value);

                }

            }

        });
        crate::platform::configure(&mut command);
        if cancellation.is_cancelled() {

            return Err("Git 작업이 취소되었습니다".into());

        }
        let child = command.spawn().map_err(message)?;
        let mut guard = ChildGuard { child, active: true };
        let stdout = guard.child.stdout().take().ok_or("Git stdout이 없습니다")?;
        let stderr = guard.child.stderr().take().ok_or("Git stderr가 없습니다")?;
        let stdin = guard.child.stdin().take();
        let complete = async {

            let write = async {

                if let (Some(mut stdin), Some(input)) = (stdin, input) {

                    stdin.write_all(&input).await?;
                    stdin.shutdown().await?;

                }
                Ok::<(), std::io::Error>(())

            };
            tokio::try_join!(guard.child.wait(), bounded(stdout, 64 * 1024 * 1024), bounded(stderr, 65536), write)

        };
        let outcome = tokio::select! {
            biased;
            _ = cancellation.cancelled() => Err("Git 작업이 취소되었습니다".to_owned()),
            result = tokio::time::timeout(Duration::from_secs(60),complete) => match result {
                Ok(Ok((status,out,_err,()))) if status.success() => Ok(out),
                Ok(Ok((status,_,err,()))) => {
                    let diagnostic = String::from_utf8_lossy(&err).split_whitespace().take(64)
                        .map(|word|if word.contains("://") || word.contains('@') {"[주소 생략]"}else{word}).collect::<Vec<_>>().join(" ");
                    Err(format!("Git {} 작업이 실패했습니다 ({:?}): {diagnostic}",args.first().copied().unwrap_or("내부"),status.code()))
                },
                Ok(Err(error)) => Err(message(error)),
                Err(_) => Err("Git 작업 제한 시간이 초과되었습니다".into()),
            },
        };
        if outcome.is_err() {

            stop(&mut *guard.child).await.map_err(message)?;

        }
        guard.active = false;
        outcome

    }

    async fn state( &self, cancellation: CancellationToken, ) -> Result<Option<State>,String> {

        self.recover()?;
        let root = self.root()?;
        let git = root.join(".git");
        match fs::symlink_metadata(&git) {

            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(message(error)),
            Ok(metadata) if crate::filesystem::paths::redirected(&metadata) => {

                return Err("Git metadata symlink는 거부합니다".into());

            }
            Ok(_) => {}

        }
        let top = self.command(&root, &["rev-parse", "--show-toplevel"], None, cancellation.clone()).await?;
        if PathBuf::from(text(top)?).canonicalize().map_err(message)? != root {

            return Err("등록된 project 자체의 Git repository만 인계합니다".into());

        }
        let directory = self.command(&root, &["rev-parse", "--absolute-git-dir"], None, cancellation.clone()).await?;
        let directory = PathBuf::from(text(directory)?);
        for marker in ["MERGE_HEAD", "CHERRY_PICK_HEAD", "REVERT_HEAD", "rebase-merge", "rebase-apply", "index.lock"] {

            if directory.join(marker).exists() {

                return Err("진행 중인 Git 작업이나 conflict를 먼저 완료하세요".into());

            }

        }
        let head = self
            .command(&root, &["rev-parse", "--verify", "HEAD"], None, cancellation.clone())
            .await
            .ok()
            .map(text)
            .transpose()?;
        let branch = self
            .command(&root, &["symbolic-ref", "-q", "HEAD"], None, cancellation.clone())
            .await
            .ok()
            .map(text)
            .transpose()?;
        let refs = self
            .command(&root, &["for-each-ref", "--format=%(refname) %(objectname)"], None, cancellation.clone())
            .await?;
        let mut references = BTreeMap::new();
        for line in text(refs)?.lines() {

            let (name, oid) = line.split_once(' ').ok_or("Git ref 형식이 잘못되었습니다")?;
            references.insert(name.into(), oid.into());

        }
        let index = self.command(&root, &["ls-files", "--stage", "-z"], None, cancellation.clone()).await?;
        let mut entries = Vec::new();
        for line in index.split(|byte| *byte == 0).filter(|line| !line.is_empty()) {

            let line = std::str::from_utf8(line).map_err(message)?;
            let (prefix, path) = line.split_once('\t').ok_or("Git index 형식이 잘못되었습니다")?;
            let parts: Vec<_> = prefix.split(' ').collect();
            if parts.len() != 3 || parts[2] != "0" {

                return Err("unmerged Git index는 인계하지 않습니다".into());

            }
            entries.push(Entry { mode: parts[0].into(), oid: parts[1].into(), path: path.into() });

        }
        let origin = self
            .command(&root, &["config", "--get", "remote.origin.url"], None, cancellation)
            .await
            .ok()
            .map(text)
            .transpose()?
            .and_then(|url| public_origin(&url));
        let state = State { head, branch, refs: references, index: entries, origin };
        self.validate(&state)?;
        Ok(Some(state))

    }

    fn validate( &self, state: &State, ) -> Result<(),String> {

        if state.refs.len() > 1024
            || state.index.len() > 10000
            || state.head.as_ref().is_some_and(|oid| !valid_oid(oid))
            || state.branch.as_ref().is_some_and(|branch| !valid_ref(branch) || !branch.starts_with("refs/heads/"))
            || state.refs.iter().any(|(name, oid)| !valid_ref(name) || !valid_oid(oid))
            || state.index.iter().any(|entry| {

                !matches!(entry.mode.as_str(), "100644" | "100755")
                    || !valid_oid(&entry.oid)
                    || !self.files.permits(&entry.path)

            })
            || state.origin.as_ref().is_some_and(|url| public_origin(url).as_ref() != Some(url))
        {

            return Err(
                "Git 상태가 project 정책을 벗어납니다. symlink, submodule과 제외된 tracked 파일은 인계하지 않습니다"
                    .into(),
            );

        }
        validate_paths(state.index.iter().map(|entry| entry.path.as_str())).map_err(message)?;
        Ok(())

    }

    pub async fn status( &self, cancellation: CancellationToken, ) -> Result<Value,String> {

        let _lock = self.files.lock("git.lock").map_err(message)?;
        let state = self.state(cancellation).await?;
        Ok(json!({"state_hash":state.as_ref().map(state_hash).transpose()?,"state":state,"recovery_pending":false}))

    }

    pub async fn export( &self, expected: &str, cancellation: CancellationToken, ) -> Result<Value,String> {

        let _lock = self.files.lock("git.lock").map_err(message)?;
        let state = self.state(cancellation.clone()).await?.ok_or("Git repository가 없습니다")?;
        if state_hash(&state)? != expected {

            return Err("Git export 직전에 상태가 변경되었습니다".into());

        }
        let root = self.root()?;
        let temporary = tempfile::Builder::new()
            .prefix("git-export-")
            .tempdir_in(self.files.metadata_dir().map_err(message)?)
            .map_err(message)?;
        let bundle = temporary.path().join("repository.bundle");
        let bundle_base64 = if state.head.is_some() || !state.refs.is_empty() {

            let bundle_path = git_path(&bundle)?;
            self.command(&root, &["bundle", "create", &bundle_path, "--all", "HEAD"], None, cancellation.clone())
                .await?;
            let bytes = fs::read(bundle).map_err(message)?;
            if bytes.len() > 48 * 1024 * 1024 {

                return Err("Git bundle 한도는 48 MiB입니다".into());

            }
            Some(STANDARD.encode(bytes))

        } else {

            None

        };
        let ids: BTreeSet<_> = state.index.iter().map(|entry| entry.oid.as_str()).collect();
        let index_pack_base64 =
            if ids.is_empty() {

                None

            } else {

                let input = ids.into_iter().map(|id| format!("{id}\n")).collect::<String>().into_bytes();
                Some(STANDARD.encode(
                    self.command(&root, &["pack-objects", "--stdout"], Some(input), cancellation.clone()).await?,
                ))

            };
        if self.state(cancellation).await?.as_ref() != Some(&state) {

            return Err("Git export 중 상태가 변경되었습니다".into());

        }
        let payload =
            serde_json::to_vec(&Archive { version: 1, state, bundle_base64, index_pack_base64 }).map_err(message)?;
        let sha256 = digest(&payload);
        self.files.save_metadata("git-export.bin", &payload).map_err(message)?;
        Ok(json!({"sha256":sha256,"bytes":payload.len(),"chunks":crate::filesystem::chunks::describe(&payload)}))

    }

    pub fn read_export( &self, expected: &str, index: usize, ) -> Result<Value,String> {

        let payload = self.files.load_metadata("git-export.bin").map_err(message)?.ok_or("Git export가 없습니다")?;
        if digest(&payload) != expected {

            return Err("Git export revision이 변경되었습니다".into());

        }
        let bytes = payload.chunks(65536).nth(index).ok_or("Git chunk index가 잘못되었습니다")?;
        Ok(json!({"content_base64":STANDARD.encode(bytes),"sha256":digest(bytes)}))

    }

    pub fn prepare_import( &self, sha256: &str, bytes: usize, expected_state: Option<String>, expected_manifest: String, ) -> Result<Value,String> {

        let _lock = self.files.lock("git.lock").map_err(message)?;
        if !valid_hash(sha256)
            || !(1..=64 * 1024 * 1024).contains(&bytes)
            || expected_state.as_ref().is_some_and(|hash| !valid_hash(hash))
            || !valid_hash(&expected_manifest)
        {

            return Err("Git import hash와 한도를 확인하세요".into());

        }
        let import = Import { sha256: sha256.into(), bytes, expected_state, expected_manifest };
        let id = digest(&serde_json::to_vec(&import).map_err(message)?);
        if let Some(previous) = self.files.load_metadata("git-import.json").map_err(message)? {

            let previous: Import = serde_json::from_slice(&previous).map_err(message)?;
            if previous.sha256 != sha256 {

                for index in 0..previous.bytes.div_ceil(65536) {

                    let path = self.files.metadata_dir().map_err(message)?.join(format!("git-import-{index}.bin"));
                    if path.exists() {

                        fs::remove_file(path).map_err(message)?;

                    }

                }

            }

        }
        self.files.save_metadata("git-import.json", &serde_json::to_vec(&import).map_err(message)?).map_err(message)?;
        let missing = (0..bytes.div_ceil(65536))
            .filter(|index| self.files.load_metadata(&format!("git-import-{index}.bin")).ok().flatten().is_none())
            .collect::<Vec<_>>();
        Ok(json!({"transfer_id":id,"missing":missing}))

    }

    fn import( &self, id: &str, ) -> Result<Import,String> {

        let bytes = self.files.load_metadata("git-import.json").map_err(message)?.ok_or("Git import가 없습니다")?;
        if digest(&bytes) != id {

            return Err("Git import handle이 변경되었습니다".into());

        }
        serde_json::from_slice(&bytes).map_err(message)

    }

    pub fn put_import( &self, id: &str, index: usize, bytes: &[u8], expected: &str, ) -> Result<Value,String> {

        let _lock = self.files.lock("git.lock").map_err(message)?;
        let import = self.import(id)?;
        if index >= import.bytes.div_ceil(65536)
            || bytes.len() != (import.bytes - index * 65536).min(65536)
            || digest(bytes) != expected
        {

            return Err("Git chunk 크기 또는 hash가 다릅니다".into());

        }
        self.files.save_metadata(&format!("git-import-{index}.bin"), bytes).map_err(message)?;
        Ok(json!({"accepted":true}))

    }

    pub async fn apply_import( &self, id: &str, cancellation: CancellationToken, ) -> Result<Value,String> {

        let _lock = self.files.lock("git.lock").map_err(message)?;
        let import = self.import(id)?;
        let mut payload = Vec::with_capacity(import.bytes);
        for index in 0..import.bytes.div_ceil(65536) {

            payload.extend(
                self.files
                    .load_metadata(&format!("git-import-{index}.bin"))
                    .map_err(message)?
                    .ok_or("Git chunk가 아직 없습니다")?,
            );

        }
        if payload.len() != import.bytes || digest(&payload) != import.sha256 {

            return Err("Git archive hash가 다릅니다".into());

        }
        let archive: Archive = serde_json::from_slice(&payload).map_err(message)?;
        if archive.version != 1 {

            return Err("지원하지 않는 Git archive version입니다".into());

        }
        self.validate(&archive.state)?;
        let old = self.state(cancellation.clone()).await?;
        let next_hash = state_hash(&archive.state)?;
        if old.as_ref().map(state_hash).transpose()?.as_deref() == Some(&next_hash) {

            return Ok(json!({"state_hash":next_hash,"already_applied":true}));

        }
        if old.as_ref().map(state_hash).transpose()? != import.expected_state {

            return Err("대상의 새 Git 변경을 덮어쓰지 않습니다: Git conflict".into());

        }
        if self.manifest()? != import.expected_manifest {

            return Err("Git import 직전에 source가 변경되었습니다".into());

        }
        let root = self.root()?;
        let existing = root.join(".git");
        if existing.is_file() {

            return Err("기존 linked worktree metadata 교체는 지원하지 않습니다. 원본 worktree는 유지합니다".into());

        }
        let staging = tempfile::Builder::new()
            .prefix("git-stage-")
            .tempdir_in(self.files.metadata_dir().map_err(message)?)
            .map_err(message)?;
        if existing.exists() {

            copy_metadata(&existing, &staging.path().join(".git"), &mut 0)?;

        } else {

            self.command(staging.path(), &["init", "--quiet"], None, cancellation.clone()).await?;

        }
        if let Some(encoded) = archive.bundle_base64 {

            let bundle = staging.path().join("repository.bundle");
            fs::write(&bundle, STANDARD.decode(encoded).map_err(message)?).map_err(message)?;
            let path = git_path(&bundle)?;
            self.command(staging.path(), &["bundle", "verify", &path], None, cancellation.clone()).await?;
            self.command(staging.path(), &["bundle", "unbundle", &path], None, cancellation.clone()).await?;

        }
        if let Some(encoded) = archive.index_pack_base64 {

            let bytes = STANDARD.decode(encoded).map_err(message)?;
            self.command(staging.path(), &["index-pack", "--stdin"], Some(bytes), cancellation.clone()).await?;

        }
        let mut update = String::from("start\n");
        for (name, oid) in &archive.state.refs {

            update.push_str(&format!("update {name} {oid}\n"));

        }
        for name in old.as_ref().into_iter().flat_map(|state| state.refs.keys()) {

            if !archive.state.refs.contains_key(name) {

                update.push_str(&format!("delete {name}\n"));

            }

        }
        update.push_str("prepare\ncommit\n");
        self.command(staging.path(), &["update-ref", "--stdin"], Some(update.into_bytes()), cancellation.clone())
            .await?;
        if let Some(branch) = &archive.state.branch {

            self.command(staging.path(), &["symbolic-ref", "HEAD", branch], None, cancellation.clone()).await?;

        } else if let Some(head) = &archive.state.head {

            self.command(staging.path(), &["update-ref", "--no-deref", "HEAD", head], None, cancellation.clone())
                .await?;

        }
        self.command(staging.path(), &["read-tree", "--empty"], None, cancellation.clone()).await?;
        if !archive.state.index.is_empty() {

            let index = archive
                .state
                .index
                .iter()
                .map(|entry| format!("{} {}\t{}\0", entry.mode, entry.oid, entry.path))
                .collect::<String>()
                .into_bytes();
            self.command(staging.path(), &["update-index", "-z", "--index-info"], Some(index), cancellation.clone())
                .await?;

        }
        if let Some(origin) = &archive.state.origin {

            self.command(staging.path(), &["config", "remote.origin.url", origin], None, cancellation.clone()).await?;

        } else {

            let _ = self
                .command(staging.path(), &["config", "--unset-all", "remote.origin.url"], None, cancellation.clone())
                .await;

        }
        self.command(staging.path(), &["fsck", "--connectivity-only", "--no-reflogs"], None, cancellation.clone())
            .await?;
        if self.state(cancellation.clone()).await? != old || self.manifest()? != import.expected_manifest {

            return Err("Git 준비 중 대상이 변경되었습니다. 인계를 중단합니다".into());

        }
        if cancellation.is_cancelled() {

            return Err("Git import가 취소되었습니다".into());

        }
        let backup_name = format!("git-backup-{id}");
        let backup = self.files.metadata_dir().map_err(message)?.join(&backup_name);
        if backup.exists() {

            return Err("기존 Git recovery backup을 보존합니다".into());

        }
        self.files
            .save_metadata(
                "git-apply.json",
                &serde_json::to_vec(&json!({"backup":backup_name,"had_git":old.is_some(),"state_hash":next_hash}))
                    .map_err(message)?,
            )
            .map_err(message)?;
        if existing.exists() {

            fs::rename(&existing, &backup).map_err(message)?;

        }
        if let Err(error) = fs::rename(staging.path().join(".git"), &existing) {

            if backup.exists() {

                fs::rename(&backup, &existing).map_err(message)?;

            }
            self.clear_apply()?;
            return Err(message(error));

        }
        self.clear_apply()?;
        let applied = self.state(cancellation).await?.ok_or("Git import 결과가 없습니다")?;
        if state_hash(&applied)? != next_hash {

            return Err("Git import 결과가 예상 상태와 다릅니다. backup을 보존합니다".into());

        }
        Ok(
            json!({"state_hash":next_hash,"already_applied":false,"backup":if old.is_some(){Some(backup_name)}else{None}}),
        )

    }

    fn manifest( &self, ) -> Result<String,String> {

        Ok(digest(&serde_json::to_vec(&hashes(&self.files.list().map_err(message)?)).map_err(message)?))

    }

    fn clear_apply( &self, ) -> Result<(),String> {

        fs::remove_file(self.files.metadata_dir().map_err(message)?.join("git-apply.json")).map_err(message)

    }

    fn recover( &self, ) -> Result<(),String> {

        if let Some(bytes) = self.files.load_metadata("git-apply.json").map_err(message)? {

            let journal: Value = serde_json::from_slice(&bytes).map_err(message)?;
            let backup_name = journal["backup"].as_str().ok_or("Git recovery journal이 잘못되었습니다")?;
            let suffix = backup_name
                .strip_prefix("git-backup-")
                .filter(|id| valid_hash(id))
                .ok_or("Git recovery 경로가 잘못되었습니다")?;
            let backup = self.files.metadata_dir().map_err(message)?.join(format!("git-backup-{suffix}"));
            let existing = self.root()?.join(".git");
            if !existing.exists() && backup.exists() {

                fs::rename(backup, existing).map_err(message)?;

            } else if !existing.exists() && journal["had_git"] == true {

                return Err("Git recovery backup이 없습니다. 수동 확인이 필요합니다".into());

            }
            self.clear_apply()?;

        }
        Ok(())

    }

}

async fn bounded( input: impl AsyncRead + Unpin, limit: usize, ) -> Result<Vec<u8>,std::io::Error> {

    let mut bytes = Vec::new();
    input.take(limit as u64 + 1).read_to_end(&mut bytes).await?;
    if bytes.len() > limit {

        return Err(std::io::Error::other("Git output 한도를 초과했습니다"));

    }
    Ok(bytes)

}

fn text( bytes: Vec<u8>, ) -> Result<String,String> {

    String::from_utf8(bytes).map(|text| text.trim_end_matches(['\r', '\n']).to_owned()).map_err(message)

}

fn state_hash( state: &State, ) -> Result<String,String> {

    Ok(digest(&serde_json::to_vec(state).map_err(message)?))

}

fn valid_hash( hash: &str, ) -> bool {

    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))

}

fn valid_oid( oid: &str, ) -> bool {

    matches!(oid.len(), 40 | 64) && oid.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))

}

fn valid_ref( name: &str, ) -> bool {

    name.starts_with("refs/")
        && name.len() <= 1024
        && !name.contains("..")
        && !name.contains("@{")
        && !name.ends_with(['.', '/'])
        && !name.ends_with(".lock")
        && !name.split('/').any(|part| part.is_empty() || part.starts_with('.'))
        && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"/-_.".contains(&byte))

}

fn public_origin( url: &str, ) -> Option<String> {

    if url.len() > 2048 || url.contains(['\0', '\n', '\r']) {

        return None;

    }
    if let Some((scheme, rest)) = url.split_once("://") {

        if !matches!(scheme, "https" | "ssh") {

            return None;

        }
        let (authority, path) = rest.split_once('/')?;
        let host = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
        if host.is_empty() || !host.bytes().all(|byte| byte.is_ascii_alphanumeric() || b".-:[]".contains(&byte)) {

            return None;

        }
        let path = path.split(['?', '#']).next()?;
        Some(format!("{scheme}://{}{}{}", if scheme == "ssh" { "git@" } else { "" }, host, format_args!("/{path}")))

    } else if url.starts_with("git@") && url.split_once(':').is_some() && !url.contains(['?', '#', ' ']) {

        Some(url.into())

    } else {

        None

    }

}

fn git_path( path: &Path, ) -> Result<String,String> {

    let path = path.to_str().ok_or("UTF-8 Git 경로가 필요합니다")?;
    #[cfg(windows)]
    {

        if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {

            return Ok(format!("//{}", unc.replace('\\', "/")));

        }
        Ok(path.strip_prefix(r"\\?\").unwrap_or(path).replace('\\', "/"))

    }
    #[cfg(not(windows))]
    Ok(path.into())

}

fn copy_metadata( source: &Path, destination: &Path, total: &mut u64, ) -> Result<(),String> {

    let mut pending = vec![(source.to_owned(), destination.to_owned(), 0usize)];
    let mut entries = 0usize;
    while let Some((source, destination, depth)) = pending.pop() {

        if depth > 32 {

            return Err("Git metadata 깊이 한도는 32입니다".into());

        }
        fs::create_dir(&destination).map_err(message)?;
        for entry in fs::read_dir(source).map_err(message)? {

            entries += 1;
            if entries > 10000 {

                return Err("Git recovery entry 한도는 10000입니다".into());

            }
            let entry = entry.map_err(message)?;
            let metadata = fs::symlink_metadata(entry.path()).map_err(message)?;
            if crate::filesystem::paths::redirected(&metadata) {

                return Err("대상 Git metadata의 symlink를 덮어쓰지 않습니다".into());

            }
            let path = destination.join(entry.file_name());
            if metadata.is_dir() {

                pending.push((entry.path(), path, depth + 1));

            } else if metadata.is_file() {

                *total = total.checked_add(metadata.len()).ok_or("Git recovery 크기가 잘못되었습니다")?;
                if *total > 256 * 1024 * 1024 {

                    return Err("로컬 Git recovery 복사 한도는 256 MiB입니다".into());

                }
                fs::copy(entry.path(), path).map_err(message)?;

            } else {

                return Err("Git special file은 거부합니다".into());

            }

        }

    }
    Ok(())

}

fn message( error: impl std::fmt::Display, ) -> String {

    format!("Git 인계 오류: {error}")

}

#[cfg(test)]
#[path = "../../tests/unit/git.rs"]
mod tests;
