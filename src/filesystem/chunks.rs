/*! 조건부 chunk 전송, delta 재사용과 durable staging을 소유해. */

use super::{FileStore, digest};
use crate::FileError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Seek, SeekFrom},
};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Chunk {

    pub sha256: String,
    pub bytes: usize,

}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Transfer {

    path: String,
    expected: Option<String>,
    sha256: String,
    chunks: Vec<Chunk>,

}

fn hash( value: &str, ) -> bool {

    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))

}

pub(crate) fn describe( content: &[u8], ) -> Vec<Chunk> {

    content.chunks(65536).map(|bytes| Chunk { sha256: digest(bytes), bytes: bytes.len() }).collect()

}

impl Transfer {

    fn validate( &self, files: &FileStore, ) -> Result<(), FileError> {

        if !files.permits(&self.path)
            || !hash(&self.sha256)
            || self.expected.as_ref().is_some_and(|expected| !hash(expected))
            || self.chunks.len() > 1024
            || self.chunks.iter().enumerate().any(|(index, chunk)| {

                !hash(&chunk.sha256)
                    || chunk.bytes == 0
                    || chunk.bytes > 65536
                    || index + 1 < self.chunks.len() && chunk.bytes != 65536

            })
            || self.chunks.iter().map(|chunk| chunk.bytes).sum::<usize>() > files.settings().max_file_bytes
        {

            return Err(FileError::Data);

        }
        Ok(())

    }

}

impl FileStore {

    pub(crate) fn chunk_manifest( &self, path: &str, ) -> Result<Value, FileError> {

        let bytes = self.read(path)?;
        Ok(json!({"sha256":digest(&bytes),"bytes":bytes.len(),"chunks":describe(&bytes)}))

    }

    pub(crate) fn read_chunk( &self, path: &str, index: usize, expected: &str, ) -> Result<Vec<u8>, FileError> {

        if !hash(expected) || index >= 1024 {

            return Err(FileError::Data);

        }
        let _lock = self.lock("access.lock")?;
        let resolved = self.resolve(path, false)?;
        if !fs::symlink_metadata(&resolved)?.is_file() {

            return Err(FileError::Path);

        }
        let mut file = fs::File::open(resolved)?;
        let length = file.metadata()?.len();
        if length > self.settings().max_file_bytes as u64 {

            return Err(FileError::Limit);

        }
        let offset = index * 65536;
        if offset as u64 >= length {

            return Err(FileError::Data);

        }
        file.seek(SeekFrom::Start(offset as u64))?;
        let mut bytes = Vec::with_capacity(65536);
        file.take(65536).read_to_end(&mut bytes)?;
        if digest(&bytes) != expected {

            return Err(FileError::Conflict);

        }
        Ok(bytes)

    }

    fn transfer( &self, id: &str, ) -> Result<Transfer, FileError> {

        if !hash(id) {

            return Err(FileError::Data);

        }
        let bytes = self.load_metadata(&format!("transfer-{id}.json"))?.ok_or(FileError::Data)?;
        let transfer: Transfer = serde_json::from_slice(&bytes).map_err(|_| FileError::Data)?;
        transfer.validate(self)?;
        if digest(&serde_json::to_vec(&transfer).map_err(|_| FileError::Data)?) != id {

            return Err(FileError::Data);

        }
        Ok(transfer)

    }

    fn staged( &self, id: &str, index: usize, chunk: &Chunk, ) -> Result<Option<Vec<u8>>, FileError> {

        let bytes = self.load_metadata(&format!("transfer-{id}-{index}.bin"))?;
        Ok(bytes.filter(|bytes| bytes.len() == chunk.bytes && digest(bytes) == chunk.sha256))

    }

    fn clear_transfer( &self, id: &str, transfer: &Transfer, ) -> Result<(), FileError> {

        let directory = self.metadata_dir()?;
        for name in (0..transfer.chunks.len())
            .map(|index| format!("transfer-{id}-{index}.bin"))
            .chain(Some(format!("transfer-{id}.json")))
        {

            match fs::remove_file(directory.join(name)) {

                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),

            }

        }
        Ok(())

    }

    pub(crate) fn prepare_transfer( &self, path: &str, expected: Option<&str>, sha256: &str, chunks: Vec<Chunk>, ) -> Result<Value, FileError> {

        let _lock = self.lock("transfer.lock")?;
        let transfer =
            Transfer { path: path.into(), expected: expected.map(str::to_owned), sha256: sha256.into(), chunks };
        transfer.validate(self)?;
        let current = match self.read(path) {

            Ok(bytes) => Some(bytes),
            Err(FileError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),

        };
        let current_hash = current.as_deref().map(digest);
        if current_hash.as_deref() == Some(sha256) {

            for entry in fs::read_dir(self.metadata_dir()?)? {

                let entry = entry?;
                let name = entry.file_name();
                if let Some(id) = name
                    .to_str()
                    .and_then(|name| name.strip_prefix("transfer-"))
                    .and_then(|name| name.strip_suffix(".json"))
                {

                    let saved = self.transfer(id)?;
                    if saved.path == path {

                        self.clear_transfer(id, &saved)?;

                    }

                }

            }
            return Ok(json!({"already_committed":true,"missing":[]}));

        }
        if current_hash.as_deref() != expected {

            return Err(FileError::Conflict);

        }
        let serialized = serde_json::to_vec(&transfer).map_err(|_| FileError::Data)?;
        let id = digest(&serialized);
        let mut active = 0;
        for entry in fs::read_dir(self.metadata_dir()?)? {

            let entry = entry?;
            let name = entry.file_name();
            if let Some(other) = name
                .to_str()
                .and_then(|name| name.strip_prefix("transfer-"))
                .and_then(|name| name.strip_suffix(".json"))
            {

                let previous = self.transfer(other)?;
                if previous.path == path && other != id {

                    self.clear_transfer(other, &previous)?;

                } else if other != id {

                    active += 1;

                }

            }

        }
        if active >= 4 {

            return Err(FileError::Limit);

        }
        self.save_metadata(&format!("transfer-{id}.json"), &serialized)?;
        let mut reusable = BTreeMap::new();
        if let Some(content) = &current {

            for chunk in content.chunks(65536) {

                reusable.insert(digest(chunk), chunk);

            }

        }
        // rename은 같은 project의 완성된 content를 재사용하고 network로 다시 보내지 않아.
        let renamed = if current.is_none() {

            match self.list()?.iter().find(|entry| entry.sha256 == sha256) {

                Some(entry) => Some(self.read(&entry.path)?),
                None => None,

            }

        } else {

            None

        };
        if let Some(content) = &renamed {

            for chunk in content.chunks(65536) {

                reusable.insert(digest(chunk), chunk);

            }

        }
        let mut missing = Vec::new();
        for (index, chunk) in transfer.chunks.iter().enumerate() {

            if self.staged(&id, index, chunk)?.is_none() {

                if let Some(bytes) = reusable.get(&chunk.sha256).filter(|bytes| bytes.len() == chunk.bytes) {

                    self.save_metadata(&format!("transfer-{id}-{index}.bin"), bytes)?;

                } else {

                    missing.push(index);

                }

            }

        }
        Ok(json!({"transfer_id":id,"already_committed":false,"missing":missing}))

    }

    pub(crate) fn put_chunk( &self, id: &str, index: usize, bytes: &[u8], ) -> Result<(), FileError> {

        let _lock = self.lock("transfer.lock")?;
        let transfer = self.transfer(id)?;
        let chunk = transfer.chunks.get(index).ok_or(FileError::Data)?;
        if bytes.len() != chunk.bytes || digest(bytes) != chunk.sha256 {

            return Err(FileError::Data);

        }
        self.save_metadata(&format!("transfer-{id}-{index}.bin"), bytes)

    }

    pub(crate) fn abort_transfer( &self, id: &str, ) -> Result<Value,FileError> {

        let _lock = self.lock("transfer.lock")?;
        self.clear_transfer(id, &self.transfer(id)?)?;
        Ok(json!({"aborted":true,"transfer_id":id}))

    }

    fn assemble( &self, id: &str, transfer: &Transfer, ) -> Result<Vec<u8>, FileError> {

        let mut content = Vec::with_capacity(transfer.chunks.iter().map(|chunk| chunk.bytes).sum());
        for (index, chunk) in transfer.chunks.iter().enumerate() {

            content.extend(self.staged(id, index, chunk)?.ok_or(FileError::Data)?);

        }
        if digest(&content) != transfer.sha256 {

            return Err(FileError::Data);

        }
        Ok(content)

    }

    pub(crate) fn transfer_content( &self, id: &str, ) -> Result<Vec<u8>, FileError> {

        let _lock = self.lock("transfer.lock")?;
        self.assemble(id, &self.transfer(id)?)

    }

    pub(crate) fn complete_staging( &self, path: &str, sha256: &str, ) -> Result<(),FileError> {

        let _lock = self.lock("transfer.lock")?;
        for entry in fs::read_dir(self.metadata_dir()?)? {

            let entry = entry?;
            let name = entry.file_name();
            if let Some(id) = name
                .to_str()
                .and_then(|name| name.strip_prefix("transfer-"))
                .and_then(|name| name.strip_suffix(".json"))
            {

                let transfer = self.transfer(id)?;
                if transfer.path == path && transfer.sha256 == sha256 {

                    self.clear_transfer(id, &transfer)?;

                }

            }

        }
        Ok(())

    }

    pub(crate) fn commit_transfer( &self, id: &str, ) -> Result<Value, FileError> {

        let _lock = self.lock("transfer.lock")?;
        let transfer = self.transfer(id)?;
        let content = self.assemble(id, &transfer)?;
        self.write(&transfer.path, Some(&content), transfer.expected.as_deref())?;
        self.clear_transfer(id, &transfer)?;
        Ok(json!({"path":transfer.path,"sha256":transfer.sha256,"bytes":content.len()}))

    }

}

#[cfg(test)]
#[path = "../../tests/unit/chunks.rs"]
mod tests;
