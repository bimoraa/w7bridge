/*! 양쪽 원본을 그대로 두고 충돌 사본을 metadata에 보존해. */

use super::{Peer, Session, SyncError, state::Conflict};
use crate::filesystem::digest;
impl Session {

    pub(super) async fn preserve(
        &self,
        peer: &impl Peer,
        path: &str,
        left: Option<&str>,
        right: Option<&str>,
    ) -> Result<Conflict, SyncError> {

        let local = self.content(peer, path, left, false).await?;
        let remote = self.content(peer, path, right, true).await?;
        let key = digest(&serde_json::to_vec(&(path, left, right))?);
        let snapshot = format!("conflict-{key}");
        if let Some(bytes) = local {

            self.local.save_metadata(&format!("{snapshot}-local.bin"), &bytes)?;

        }
        if let Some(bytes) = remote {

            self.local.save_metadata(&format!("{snapshot}-remote.bin"), &bytes)?;

        }
        let conflict = Conflict {

            path: path.into(),
            local_hash: left.map(str::to_owned),
            remote_hash: right.map(str::to_owned),
            snapshot,

        };
        self.local.save_metadata(&format!("{}.json", conflict.snapshot), &serde_json::to_vec(&conflict)?)?;
        Ok(conflict)

    }

}
