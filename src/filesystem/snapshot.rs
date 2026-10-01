/*! 실행할 source snapshot을 만들고 복사 중 변경된 project를 거부해. */

use super::{FileStore, digest, hashes};
use crate::FileError;
use std::{fs, io::Write};
use tempfile::TempDir;

impl FileStore {

    pub(crate) fn snapshot( &self, ) -> Result<(TempDir, FileStore, String), FileError> {

        let _source = self.read_lock()?;
        let entries = self.list()?;
        let revision = digest(&serde_json::to_vec(&hashes(&entries)).map_err(|_| FileError::Data)?);
        let directory = tempfile::Builder::new().prefix("build-").tempdir_in(self.metadata_dir()?)?;
        let snapshot = FileStore::new(directory.path(), self.settings.clone())?;
        for entry in &entries {

            let content = self.read(&entry.path)?;
            if digest(&content) != entry.sha256 {

                return Err(FileError::Conflict);

            }
            let path = directory.path().join(&entry.path);
            fs::create_dir_all(path.parent().ok_or(FileError::Path)?)?;
            let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&path)?;
            file.write_all(&content)?;
            file.set_permissions(fs::metadata(self.resolve(&entry.path, false)?)?.permissions())?;
            file.sync_all()?;

        }
        let copied = digest(&serde_json::to_vec(&hashes(&snapshot.list()?)).map_err(|_| FileError::Data)?);
        let latest = digest(&serde_json::to_vec(&hashes(&self.list()?)).map_err(|_| FileError::Data)?);
        if copied != revision || latest != revision {

            return Err(FileError::Conflict);

        }
        snapshot.save_metadata("build-manifest.json", &serde_json::to_vec(&entries).map_err(|_| FileError::Data)?)?;
        Ok((directory, snapshot, revision))

    }

}
