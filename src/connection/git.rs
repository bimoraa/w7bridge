/*! file sync 뒤에 Git baseline을 비교하고 한쪽 변경만 안전하게 인계해. */

use super::session::Remote;
use crate::{filesystem::FileStore, git::Repository, sync::SyncError};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Baseline {

    established: bool,
    common: Option<String>,
    pending: Option<Pending>,

}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Pending {

    remote_target: bool,
    expected: Option<String>,
    intended: String,

}

impl Remote {

    pub(super) async fn git_handoff( &self, files: &FileStore, ) -> Result<bool,SyncError> {

        let executable = self.git_executable.as_ref().ok_or(SyncError::State)?.clone();
        let repository = Repository::new(files.clone(), executable);
        let cancellation = tokio_util::sync::CancellationToken::new();
        let left = repository.status(cancellation.clone()).await.map_err(SyncError::Git)?;
        let right = self.call("git_status", json!({})).await?;
        let left_hash = optional_hash(&left)?;
        let right_hash = optional_hash(&right)?;
        let mut baseline: Baseline = match files.load_metadata(&self.git_baseline_name)? {

            Some(bytes) => serde_json::from_slice(&bytes).map_err(|_| SyncError::State)?,
            None => Baseline::default(),

        };
        if let Some(pending) = baseline.pending.take() {

            let current = if pending.remote_target { &right_hash } else { &left_hash };
            if current.as_deref() == Some(&pending.intended) {

                baseline.established = true;
                baseline.common = Some(pending.intended);

            } else if current != &pending.expected {

                baseline.pending = Some(pending);
                return Ok(false);

            }
            files.save_metadata(&self.git_baseline_name, &serde_json::to_vec(&baseline)?)?;

        }
        if left_hash == right_hash {

            baseline.established = true;
            baseline.common = left_hash;
            files.save_metadata(&self.git_baseline_name, &serde_json::to_vec(&baseline)?)?;
            return Ok(true);

        }
        let remote_target = if !baseline.established {

            match (&left_hash, &right_hash) {

                (Some(_), None) => true,
                (None, Some(_)) => false,
                _ => return Ok(false),

            }

        } else if left_hash != baseline.common && right_hash == baseline.common {

            true

        } else if right_hash != baseline.common && left_hash == baseline.common {

            false

        } else {

            return Ok(false);

        };
        let (source, expected) = if remote_target { (&left_hash, &right_hash) } else { (&right_hash, &left_hash) };
        let Some(source) = source.as_ref() else { return Ok(false) };
        let description = if remote_target {

            repository.export(source, cancellation.clone()).await.map_err(SyncError::Git)?

        } else {

            self.call("git_export", json!({"expected_state":source})).await?

        };
        let sha256 = description["sha256"].as_str().ok_or(SyncError::Peer)?;
        let bytes = description["bytes"]
            .as_u64()
            .filter(|bytes| *bytes > 0 && *bytes <= 64 * 1024 * 1024)
            .ok_or(SyncError::Peer)? as usize;
        let manifest = crate::sync::state::coordination::manifest(files)?;
        let prepared = if remote_target {

            self.call(
                "prepare_git_import",
                json!({"sha256":sha256,"bytes":bytes,"expected_state":expected,"expected_manifest":manifest}),
            )
            .await?

        } else {

            repository.prepare_import(sha256, bytes, expected.clone(), manifest).map_err(SyncError::Git)?

        };
        let id = prepared["transfer_id"].as_str().ok_or(SyncError::Peer)?;
        let chunks: Vec<crate::filesystem::chunks::Chunk> =
            serde_json::from_value(description["chunks"].clone()).map_err(|_| SyncError::Peer)?;
        if chunks.len() != bytes.div_ceil(65536) {

            return Err(SyncError::Peer);

        }
        let missing = prepared["missing"].as_array().ok_or(SyncError::Peer)?;
        for index in missing {

            let index = index.as_u64().filter(|index| *index < chunks.len() as u64).ok_or(SyncError::Peer)? as usize;
            self.throttle(chunks[index].bytes).await?;
            let result = if remote_target {

                repository.read_export(sha256, index).map_err(SyncError::Git)?

            } else {

                self.call("read_git_chunk", json!({"sha256":sha256,"index":index})).await?

            };
            let encoded = result["content_base64"].as_str().ok_or(SyncError::Peer)?;
            if encoded.len() > 87384 {

                return Err(SyncError::Peer);

            }
            let content = STANDARD.decode(encoded).map_err(|_| SyncError::Peer)?;
            if content.len() != chunks[index].bytes || crate::filesystem::digest(&content) != chunks[index].sha256 {

                return Err(SyncError::Peer);

            }
            if remote_target {

                self.call(
                    "put_git_chunk",
                    json!({"transfer_id":id,"index":index,"sha256":chunks[index].sha256,"content_base64":encoded}),
                )
                .await?;

            } else {

                repository.put_import(id, index, &content, &chunks[index].sha256).map_err(SyncError::Git)?;

            }

        }
        baseline.pending = Some(Pending { remote_target, expected: expected.clone(), intended: source.clone() });
        files.save_metadata(&self.git_baseline_name, &serde_json::to_vec(&baseline)?)?;
        let applied = if remote_target {

            self.call("import_git", json!({"transfer_id":id})).await?

        } else {

            repository.apply_import(id, cancellation).await.map_err(SyncError::Git)?

        };
        if applied["state_hash"].as_str() != Some(source) {

            return Err(SyncError::State);

        }
        baseline.pending = None;
        baseline.established = true;
        baseline.common = Some(source.clone());
        files.save_metadata(&self.git_baseline_name, &serde_json::to_vec(&baseline)?)?;
        Ok(true)

    }

}

fn optional_hash( status: &Value, ) -> Result<Option<String>,SyncError> {

    match status["state_hash"].as_str() {

        Some(hash)
            if hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) =>
        {

            Ok(Some(hash.into()))

        }
        None if status["state_hash"].is_null() => Ok(None),
        _ => Err(SyncError::Peer),

    }

}
